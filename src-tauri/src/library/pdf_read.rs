use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use parking_lot::Mutex;
use thiserror::Error;

#[derive(Clone, Default)]
pub struct PdfReadRegistry {
    active: Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>,
}

impl PdfReadRegistry {
    pub fn begin(&self, request_id: &str) -> Result<PdfReadLease, PdfReadError> {
        validate_request_id(request_id)?;
        let cancelled = Arc::new(AtomicBool::new(false));
        let mut active = self.active.lock();
        for previous in active.values() {
            previous.store(true, Ordering::Release);
        }
        active.clear();
        active.insert(request_id.to_owned(), Arc::clone(&cancelled));
        Ok(PdfReadLease {
            registry: self.clone(),
            request_id: request_id.to_owned(),
            cancelled,
            finished: false,
        })
    }

    pub fn cancel(&self, request_id: &str) -> bool {
        let active = self.active.lock();
        let Some(cancelled) = active.get(request_id) else {
            return false;
        };
        cancelled.store(true, Ordering::Release);
        true
    }
}

pub struct PdfReadLease {
    registry: PdfReadRegistry,
    request_id: String,
    cancelled: Arc<AtomicBool>,
    finished: bool,
}

impl PdfReadLease {
    pub fn is_cancelled(&self) -> bool {
        if self.cancelled.load(Ordering::Acquire) {
            return true;
        }
        let active = self.registry.active.lock();
        !active
            .get(&self.request_id)
            .is_some_and(|control| Arc::ptr_eq(control, &self.cancelled))
    }

    pub fn finish<T>(mut self, value: T) -> Result<T, PdfReadError> {
        let mut active = self.registry.active.lock();
        if self.cancelled.load(Ordering::Acquire)
            || !active
                .get(&self.request_id)
                .is_some_and(|control| Arc::ptr_eq(control, &self.cancelled))
        {
            return Err(PdfReadError::Cancelled);
        }
        active.remove(&self.request_id);
        self.finished = true;
        Ok(value)
    }
}

impl Drop for PdfReadLease {
    fn drop(&mut self) {
        if self.finished {
            return;
        }
        let mut active = self.registry.active.lock();
        if active
            .get(&self.request_id)
            .is_some_and(|control| Arc::ptr_eq(control, &self.cancelled))
        {
            active.remove(&self.request_id);
        }
    }
}

fn validate_request_id(request_id: &str) -> Result<(), PdfReadError> {
    if request_id.is_empty()
        || request_id.len() > 128
        || !request_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        Err(PdfReadError::InvalidRequest)
    } else {
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum PdfReadError {
    #[error("PDF read request identifier is invalid")]
    InvalidRequest,
    #[error("PDF read was cancelled")]
    Cancelled,
}

impl PdfReadError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidRequest => "PDF_READ_REQUEST_INVALID",
            Self::Cancelled => "PDF_READ_CANCELLED",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_read_cancels_the_previous_owner_without_affecting_itself() {
        let registry = PdfReadRegistry::default();
        let old = registry.begin("pdf-old").unwrap();
        let current = registry.begin("pdf-current").unwrap();

        assert!(old.is_cancelled());
        assert!(matches!(old.finish(()), Err(PdfReadError::Cancelled)));
        assert!(!current.is_cancelled());
        assert!(registry.cancel("pdf-current"));
        assert!(current.is_cancelled());
    }

    #[test]
    fn request_ids_are_strictly_bounded_ascii_tokens() {
        let registry = PdfReadRegistry::default();
        assert!(matches!(
            registry.begin("../pdf"),
            Err(PdfReadError::InvalidRequest)
        ));
        assert!(matches!(
            registry.begin(&"a".repeat(129)),
            Err(PdfReadError::InvalidRequest)
        ));
    }
}
