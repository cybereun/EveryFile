use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use parking_lot::Mutex;
use rusqlite::{Connection, InterruptHandle};

use super::SearchError;

#[derive(Clone)]
pub struct SearchRegistry {
    inner: Arc<Mutex<HashMap<String, Arc<SearchControl>>>>,
    interrupt: Arc<InterruptHandle>,
}

impl SearchRegistry {
    pub fn new(interrupt: Arc<InterruptHandle>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
            interrupt,
        }
    }

    pub fn begin(&self, request_id: &str) -> Result<SearchLease, SearchError> {
        validate_request_id(request_id)?;
        let control = Arc::new(SearchControl::default());
        let mut active = self.inner.lock();
        if active.contains_key(request_id) {
            return Err(SearchError::invalid_request(
                "search request identifier is already active",
            ));
        }
        for previous in active.values() {
            previous.cancel(&self.interrupt);
        }
        active.clear();
        active.insert(request_id.to_owned(), Arc::clone(&control));
        Ok(SearchLease {
            request_id: request_id.to_owned(),
            control,
            registry: self.clone(),
            finished: false,
        })
    }

    pub fn cancel(&self, request_id: &str) -> bool {
        let active = self.inner.lock();
        let Some(control) = active.get(request_id) else {
            return false;
        };
        control.cancel(&self.interrupt);
        true
    }
}

struct SearchControl {
    cancelled: AtomicBool,
    executing: AtomicBool,
    #[cfg(test)]
    progress_gate: Mutex<Option<ProgressGate>>,
}

impl Default for SearchControl {
    fn default() -> Self {
        Self {
            cancelled: AtomicBool::new(false),
            executing: AtomicBool::new(false),
            #[cfg(test)]
            progress_gate: Mutex::new(None),
        }
    }
}

impl SearchControl {
    fn cancel(&self, interrupt: &InterruptHandle) {
        self.cancelled.store(true, Ordering::Release);
        if self.executing.load(Ordering::Acquire) {
            interrupt.interrupt();
        }
    }

    fn progress_cancelled(&self) -> bool {
        #[cfg(test)]
        if let Some(gate) = self.progress_gate.lock().take() {
            if gate.entered.send(()).is_ok() {
                let _ = gate.release.recv();
            }
        }
        self.cancelled.load(Ordering::Acquire)
    }
}

pub struct SearchLease {
    request_id: String,
    control: Arc<SearchControl>,
    registry: SearchRegistry,
    finished: bool,
}

impl SearchLease {
    pub fn request_id(&self) -> &str {
        &self.request_id
    }

    pub(crate) fn ensure_current(&self) -> Result<(), SearchError> {
        if self.control.cancelled.load(Ordering::Acquire) {
            return Err(SearchError::Cancelled);
        }
        let active = self.registry.inner.lock();
        if active
            .get(&self.request_id)
            .is_some_and(|control| Arc::ptr_eq(control, &self.control))
            && !self.control.cancelled.load(Ordering::Acquire)
        {
            Ok(())
        } else {
            Err(SearchError::Cancelled)
        }
    }

    pub(crate) fn begin_execution<'a>(
        &'a self,
        connection: &'a Connection,
    ) -> Result<SearchExecution<'a>, SearchError> {
        self.ensure_current()?;
        self.control.executing.store(true, Ordering::Release);
        if let Err(error) = self.ensure_current() {
            self.control.executing.store(false, Ordering::Release);
            return Err(error);
        }
        let control = Arc::clone(&self.control);
        connection.progress_handler(1_000, Some(move || control.progress_cancelled()))?;
        Ok(SearchExecution {
            lease: self,
            connection,
        })
    }

    pub(crate) fn finish<T>(
        mut self,
        commit: impl FnOnce() -> Result<T, SearchError>,
    ) -> Result<T, SearchError> {
        let mut active = self.registry.inner.lock();
        if self.control.cancelled.load(Ordering::Acquire)
            || !active
                .get(&self.request_id)
                .is_some_and(|control| Arc::ptr_eq(control, &self.control))
        {
            return Err(SearchError::Cancelled);
        }
        let value = commit()?;
        active.remove(&self.request_id);
        self.finished = true;
        Ok(value)
    }
}

impl Drop for SearchLease {
    fn drop(&mut self) {
        if self.finished {
            return;
        }
        let mut active = self.registry.inner.lock();
        if active
            .get(&self.request_id)
            .is_some_and(|control| Arc::ptr_eq(control, &self.control))
        {
            active.remove(&self.request_id);
        }
    }
}

pub(crate) struct SearchExecution<'a> {
    lease: &'a SearchLease,
    connection: &'a Connection,
}

impl Drop for SearchExecution<'_> {
    fn drop(&mut self) {
        let _ = self.connection.progress_handler(0, None::<fn() -> bool>);
        self.lease.control.executing.store(false, Ordering::Release);
    }
}

#[cfg(test)]
struct ProgressGate {
    entered: std::sync::mpsc::SyncSender<()>,
    release: std::sync::mpsc::Receiver<()>,
}

fn validate_request_id(request_id: &str) -> Result<(), SearchError> {
    if request_id.is_empty()
        || request_id.len() > 128
        || !request_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(SearchError::invalid_request(
            "search request identifier must be 1-128 ASCII letters, numbers, '-' or '_'",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    use rusqlite::Connection;

    use super::*;

    #[test]
    fn superseding_an_executing_search_interrupts_its_sqlite_statement() {
        let connection = Arc::new(Mutex::new(Connection::open_in_memory().unwrap()));
        let interrupt = Arc::new(connection.lock().get_interrupt_handle());
        let registry = SearchRegistry::new(interrupt);
        let old = registry.begin("old-request").unwrap();
        let (entered_tx, entered_rx) = mpsc::sync_channel(0);
        let (release_tx, release_rx) = mpsc::sync_channel(0);
        let (finished_tx, finished_rx) = mpsc::sync_channel(0);

        *old.control.progress_gate.lock() = Some(ProgressGate {
            entered: entered_tx,
            release: release_rx,
        });

        let worker_connection = Arc::clone(&connection);
        let worker = thread::spawn(move || {
            let connection = worker_connection.lock();
            let _execution = old.begin_execution(&connection).unwrap();
            let query_result = connection.query_row(
                "WITH RECURSIVE count(value) AS (
                   VALUES(0)
                   UNION ALL
                   SELECT value + 1 FROM count WHERE value < 1000000000
                 )
                 SELECT sum(value) FROM count",
                [],
                |row| row.get::<_, i64>(0),
            );
            let cancellation = old.ensure_current();
            finished_tx.send((query_result, cancellation)).unwrap();
        });

        entered_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("old search never entered the SQLite statement");
        let new = registry.begin("new-request").unwrap();
        release_tx.send(()).unwrap();
        let (query_result, cancellation) = finished_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("interrupted old search did not stop promptly");

        assert!(query_result.is_err());
        assert!(matches!(cancellation, Err(SearchError::Cancelled)));
        new.finish(|| Ok(())).unwrap();
        worker.join().unwrap();
    }
}
