//! Encryption for the Cleanverse endpoints that issue things.
//!
//! Their scheme, not our choice: AES-256 in CBC mode with PKCS#7 padding and a
//! **fixed IV of sixteen zero bytes**, keyed by the base64-decoded api-key. The
//! ciphertext goes out base64-encoded as `{"data": "…"}`.
//!
//! A fixed IV means identical plaintexts produce identical ciphertexts, which
//! leaks equality between requests. That is a real weakness, and it is theirs —
//! we have to speak the protocol they defined. It matters little here in
//! practice: the bodies we encrypt carry no secrets of their own, only token
//! metadata and wallet addresses that are public on chain anyway. The api-key
//! itself is never transmitted.

use aes::cipher::{BlockEncryptMut, KeyIvInit, block_padding::Pkcs7};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;

use crate::cleanverse::types::CleanverseError;

type Encryptor = cbc::Encryptor<aes::Aes256>;

/// The all-zero initialisation vector their protocol prescribes.
const ZERO_IV: [u8; 16] = [0u8; 16];

/// Encrypts a request body the way their API expects it.
///
/// # Parameters
/// * `api_key` — the key exactly as stored, base64
/// * `plaintext` — the request JSON
///
/// # Returns
/// * `Ok(String)` — base64 ciphertext, ready to be sent as the `data` field
/// * `Err(_)` — the key is not valid base64, or is the wrong length
pub fn encrypt_body(api_key: &str, plaintext: &str) -> Result<String, CleanverseError> {
    let key = B64
        .decode(api_key.trim())
        .map_err(|e| CleanverseError::Crypto(format!("api_key is not valid base64: {e}")))?;
    if key.len() != 32 {
        return Err(CleanverseError::Crypto(format!(
            "api_key decodes to {} bytes, AES-256 needs 32",
            key.len()
        )));
    }

    let cipher = Encryptor::new_from_slices(&key, &ZERO_IV)
        .map_err(|e| CleanverseError::Crypto(format!("cipher setup: {e}")))?;
    let encrypted = cipher.encrypt_padded_vec_mut::<Pkcs7>(plaintext.as_bytes());
    Ok(B64.encode(encrypted))
}
