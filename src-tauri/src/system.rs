//! Windows integration for the system settings shown in the settings dialog.

use thiserror::Error;

/// Synchronizes the Windows per-user startup entry with the saved setting.
///
/// The non-Windows implementation intentionally succeeds without doing
/// anything so the settings repository remains portable for development and
/// tests. The shipped EveryFile build targets Windows.
pub fn apply_startup(enabled: bool) -> Result<(), SystemError> {
    #[cfg(windows)]
    {
        return windows_startup::apply(enabled);
    }

    #[cfg(not(windows))]
    {
        let _ = enabled;
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum SystemError {
    #[error("could not determine the EveryFile executable path: {0}")]
    CurrentExecutable(#[source] std::io::Error),
    #[error("Windows startup registration failed while {operation} (code {code})")]
    Registry { operation: &'static str, code: u32 },
}

#[cfg(windows)]
mod windows_startup {
    use std::os::windows::ffi::OsStrExt;

    use windows::core::PCWSTR;
    use windows::Win32::Foundation::ERROR_FILE_NOT_FOUND;
    use windows::Win32::System::Registry::{
        RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegOpenKeyExW, RegSetValueExW, HKEY,
        HKEY_CURRENT_USER, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ,
    };

    use super::SystemError;

    const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    const VALUE_NAME: &str = "EveryFile";

    pub(super) fn apply(enabled: bool) -> Result<(), SystemError> {
        if enabled {
            enable()
        } else {
            disable()
        }
    }

    fn enable() -> Result<(), SystemError> {
        let executable = std::env::current_exe().map_err(SystemError::CurrentExecutable)?;
        let command = format!("\"{}\"", executable.display());
        let key = create_run_key()?;
        let value_name = wide(VALUE_NAME);
        let value = wide(&command);
        let bytes =
            unsafe { std::slice::from_raw_parts(value.as_ptr().cast::<u8>(), value.len() * 2) };
        let result =
            unsafe { RegSetValueExW(key, PCWSTR(value_name.as_ptr()), None, REG_SZ, Some(bytes)) };
        unsafe {
            let _ = RegCloseKey(key);
        }
        if result.0 != 0 {
            return Err(SystemError::Registry {
                operation: "writing the startup command",
                code: result.0,
            });
        }
        Ok(())
    }

    fn disable() -> Result<(), SystemError> {
        let subkey = wide(RUN_KEY);
        let mut key = HKEY::default();
        let result = unsafe {
            RegOpenKeyExW(
                HKEY_CURRENT_USER,
                PCWSTR(subkey.as_ptr()),
                None,
                KEY_SET_VALUE,
                &mut key,
            )
        };
        if result == ERROR_FILE_NOT_FOUND {
            return Ok(());
        }
        if result.0 != 0 {
            return Err(SystemError::Registry {
                operation: "opening the startup key",
                code: result.0,
            });
        }

        let value_name = wide(VALUE_NAME);
        let delete_result = unsafe { RegDeleteValueW(key, PCWSTR(value_name.as_ptr())) };
        unsafe {
            let _ = RegCloseKey(key);
        }
        if delete_result.0 != 0 && delete_result != ERROR_FILE_NOT_FOUND {
            return Err(SystemError::Registry {
                operation: "removing the startup command",
                code: delete_result.0,
            });
        }
        Ok(())
    }

    fn create_run_key() -> Result<HKEY, SystemError> {
        let subkey = wide(RUN_KEY);
        let mut key = HKEY::default();
        let result = unsafe {
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                PCWSTR(subkey.as_ptr()),
                None,
                PCWSTR::null(),
                REG_OPTION_NON_VOLATILE,
                KEY_SET_VALUE,
                None,
                &mut key,
                None,
            )
        };
        if result.0 != 0 {
            return Err(SystemError::Registry {
                operation: "creating the startup key",
                code: result.0,
            });
        }
        Ok(key)
    }

    fn wide(value: &str) -> Vec<u16> {
        std::ffi::OsStr::new(value)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }
}
