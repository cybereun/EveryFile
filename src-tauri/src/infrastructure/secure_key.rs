use std::fs;
use std::io;
use std::path::Path;

use thiserror::Error;
use zeroize::{Zeroize, ZeroizeOnDrop};

const KEY_FILE_NAME: &str = "key.dat";
const KEY_LENGTH: usize = 32;

#[derive(Zeroize, ZeroizeOnDrop)]
pub struct SecretKey([u8; KEY_LENGTH]);

impl SecretKey {
    pub fn from_bytes(bytes: [u8; KEY_LENGTH]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; KEY_LENGTH] {
        &self.0
    }
}

pub struct SecureKeyStore;

impl SecureKeyStore {
    pub fn load_or_create(app_data_dir: &Path) -> Result<SecretKey, SecureKeyError> {
        fs::create_dir_all(app_data_dir)?;
        let key_path = app_data_dir.join(KEY_FILE_NAME);

        if key_path.exists() {
            let protected = fs::read(key_path)?;
            return unprotect(&protected);
        }

        let mut bytes = [0_u8; KEY_LENGTH];
        rand::fill(&mut bytes);
        let key = SecretKey::from_bytes(bytes);
        let protected = protect(key.as_bytes())?;
        fs::write(key_path, protected)?;
        Ok(key)
    }
}

#[cfg(windows)]
fn protect(plaintext: &[u8]) -> Result<Vec<u8>, SecureKeyError> {
    use windows::core::w;
    use windows::Win32::Security::Cryptography::{CryptProtectData, CRYPTPROTECT_UI_FORBIDDEN};

    let input = data_blob(plaintext)?;
    let mut output = LocalDataBlob::default();

    unsafe {
        CryptProtectData(
            &input,
            w!("EveryFile index key"),
            None,
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output.0,
        )?;
        Ok(output.as_slice().to_vec())
    }
}

#[cfg(windows)]
fn unprotect(protected: &[u8]) -> Result<SecretKey, SecureKeyError> {
    use windows::Win32::Security::Cryptography::{CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN};

    let input = data_blob(protected)?;
    let mut output = LocalDataBlob::default();

    unsafe {
        CryptUnprotectData(
            &input,
            None,
            None,
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output.0,
        )?;

        let plaintext = output.as_mut_slice();
        if plaintext.len() != KEY_LENGTH {
            let actual = plaintext.len();
            plaintext.zeroize();
            return Err(SecureKeyError::InvalidKeyLength(actual));
        }

        let mut bytes = [0_u8; KEY_LENGTH];
        bytes.copy_from_slice(plaintext);
        plaintext.zeroize();
        Ok(SecretKey::from_bytes(bytes))
    }
}

#[cfg(windows)]
fn data_blob(
    bytes: &[u8],
) -> Result<windows::Win32::Security::Cryptography::CRYPT_INTEGER_BLOB, SecureKeyError> {
    use windows::Win32::Security::Cryptography::CRYPT_INTEGER_BLOB;

    Ok(CRYPT_INTEGER_BLOB {
        cbData: u32::try_from(bytes.len()).map_err(|_| SecureKeyError::InputTooLarge)?,
        pbData: bytes.as_ptr().cast_mut(),
    })
}

#[cfg(windows)]
#[derive(Default)]
struct LocalDataBlob(windows::Win32::Security::Cryptography::CRYPT_INTEGER_BLOB);

#[cfg(windows)]
impl LocalDataBlob {
    unsafe fn as_slice(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.0.pbData, self.0.cbData as usize) }
    }

    unsafe fn as_mut_slice(&mut self) -> &mut [u8] {
        unsafe { std::slice::from_raw_parts_mut(self.0.pbData, self.0.cbData as usize) }
    }
}

#[cfg(windows)]
impl Drop for LocalDataBlob {
    fn drop(&mut self) {
        if !self.0.pbData.is_null() {
            use windows::Win32::Foundation::{LocalFree, HLOCAL};

            unsafe {
                let _ = LocalFree(Some(HLOCAL(self.0.pbData.cast())));
            }
        }
    }
}

#[cfg(not(windows))]
fn protect(_plaintext: &[u8]) -> Result<Vec<u8>, SecureKeyError> {
    Err(SecureKeyError::UnsupportedPlatform)
}

#[cfg(not(windows))]
fn unprotect(_protected: &[u8]) -> Result<SecretKey, SecureKeyError> {
    Err(SecureKeyError::UnsupportedPlatform)
}

#[derive(Debug, Error)]
pub enum SecureKeyError {
    #[error("secure key storage I/O failed")]
    Io(#[from] io::Error),
    #[cfg(windows)]
    #[error("Windows DPAPI operation failed")]
    Dpapi(#[from] windows::core::Error),
    #[error("DPAPI returned a key with an invalid length: {0}")]
    InvalidKeyLength(usize),
    #[cfg(windows)]
    #[error("secure key input is too large for Windows DPAPI")]
    InputTooLarge,
    #[cfg(not(windows))]
    #[error("secure key storage requires Windows DPAPI")]
    UnsupportedPlatform,
}
