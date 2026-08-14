//! Cryptographic primitives for WeCom Smart Bot HTTP URL callbacks.
//!
//! Byte-for-byte parity target: the official WeCom SDK
//! (`WecomTeam/aibot-node-sdk`, `src/wecom-crypto/index.ts`):
//!
//! 1. EncodingAESKey: `trim`, append `=` if missing, base64 decode,
//!    result must be exactly 32 bytes.
//! 2. IV = `aes_key[0..16]`.
//! 3. AES-256-CBC with auto padding = false (raw decrypt, never
//!    `block_padding::Pkcs7`, never `decrypt_padded*`).
//! 4. Ciphertext = standard base64 of the AES output; the HTTP query
//!    layer must URL percent decode first and use the SAME decoded
//!    value for signature verification and decryption.
//! 5. PKCS#7 with block size **32** (not the AES block size 16):
//!    `pad = last_byte`, `1 <= pad <= 32`, the trailing `pad` bytes
//!    must all equal `pad`, then truncate.
//! 6. Frame: `random[0..16]`, `msg_len[16..20]` BIG-ENDIAN UInt32,
//!    `message[20 .. 20+msg_len]`, trailing bytes = receiveId.
//!    Requires `decrypted.length >= 20` and `msg_end <= length`.
//! 7. receiveId is OPTIONAL: it is only validated when the expected
//!    receiveId is non-empty.
//!
//! Every failure is reported as a structured [`WecomCryptoError`] plus
//! a [`CryptoStageReport`] so the real GET path can log the exact
//! failing stage instead of a collapsed `DecryptFailed`.

use aes::cipher::{Block, BlockDecrypt, KeyInit};
use aes::Aes256;
use base64::engine::general_purpose::{
    GeneralPurpose, PAD, STANDARD as BASE64,
};
use base64::engine::Engine as _;
use base64::alphabet;

/// Base64 engine for the EncodingAESKey, mirroring the official
/// WecomTeam/aibot-node-sdk `Buffer.from(withPadding, "base64")`:
/// STANDARD alphabet + padding, but UNUSED TRAILING BITS ARE IGNORED.
///
/// The plain [`base64::engine::general_purpose::STANDARD`] engine uses
/// `decode_allow_trailing_bits = false` + `DecodePaddingMode::RequireCanonical`
/// and rejects the 43rd key character when its low 2 padding bits are
/// non-zero (`DecodeError::InvalidLastSymbol`). Node's `Buffer.from`
/// (official SDK) and .NET `Convert.FromBase64String` both decode such
/// keys to the same 32 bytes, so strict-canonical decoding diverges
/// from the official SDK and turns a valid WeCom key into
/// `EncodingKeyDecodeFailed`.
const WECOM_ENCODING_KEY_BASE64: GeneralPurpose =
    GeneralPurpose::new(&alphabet::STANDARD, PAD.with_decode_allow_trailing_bits(true));

/// Fine-grained, secret-free failure reason for the EncodingAESKey
/// decoder (Phase 2A.1.4 P3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncodingKeyDecodeError {
    /// Empty after `trim()`.
    Missing,
    /// Not valid standard base64 (bad characters / length / padding).
    InvalidBase64,
    /// Valid base64 but the decoded length is not exactly 32 bytes.
    DecodedLengthMismatch,
}

impl EncodingKeyDecodeError {
    pub fn as_str(&self) -> &'static str {
        match self {
            EncodingKeyDecodeError::Missing => "Missing",
            EncodingKeyDecodeError::InvalidBase64 => "InvalidBase64",
            EncodingKeyDecodeError::DecodedLengthMismatch => "DecodedLengthMismatch",
        }
    }
}

impl std::fmt::Display for EncodingKeyDecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::error::Error for EncodingKeyDecodeError {}

/// Secret-free structural diagnostics for a configured EncodingAESKey.
/// Carries only counts and booleans, never the key itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EncodingKeyDiag {
    /// Character count after `trim()`.
    pub input_chars: usize,
    /// Character count after appending `=` (when missing).
    pub padded_chars: usize,
    /// Whether standard base64 decoding succeeded.
    pub base64_decode_ok: bool,
    /// Decoded byte count (0 when base64 decoding failed).
    pub decoded_bytes: usize,
    /// Decoded byte count is exactly 32.
    pub decode_valid: bool,
}

impl EncodingKeyDiag {
    /// Stable reason string for the startup `configuration_invalid` log.
    pub fn failure_reason(&self) -> Option<&'static str> {
        if self.decode_valid {
            None
        } else if self.input_chars == 0 {
            Some("encoding_aes_key_missing")
        } else if !self.base64_decode_ok {
            Some("encoding_aes_key_invalid_base64")
        } else {
            Some("encoding_aes_key_decoded_length_mismatch")
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecryptedWecomMessage {
    pub message: String,
    pub receive_id: String,
}

/// Structured decryption failure, one variant per official-SDK stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WecomCryptoError {
    /// EncodingAESKey failed trim/append/base64/32-byte validation.
    EncodingKeyDecodeFailed(EncodingKeyDecodeError),
    /// echostr (or POST `encrypt`) is not valid standard base64.
    CiphertextBase64Failed,
    /// Ciphertext length is not a multiple of 16, or AES-CBC failed.
    AesCbcFailed,
    /// PKCS#7 (block size 32) unpadding failed.
    Pkcs7Failed,
    /// Decrypted frame is shorter than 20 bytes.
    FrameTooShort,
    /// `msg_len` (big-endian UInt32) overflows or `20+msg_len > len`.
    MessageLengthInvalid,
    /// Message or trailing receiveId is not valid UTF-8.
    Utf8Failed,
    /// Expected receiveId is configured and does not match the frame.
    ReceiveIdMismatch,
}

impl std::fmt::Display for WecomCryptoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for WecomCryptoError {}

/// Outcome of the optional receiveId check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReceiveIdCheck {
    /// No expected receiveId configured: trailing bytes not validated.
    #[default]
    Skipped,
    /// Expected receiveId configured and matched the frame.
    Valid,
    /// Expected receiveId configured but did not match (or the
    /// trailing bytes were not valid UTF-8).
    Invalid,
}

impl ReceiveIdCheck {
    pub fn as_str(&self) -> &'static str {
        match self {
            ReceiveIdCheck::Skipped => "skipped",
            ReceiveIdCheck::Valid => "valid",
            ReceiveIdCheck::Invalid => "invalid",
        }
    }
}

/// Per-stage success flags for diagnostics. Never contains secret data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CryptoStageReport {
    pub aes_key_decode_ok: bool,
    /// Secret-free EncodingAESKey structural diagnostics (P3):
    /// character counts, base64 result and decoded byte count only.
    pub encoding_key_input_chars: usize,
    pub encoding_key_padded_chars: usize,
    pub encoding_key_base64_decode_ok: bool,
    pub encoding_key_decoded_bytes: usize,
    pub ciphertext_decode_ok: bool,
    pub aes_cbc_ok: bool,
    pub pkcs7_ok: bool,
    pub frame_parse_ok: bool,
    pub message_extract_ok: bool,
    pub receive_id_check: ReceiveIdCheck,
}

/// Decryption result together with the stage report used by the HTTP
/// layer to emit `[wecom-http] crypto_stage` diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecryptOutcome {
    pub result: Result<DecryptedWecomMessage, WecomCryptoError>,
    pub stages: CryptoStageReport,
}

pub fn verify_signature(
    token: &str,
    timestamp: &str,
    nonce: &str,
    encrypted: &str,
    signature: &str,
) -> bool {
    let mut parts = [token, timestamp, nonce, encrypted];
    parts.sort_unstable();
    let joined = parts.concat();
    let expected = hex::encode(sha1_digest(joined.as_bytes()));
    constant_time_eq(expected.as_bytes(), signature.as_bytes())
}

/// Convenience wrapper returning only the result. The real GET path
/// uses [`decrypt_message_with_report`] so stage diagnostics survive.
pub fn decrypt_message(
    encoding_aes_key: &str,
    encrypted: &str,
    expected_receive_id: &str,
) -> Result<DecryptedWecomMessage, WecomCryptoError> {
    decrypt_message_with_report(encoding_aes_key, encrypted, expected_receive_id).result
}

/// Decrypt with a full per-stage report, mirroring the official SDK
/// stage by stage:
///
/// key decode → base64 → AES-256-CBC (no padding) → PKCS#7-32 →
/// frame parse (20+len bounds) → UTF-8 extract → optional receiveId.
pub fn decrypt_message_with_report(
    encoding_aes_key: &str,
    encrypted: &str,
    expected_receive_id: &str,
) -> DecryptOutcome {
    let mut stages = CryptoStageReport::default();

    // Stage 1: EncodingAESKey (official: trim, append '=', 32 bytes;
    // base64 decode ignores unused trailing bits like Node/.NET).
    let (key_result, key_diag) = decode_encoding_aes_key_with_diag(encoding_aes_key);
    stages.encoding_key_input_chars = key_diag.input_chars;
    stages.encoding_key_padded_chars = key_diag.padded_chars;
    stages.encoding_key_base64_decode_ok = key_diag.base64_decode_ok;
    stages.encoding_key_decoded_bytes = key_diag.decoded_bytes;
    let key = match key_result {
        Ok(key) => {
            stages.aes_key_decode_ok = true;
            key
        }
        Err(error) => {
            return DecryptOutcome {
                result: Err(WecomCryptoError::EncodingKeyDecodeFailed(error)),
                stages,
            };
        }
    };

    // Stage 2: standard base64 of the (already URL-decoded) ciphertext.
    let ciphertext = match BASE64.decode(encrypted) {
        Ok(bytes) if !bytes.is_empty() => {
            stages.ciphertext_decode_ok = true;
            bytes
        }
        Ok(_) | Err(_) => {
            return DecryptOutcome {
                result: Err(WecomCryptoError::CiphertextBase64Failed),
                stages,
            };
        }
    };

    // Stage 3: AES-256-CBC raw decrypt, auto padding = false.
    if ciphertext.len() % 16 != 0 {
        return DecryptOutcome {
            result: Err(WecomCryptoError::AesCbcFailed),
            stages,
        };
    }
    let decrypted = decrypt_aes256_cbc(&ciphertext, &key);
    stages.aes_cbc_ok = true;

    // Stage 4: PKCS#7 with block size 32 (NOT the AES block size 16).
    let unpadded = match remove_pkcs7_32_padding(&decrypted) {
        Ok(value) => {
            stages.pkcs7_ok = true;
            value
        }
        Err(()) => {
            return DecryptOutcome {
                result: Err(WecomCryptoError::Pkcs7Failed),
                stages,
            };
        }
    };

    // Stage 5: frame parse — random[0..16], msg_len[16..20] BE UInt32.
    if unpadded.len() < 20 {
        return DecryptOutcome {
            result: Err(WecomCryptoError::FrameTooShort),
            stages,
        };
    }
    let message_len_bytes: [u8; 4] = match unpadded[16..20].try_into() {
        Ok(bytes) => bytes,
        Err(_) => {
            return DecryptOutcome {
                result: Err(WecomCryptoError::FrameTooShort),
                stages,
            };
        }
    };
    let message_len = u32::from_be_bytes(message_len_bytes) as usize;
    let message_end = match 20usize.checked_add(message_len) {
        Some(end) => end,
        None => {
            return DecryptOutcome {
                result: Err(WecomCryptoError::MessageLengthInvalid),
                stages,
            };
        }
    };
    if message_end > unpadded.len() {
        return DecryptOutcome {
            result: Err(WecomCryptoError::MessageLengthInvalid),
            stages,
        };
    }
    stages.frame_parse_ok = true;

    // Stage 6: extract message and trailing receiveId as UTF-8.
    let message = match std::str::from_utf8(&unpadded[20..message_end]) {
        Ok(value) => value.to_string(),
        Err(_) => {
            return DecryptOutcome {
                result: Err(WecomCryptoError::Utf8Failed),
                stages,
            };
        }
    };
    stages.message_extract_ok = true;
    let receive_id = match std::str::from_utf8(&unpadded[message_end..]) {
        Ok(value) => value.to_string(),
        Err(_) => {
            stages.receive_id_check = ReceiveIdCheck::Invalid;
            return DecryptOutcome {
                result: Err(WecomCryptoError::Utf8Failed),
                stages,
            };
        }
    };

    // Stage 7: receiveId is optional — validate only when configured.
    if expected_receive_id.is_empty() {
        stages.receive_id_check = ReceiveIdCheck::Skipped;
    } else if receive_id == expected_receive_id {
        stages.receive_id_check = ReceiveIdCheck::Valid;
    } else {
        stages.receive_id_check = ReceiveIdCheck::Invalid;
        return DecryptOutcome {
            result: Err(WecomCryptoError::ReceiveIdMismatch),
            stages,
        };
    }

    DecryptOutcome {
        result: Ok(DecryptedWecomMessage {
            message,
            receive_id,
        }),
        stages,
    }
}

/// Official EncodingAESKey decoding (WecomTeam/aibot-node-sdk
/// `decodeEncodingAESKey`):
///
/// 1. `trim()`
/// 2. empty check → `Missing`
/// 3. append `=` when the trimmed value does not end with `=`
/// 4. STANDARD-alphabet base64 decode, unused trailing bits ignored
///    (Node `Buffer.from(..., "base64")` / .NET `Convert.FromBase64String`
///    parity — a strict-canonical engine rejects valid WeCom keys)
/// 5. decoded length must be exactly 32 bytes → `DecodedLengthMismatch`
///
/// Never: URL_SAFE, NO_PAD, strict `% 4 == 0` pre-checks, a 44-char
/// requirement, or using the 43 ASCII bytes directly as the AES key.
pub(crate) fn decode_encoding_aes_key(
    value: &str,
) -> Result<[u8; 32], EncodingKeyDecodeError> {
    let (result, _) = decode_encoding_aes_key_with_diag(value);
    result
}

/// Decoder plus secret-free structural diagnostics (input/padded char
/// counts, base64 result, decoded byte count).
pub(crate) fn decode_encoding_aes_key_with_diag(
    value: &str,
) -> (Result<[u8; 32], EncodingKeyDecodeError>, EncodingKeyDiag) {
    let trimmed = value.trim();
    let input_chars = trimmed.chars().count();
    let padded = if trimmed.ends_with('=') {
        trimmed.to_string()
    } else {
        format!("{trimmed}=")
    };
    let padded_chars = padded.chars().count();

    if input_chars == 0 {
        let diag = EncodingKeyDiag {
            input_chars,
            padded_chars,
            base64_decode_ok: false,
            decoded_bytes: 0,
            decode_valid: false,
        };
        return (Err(EncodingKeyDecodeError::Missing), diag);
    }

    match WECOM_ENCODING_KEY_BASE64.decode(&padded) {
        Ok(decoded) => {
            let decode_valid = decoded.len() == 32;
            let diag = EncodingKeyDiag {
                input_chars,
                padded_chars,
                base64_decode_ok: true,
                decoded_bytes: decoded.len(),
                decode_valid,
            };
            if !decode_valid {
                return (
                    Err(EncodingKeyDecodeError::DecodedLengthMismatch),
                    diag,
                );
            }
            let mut key = [0u8; 32];
            key.copy_from_slice(&decoded);
            (Ok(key), diag)
        }
        Err(_) => {
            let diag = EncodingKeyDiag {
                input_chars,
                padded_chars,
                base64_decode_ok: false,
                decoded_bytes: 0,
                decode_valid: false,
            };
            (Err(EncodingKeyDecodeError::InvalidBase64), diag)
        }
    }
}

/// Secret-free structural check of a configured EncodingAESKey string,
/// used by the HTTP-callback startup validation and GET diagnostics.
/// Never returns the key bytes or any hash of them.
pub(crate) fn encoding_key_diag(value: &str) -> EncodingKeyDiag {
    let (_, diag) = decode_encoding_aes_key_with_diag(value);
    diag
}

/// Raw AES-256-CBC decrypt with IV = `key[0..16]` and NO padding
/// removal. Equivalent to Node `createDecipheriv('aes-256-cbc', key,
/// iv)` with `setAutoPadding(false)`.
fn decrypt_aes256_cbc(ciphertext: &[u8], key: &[u8; 32]) -> Vec<u8> {
    let cipher = Aes256::new_from_slice(key).expect("AES-256 key is exactly 32 bytes");
    let mut previous = key[..16].to_vec();
    let mut output = Vec::with_capacity(ciphertext.len());
    for chunk in ciphertext.chunks_exact(16) {
        let mut bytes = [0u8; 16];
        bytes.copy_from_slice(chunk);
        let mut block = Block::<Aes256>::from(bytes);
        cipher.decrypt_block(&mut block);
        for index in 0..16 {
            output.push(block[index] ^ previous[index]);
        }
        previous.copy_from_slice(chunk);
    }
    output
}

/// Manual PKCS#7 unpadding with block size 32 — the official WeCom
/// block size. Deliberately NOT the AES block size 16 and NOT
/// `block_padding::Pkcs7`.
fn remove_pkcs7_32_padding(value: &[u8]) -> Result<&[u8], ()> {
    let padding = *value.last().ok_or(())? as usize;
    if padding == 0 || padding > 32 || padding > value.len() {
        return Err(());
    }
    if !value[value.len() - padding..]
        .iter()
        .all(|byte| *byte as usize == padding)
    {
        return Err(());
    }
    Ok(&value[..value.len() - padding])
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0u8, |diff, (a, b)| diff | (a ^ b))
        == 0
}

fn sha1_digest(input: &[u8]) -> [u8; 20] {
    let bit_len = (input.len() as u64) * 8;
    let mut padded = input.to_vec();
    padded.push(0x80);
    while padded.len() % 64 != 56 {
        padded.push(0);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());

    let mut h0 = 0x6745_2301u32;
    let mut h1 = 0xefcd_ab89u32;
    let mut h2 = 0x98ba_dcfeu32;
    let mut h3 = 0x1032_5476u32;
    let mut h4 = 0xc3d2_e1f0u32;
    for chunk in padded.chunks_exact(64) {
        let mut words = [0u32; 80];
        for (index, word) in words.iter_mut().take(16).enumerate() {
            let offset = index * 4;
            *word = u32::from_be_bytes(chunk[offset..offset + 4].try_into().unwrap());
        }
        for index in 16..80 {
            words[index] = (words[index - 3]
                ^ words[index - 8]
                ^ words[index - 14]
                ^ words[index - 16])
                .rotate_left(1);
        }

        let (mut a, mut b, mut c, mut d, mut e) = (h0, h1, h2, h3, h4);
        for (index, word) in words.iter().enumerate() {
            let (f, k) = match index {
                0..=19 => ((b & c) | ((!b) & d), 0x5a82_7999),
                20..=39 => (b ^ c ^ d, 0x6ed9_eba1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8f1b_bcdc),
                _ => (b ^ c ^ d, 0xca62_c1d6),
            };
            let temp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(*word);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }
        h0 = h0.wrapping_add(a);
        h1 = h1.wrapping_add(b);
        h2 = h2.wrapping_add(c);
        h3 = h3.wrapping_add(d);
        h4 = h4.wrapping_add(e);
    }

    let mut output = [0u8; 20];
    for (index, value) in [h0, h1, h2, h3, h4].iter().enumerate() {
        output[index * 4..index * 4 + 4].copy_from_slice(&value.to_be_bytes());
    }
    output
}

#[cfg(test)]
pub(crate) fn encrypt_test_fixture(
    encoding_aes_key: &str,
    message: &str,
    receive_id: &str,
) -> String {
    let key = decode_encoding_aes_key(encoding_aes_key).unwrap();
    let mut plaintext = vec![7u8; 16];
    plaintext.extend_from_slice(&(message.len() as u32).to_be_bytes());
    plaintext.extend_from_slice(message.as_bytes());
    plaintext.extend_from_slice(receive_id.as_bytes());
    let padding = 32 - (plaintext.len() % 32);
    plaintext.extend(std::iter::repeat_n(padding as u8, padding));
    encrypt_raw_bytes(&key, &plaintext)
}

/// Raw AES-256-CBC encrypt of an already-padded plaintext (length must
/// be a multiple of 16). Test helper mirroring the official algorithm.
#[cfg(test)]
pub(crate) fn encrypt_raw_bytes(key: &[u8; 32], plaintext: &[u8]) -> String {
    use aes::cipher::BlockEncrypt;

    assert_eq!(plaintext.len() % 16, 0, "test plaintext must be block-aligned");
    let cipher = Aes256::new_from_slice(key).unwrap();
    let mut previous = key[..16].to_vec();
    let mut encrypted = Vec::with_capacity(plaintext.len());
    for chunk in plaintext.chunks_exact(16) {
        let mut bytes = [0u8; 16];
        for index in 0..16 {
            bytes[index] = chunk[index] ^ previous[index];
        }
        let mut block = Block::<Aes256>::from(bytes);
        cipher.encrypt_block(&mut block);
        encrypted.extend_from_slice(&block);
        previous.copy_from_slice(&block);
    }
    BASE64.encode(encrypted)
}

/// Build a WeCom frame `random(16) + msg_len(BE32) + message + receiveId`
/// padded with PKCS#7 (block 32) to a multiple of 16, then AES-encrypt.
#[cfg(test)]
pub(crate) fn encrypt_frame_fixture(
    encoding_aes_key: &str,
    random: &[u8; 16],
    message: &[u8],
    receive_id: &[u8],
) -> String {
    let key = decode_encoding_aes_key(encoding_aes_key).unwrap();
    let mut plaintext = Vec::with_capacity(20 + message.len() + receive_id.len());
    plaintext.extend_from_slice(random);
    plaintext.extend_from_slice(&(message.len() as u32).to_be_bytes());
    plaintext.extend_from_slice(message);
    plaintext.extend_from_slice(receive_id);
    let padding = 32 - (plaintext.len() % 32);
    plaintext.extend(std::iter::repeat_n(padding as u8, padding));
    encrypt_raw_bytes(&key, &plaintext)
}

#[cfg(test)]
pub(crate) fn signature_for_test(joined_sorted_values: &str) -> String {
    hex::encode(sha1_digest(joined_sorted_values.as_bytes()))
}

#[cfg(test)]
pub(crate) const TEST_ENCODING_AES_KEY: &str =
    "MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3ODlhYmNkZWY";

// =========================================================================
// Official parity fixtures
// =========================================================================
//
// Generated with an independent Node.js reference implementation of the
// official WeCom algorithm (`WecomTeam/aibot-node-sdk` crypto):
//   AES-256-CBC, autoPadding=false, IV=key[0..16], PKCS#7 block=32,
//   frame=random(16)+msg_len(BE32)+message+receiveId.
// They do NOT reuse any Rust encrypt helper, so the Rust decrypt/sign
// paths are validated against externally fixed byte strings.

#[cfg(test)]
pub(crate) const PARITY_TOKEN: &str = "parity-token-2a13";
#[cfg(test)]
pub(crate) const PARITY_TIMESTAMP: &str = "1700000000";
#[cfg(test)]
pub(crate) const PARITY_NONCE: &str = "parity-nonce";
#[cfg(test)]
pub(crate) const PARITY_ENCODING_AES_KEY: &str =
    "MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3ODlhYmNkZWY";
#[cfg(test)]
pub(crate) const PARITY_RECEIVE_ID: &str = "corp-parity";

/// Fixture 1: full JSON message, receiveId = "corp-parity",
/// random = 00..0F, PKCS#7 pad = 5.
#[cfg(test)]
pub(crate) const PARITY_MESSAGE_1: &str = r#"{"msgid":"parity-msg-1","aibotid":"parity-bot","chattype":"single","from":{"userid":"parity-user"},"msgtype":"text","text":{"content":"official sdk parity"}}"#;
#[cfg(test)]
pub(crate) const PARITY_CIPHERTEXT_1: &str = "oChxUuEMxdC3iAHYNh2h5SRr8wx0O0BbpXMPNFCcO+BjH8/CgfPyHsu+9nExTARoRXq6z/L2FQ6O28j///VbwnrL+eRsL49KlPjXr7V5Rkeln+j2dSG8pA635YhHQNidKajnKhwUMTNPfLfgQ2y6hDYwCEShTBn/q2tWoxG/wD5e6YffTgozwzhMmHqxwKomXSGbCQw1sG/CmG4HJ5hEVx65Y/Q8fTYSATIS4YtRSIY8vzEgwEWh86Iugbxhf2JF";
#[cfg(test)]
pub(crate) const PARITY_SIGNATURE_1: &str = "6fb8d095d610426b296492d61463bed85e3731a8";

/// Fixture 2: 27-byte message, receiveId = "" (optional),
/// random = 22 33 44 55 66 77 88 99 aa bb cc dd ee ff 10 21,
/// frame = 47 bytes → PKCS#7 pad = **17** (17..=32 window, which a
/// PKCS#7-16 implementation cannot accept). Base64 contains `+`, `/`
/// and `=` so it also exercises URL percent decoding.
#[cfg(test)]
pub(crate) const PARITY_MESSAGE_2: &str = r#"{"msg":"abcdefghijklmnopq"}"#;
#[cfg(test)]
pub(crate) const PARITY_CIPHERTEXT_2: &str = "E6Psz4hL9EOL50aolGiiZCGMIJ9ge9cThhl5InzYkQzWLZdKbLvJVgTdLCNpvo4QljQns+/5GrerFZ0aPENbDg==";
#[cfg(test)]
pub(crate) const PARITY_SIGNATURE_2: &str = "9f7dde700b023d20192eb0e28d08e88e39ca151d";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha1_matches_standard_abc_vector() {
        assert_eq!(
            hex::encode(sha1_digest(b"abc")),
            "a9993e364706816aba3e25717850c26c9cd0d89d"
        );
    }

    #[test]
    fn signature_matches_sorted_sha1_protocol() {
        let encrypted = "ciphertext";
        let mut parts = ["token", "1700000000", "nonce", encrypted];
        parts.sort_unstable();
        let signature = hex::encode(sha1_digest(parts.concat().as_bytes()));
        assert!(verify_signature("token", "1700000000", "nonce", encrypted, &signature));
        assert!(!verify_signature("other", "1700000000", "nonce", encrypted, &signature));
    }

    #[test]
    fn decrypts_json_and_validates_empty_smart_bot_receive_id() {
        let encrypted = encrypt_test_fixture(TEST_ENCODING_AES_KEY, r#"{"msgid":"m1"}"#, "");
        let result = decrypt_message(TEST_ENCODING_AES_KEY, &encrypted, "").unwrap();
        assert_eq!(result.message, r#"{"msgid":"m1"}"#);
        assert_eq!(result.receive_id, "");
    }

    #[test]
    fn receive_id_empty_accepts_any() {
        // When expected_receive_id is empty (not configured), accept any receive_id
        let encrypted = encrypt_test_fixture(TEST_ENCODING_AES_KEY, "{}", "corp-id");
        let result = decrypt_message(TEST_ENCODING_AES_KEY, &encrypted, "");
        // Should succeed because expected_receive_id is empty
        assert!(result.is_ok());
        assert_eq!(result.unwrap().receive_id, "corp-id");
    }

    #[test]
    fn receive_id_configured_must_match() {
        // When expected_receive_id is configured, it must match exactly
        let encrypted = encrypt_test_fixture(TEST_ENCODING_AES_KEY, "{}", "corp-id");
        let result = decrypt_message(TEST_ENCODING_AES_KEY, &encrypted, "corp-id");
        // Should succeed because expected matches actual
        assert!(result.is_ok());
        assert_eq!(result.unwrap().receive_id, "corp-id");
    }

    #[test]
    fn receive_id_mismatch_rejected() {
        // When expected_receive_id is configured but doesn't match, reject
        let encrypted = encrypt_test_fixture(TEST_ENCODING_AES_KEY, "{}", "corp-id");
        let result = decrypt_message(TEST_ENCODING_AES_KEY, &encrypted, "wrong-id");
        // Should fail because expected != actual
        assert!(matches!(result, Err(WecomCryptoError::ReceiveIdMismatch)));
    }

    #[test]
    fn frame_fixture_roundtrip_official_layout() {
        let random = [0xABu8; 16];
        let encrypted = encrypt_frame_fixture(TEST_ENCODING_AES_KEY, &random, b"frame-msg", b"rid");
        let result = decrypt_message(TEST_ENCODING_AES_KEY, &encrypted, "rid").unwrap();
        assert_eq!(result.message, "frame-msg");
        assert_eq!(result.receive_id, "rid");
    }

    #[test]
    fn encoding_aes_key_whitespace_is_trimmed() {
        // Official SDK trims the key before decoding.
        let encrypted = encrypt_test_fixture(TEST_ENCODING_AES_KEY, "trimmed", "");
        let padded = format!("  {TEST_ENCODING_AES_KEY}  ");
        let result = decrypt_message(&padded, &encrypted, "").unwrap();
        assert_eq!(result.message, "trimmed");
    }

    #[test]
    fn encoding_aes_key_with_equals_suffix_accepts_44_chars() {
        // 43-char key + explicit '=' (44 chars) must decode like the
        // official SDK (append only when '=' is missing).
        let with_eq = format!("{TEST_ENCODING_AES_KEY}=");
        assert_eq!(with_eq.len(), 44);
        let encrypted = encrypt_test_fixture(TEST_ENCODING_AES_KEY, "with-equals", "");
        let result = decrypt_message(&with_eq, &encrypted, "").unwrap();
        assert_eq!(result.message, "with-equals");
    }

    #[test]
    fn encoding_aes_key_wrong_size_rejected() {
        let encrypted = encrypt_test_fixture(TEST_ENCODING_AES_KEY, "x", "");
        // "QUJDQQ==" already ends with '=' -> decodes to 4 bytes ("ABCA").
        let result = decrypt_message("QUJDQQ==", &encrypted, "");
        assert!(matches!(
            result,
            Err(WecomCryptoError::EncodingKeyDecodeFailed(
                EncodingKeyDecodeError::DecodedLengthMismatch
            ))
        ));
    }

    // ------------------------------------------------------------------
    // Phase 2A.1.4: EncodingAESKey decoder tests (P4)
    // ------------------------------------------------------------------

    #[test]
    fn wecom_encoding_key_43_char_appends_padding_decodes_32() {
        // Real WeCom form: 43 chars, no '='. Append '=' -> 32 bytes.
        assert_eq!(TEST_ENCODING_AES_KEY.chars().count(), 43);
        let key = decode_encoding_aes_key(TEST_ENCODING_AES_KEY).unwrap();
        assert_eq!(
            hex::encode(key),
            "3031323334353637383961626364656630313233343536373839616263646566"
        );
        let (_, diag) = decode_encoding_aes_key_with_diag(TEST_ENCODING_AES_KEY);
        assert_eq!(diag.input_chars, 43);
        assert_eq!(diag.padded_chars, 44);
        assert!(diag.base64_decode_ok);
        assert_eq!(diag.decoded_bytes, 32);
        assert!(diag.decode_valid);
        assert_eq!(diag.failure_reason(), None);
    }

    #[test]
    fn wecom_encoding_key_44_char_with_equals_decodes_32() {
        let with_eq = format!("{TEST_ENCODING_AES_KEY}=");
        assert_eq!(with_eq.chars().count(), 44);
        let key = decode_encoding_aes_key(&with_eq).unwrap();
        assert_eq!(
            hex::encode(key),
            "3031323334353637383961626364656630313233343536373839616263646566"
        );
        // 44-char input: no second '=' is appended.
        let (_, diag) = decode_encoding_aes_key_with_diag(&with_eq);
        assert_eq!(diag.input_chars, 44);
        assert_eq!(diag.padded_chars, 44);
        assert!(diag.decode_valid);
    }

    #[test]
    fn wecom_encoding_key_leading_trailing_whitespace_trimmed() {
        let padded = format!("  {TEST_ENCODING_AES_KEY}\t");
        let key = decode_encoding_aes_key(&padded).unwrap();
        assert_eq!(
            hex::encode(key),
            "3031323334353637383961626364656630313233343536373839616263646566"
        );
        let (_, diag) = decode_encoding_aes_key_with_diag(&padded);
        assert_eq!(diag.input_chars, 43);
        assert!(diag.decode_valid);
    }

    #[test]
    fn wecom_encoding_key_invalid_base64_reason() {
        let (result, diag) = decode_encoding_aes_key_with_diag("abc!defghi");
        assert_eq!(result, Err(EncodingKeyDecodeError::InvalidBase64));
        assert_eq!(diag.input_chars, 10);
        assert_eq!(diag.padded_chars, 11);
        assert!(!diag.base64_decode_ok);
        assert_eq!(diag.decoded_bytes, 0);
        assert!(!diag.decode_valid);
        assert_eq!(diag.failure_reason(), Some("encoding_aes_key_invalid_base64"));
    }

    #[test]
    fn wecom_encoding_key_valid_base64_wrong_decoded_length_reason() {
        // "QUJDQQ==" already ends with '=' (no padding appended) and
        // decodes to "ABCA" (4 bytes) — valid base64, wrong length.
        let (result, diag) = decode_encoding_aes_key_with_diag("QUJDQQ==");
        assert_eq!(
            result,
            Err(EncodingKeyDecodeError::DecodedLengthMismatch)
        );
        assert!(diag.base64_decode_ok);
        assert_eq!(diag.decoded_bytes, 4);
        assert!(!diag.decode_valid);
        assert_eq!(
            diag.failure_reason(),
            Some("encoding_aes_key_decoded_length_mismatch")
        );
    }

    #[test]
    fn wecom_encoding_key_missing_reason() {
        for empty in ["", "   ", "\t\n"] {
            let (result, diag) = decode_encoding_aes_key_with_diag(empty);
            assert_eq!(result, Err(EncodingKeyDecodeError::Missing));
            assert_eq!(diag.input_chars, 0);
            assert!(!diag.base64_decode_ok);
            assert_eq!(diag.decoded_bytes, 0);
            assert!(!diag.decode_valid);
            assert_eq!(diag.failure_reason(), Some("encoding_aes_key_missing"));
        }
    }

    /// Official SDK parity fixture key: the well-known WeCom sample
    /// EncodingAESKey used in the official callback documentation.
    /// Expected bytes verified independently with Node
    /// `Buffer.from(key + "=", "base64")`.
    #[test]
    fn wecom_encoding_key_official_sdk_parity_fixture_exact_bytes() {
        const OFFICIAL_SAMPLE_KEY: &str = "jWmYm7qr5nMoAUwZRjGtBxmz3KA1tkAj3ykkR6q2B2C";
        assert_eq!(OFFICIAL_SAMPLE_KEY.chars().count(), 43);
        let key = decode_encoding_aes_key(OFFICIAL_SAMPLE_KEY).unwrap();
        assert_eq!(
            hex::encode(key),
            "8d69989bbaabe67328014c194631ad0719b3dca035b64023df292447aab60760"
        );
    }

    /// ROOT-CAUSE regression: a 43-char key whose LAST character carries
    /// non-zero unused trailing bits. Node `Buffer.from` (official SDK)
    /// and .NET `Convert.FromBase64String` decode it to the same 32
    /// bytes; the strict-canonical STANDARD engine rejects it with
    /// `InvalidLastSymbol`. Our decoder must match Node/.NET.
    #[test]
    fn wecom_encoding_key_noncanonical_trailing_bits_accepted() {
        use base64::engine::Engine as _;
        // Canonical: last char 'Y' (value 24, low 2 bits 00).
        assert_eq!(TEST_ENCODING_AES_KEY.chars().last(), Some('Y'));
        // Non-canonical: same 4 data bits, low 2 bits set (24 | 2 = 26 -> 'a').
        let mut noncanonical = TEST_ENCODING_AES_KEY.to_string();
        noncanonical.pop();
        noncanonical.push('a');
        assert_eq!(noncanonical.chars().count(), 43);

        let padded = format!("{noncanonical}=");
        // Strict-canonical engine rejects it (documents the divergence)…
        assert!(base64::engine::general_purpose::STANDARD
            .decode(&padded)
            .is_err());
        // …while our official-parity decoder accepts it and produces the
        // exact same 32 bytes as the canonical key.
        let canonical_key = decode_encoding_aes_key(TEST_ENCODING_AES_KEY).unwrap();
        let noncanonical_key = decode_encoding_aes_key(&noncanonical).unwrap();
        assert_eq!(noncanonical_key, canonical_key);
        assert_eq!(
            hex::encode(noncanonical_key),
            "3031323334353637383961626364656630313233343536373839616263646566"
        );
        let (_, diag) = decode_encoding_aes_key_with_diag(&noncanonical);
        assert_eq!(diag.decoded_bytes, 32);
        assert!(diag.decode_valid);
    }

    // ------------------------------------------------------------------
    // Official byte-for-byte parity fixtures (externally generated)
    // ------------------------------------------------------------------

    #[test]
    fn wecom_official_parity_signature_passes() {
        assert!(verify_signature(
            PARITY_TOKEN,
            PARITY_TIMESTAMP,
            PARITY_NONCE,
            PARITY_CIPHERTEXT_1,
            PARITY_SIGNATURE_1,
        ));
        assert!(verify_signature(
            PARITY_TOKEN,
            PARITY_TIMESTAMP,
            PARITY_NONCE,
            PARITY_CIPHERTEXT_2,
            PARITY_SIGNATURE_2,
        ));
        assert!(!verify_signature(
            PARITY_TOKEN,
            PARITY_TIMESTAMP,
            PARITY_NONCE,
            PARITY_CIPHERTEXT_1,
            "0000000000000000000000000000000000000000",
        ));
    }

    #[test]
    fn wecom_official_parity_decrypt_exact_match() {
        let result = decrypt_message(PARITY_ENCODING_AES_KEY, PARITY_CIPHERTEXT_1, "")
            .expect("official fixture 1 must decrypt");
        assert_eq!(result.message, PARITY_MESSAGE_1);
        assert_eq!(result.receive_id, PARITY_RECEIVE_ID);
    }

    #[test]
    fn wecom_official_parity_receive_id_optional_decrypts() {
        // receiveId=None/empty -> official SDK skips the check.
        let result = decrypt_message(PARITY_ENCODING_AES_KEY, PARITY_CIPHERTEXT_1, "")
            .expect("optional receiveId must pass");
        assert_eq!(result.message, PARITY_MESSAGE_1);
    }

    #[test]
    fn wecom_official_parity_receive_id_correct_passes() {
        let result = decrypt_message(
            PARITY_ENCODING_AES_KEY,
            PARITY_CIPHERTEXT_1,
            PARITY_RECEIVE_ID,
        )
        .expect("matching receiveId must pass");
        assert_eq!(result.message, PARITY_MESSAGE_1);
    }

    #[test]
    fn wecom_official_parity_receive_id_wrong_rejected() {
        let outcome = decrypt_message_with_report(
            PARITY_ENCODING_AES_KEY,
            PARITY_CIPHERTEXT_1,
            "wrong-corp",
        );
        assert!(matches!(
            outcome.result,
            Err(WecomCryptoError::ReceiveIdMismatch)
        ));
        assert_eq!(outcome.stages.receive_id_check, ReceiveIdCheck::Invalid);
        assert!(outcome.stages.aes_key_decode_ok);
        assert!(outcome.stages.ciphertext_decode_ok);
        assert!(outcome.stages.aes_cbc_ok);
        assert!(outcome.stages.pkcs7_ok);
        assert!(outcome.stages.frame_parse_ok);
        assert!(outcome.stages.message_extract_ok);
    }

    #[test]
    fn wecom_official_parity_pkcs7_padding_17_to_32() {
        // Fixture 2: frame = 47 bytes -> PKCS#7 pad = 17.
        // A PKCS#7-16 (AES block size) implementation rejects pad 17,
        // so this test fails on any 16-byte-block unpadder.
        let outcome = decrypt_message_with_report(
            PARITY_ENCODING_AES_KEY,
            PARITY_CIPHERTEXT_2,
            "",
        );
        let result = outcome
            .result
            .expect("PKCS#7-32 fixture with pad=17 must decrypt");
        assert_eq!(result.message, PARITY_MESSAGE_2);
        assert_eq!(result.receive_id, "");
        assert!(outcome.stages.pkcs7_ok);
        assert_eq!(outcome.stages.receive_id_check, ReceiveIdCheck::Skipped);
    }

    // ------------------------------------------------------------------
    // Structured stage-report diagnostics (P4)
    // ------------------------------------------------------------------

    #[test]
    fn wecom_crypto_stage_full_success_report() {
        let outcome = decrypt_message_with_report(
            PARITY_ENCODING_AES_KEY,
            PARITY_CIPHERTEXT_2,
            "",
        );
        assert!(outcome.result.is_ok());
        assert!(outcome.stages.aes_key_decode_ok);
        assert!(outcome.stages.ciphertext_decode_ok);
        assert!(outcome.stages.aes_cbc_ok);
        assert!(outcome.stages.pkcs7_ok);
        assert!(outcome.stages.frame_parse_ok);
        assert!(outcome.stages.message_extract_ok);
        assert_eq!(outcome.stages.receive_id_check, ReceiveIdCheck::Skipped);
    }

    #[test]
    fn wecom_crypto_stage_encoding_key_decode_failed() {
        let outcome = decrypt_message_with_report("not-a-key", PARITY_CIPHERTEXT_1, "");
        assert!(matches!(
            outcome.result,
            Err(WecomCryptoError::EncodingKeyDecodeFailed(
                EncodingKeyDecodeError::InvalidBase64
            ))
        ));
        // Secret-free key diagnostics survive on the failure path (P3).
        assert_eq!(outcome.stages.encoding_key_input_chars, 9);
        assert_eq!(outcome.stages.encoding_key_padded_chars, 10);
        assert!(!outcome.stages.encoding_key_base64_decode_ok);
        assert_eq!(outcome.stages.encoding_key_decoded_bytes, 0);
        assert!(!outcome.stages.aes_key_decode_ok);
        assert!(!outcome.stages.ciphertext_decode_ok);
        assert!(!outcome.stages.aes_cbc_ok);
        assert!(!outcome.stages.pkcs7_ok);
        assert!(!outcome.stages.frame_parse_ok);
        assert!(!outcome.stages.message_extract_ok);
        assert_eq!(outcome.stages.receive_id_check, ReceiveIdCheck::Skipped);
    }

    #[test]
    fn wecom_crypto_stage_ciphertext_base64_failed() {
        let outcome = decrypt_message_with_report(
            PARITY_ENCODING_AES_KEY,
            "!!!not-base64!!!",
            "",
        );
        assert!(matches!(
            outcome.result,
            Err(WecomCryptoError::CiphertextBase64Failed)
        ));
        assert!(outcome.stages.aes_key_decode_ok);
        assert!(!outcome.stages.ciphertext_decode_ok);
        assert!(!outcome.stages.aes_cbc_ok);
    }

    #[test]
    fn wecom_crypto_stage_aes_cbc_length_failed() {
        // 20 base64 chars -> 15 bytes -> not a multiple of 16.
        let outcome =
            decrypt_message_with_report(PARITY_ENCODING_AES_KEY, "AAAAAAAAAAAAAAAAAAAA", "");
        assert!(matches!(outcome.result, Err(WecomCryptoError::AesCbcFailed)));
        assert!(outcome.stages.aes_key_decode_ok);
        assert!(outcome.stages.ciphertext_decode_ok);
        assert!(!outcome.stages.aes_cbc_ok);
        assert!(!outcome.stages.pkcs7_ok);
    }

    #[test]
    fn wecom_crypto_stage_pkcs7_failed() {
        // Frame of 22 bytes + trailing 0x00 -> last byte 0 -> pad < 1.
        let key = decode_encoding_aes_key(PARITY_ENCODING_AES_KEY).unwrap();
        let mut plaintext = vec![0u8; 16];
        plaintext.extend_from_slice(&2u32.to_be_bytes());
        plaintext.extend_from_slice(b"hi");
        plaintext.extend_from_slice(&[0u8; 10]);
        assert_eq!(plaintext.len(), 32);
        let encrypted = encrypt_raw_bytes(&key, &plaintext);
        let outcome = decrypt_message_with_report(PARITY_ENCODING_AES_KEY, &encrypted, "");
        assert!(matches!(outcome.result, Err(WecomCryptoError::Pkcs7Failed)));
        assert!(outcome.stages.aes_cbc_ok);
        assert!(!outcome.stages.pkcs7_ok);
        assert!(!outcome.stages.frame_parse_ok);
    }

    #[test]
    fn wecom_crypto_stage_frame_too_short() {
        // 16 bytes + PKCS#7 pad 16 -> 16 bytes after unpad -> < 20.
        let key = decode_encoding_aes_key(PARITY_ENCODING_AES_KEY).unwrap();
        let mut plaintext = vec![0u8; 16];
        plaintext.extend_from_slice(&[0x10u8; 16]);
        let encrypted = encrypt_raw_bytes(&key, &plaintext);
        let outcome = decrypt_message_with_report(PARITY_ENCODING_AES_KEY, &encrypted, "");
        assert!(matches!(outcome.result, Err(WecomCryptoError::FrameTooShort)));
        assert!(outcome.stages.pkcs7_ok);
        assert!(!outcome.stages.frame_parse_ok);
        assert!(!outcome.stages.message_extract_ok);
    }

    #[test]
    fn wecom_crypto_stage_message_length_invalid() {
        // msg_len = 0xFFFFFFFF -> 20+msg_len overflows the frame.
        let key = decode_encoding_aes_key(PARITY_ENCODING_AES_KEY).unwrap();
        let mut plaintext = vec![0u8; 16];
        plaintext.extend_from_slice(&u32::MAX.to_be_bytes());
        plaintext.extend_from_slice(b"x");
        let padding = 32 - (plaintext.len() % 32);
        plaintext.extend(std::iter::repeat_n(padding as u8, padding));
        let encrypted = encrypt_raw_bytes(&key, &plaintext);
        let outcome = decrypt_message_with_report(PARITY_ENCODING_AES_KEY, &encrypted, "");
        assert!(matches!(
            outcome.result,
            Err(WecomCryptoError::MessageLengthInvalid)
        ));
        assert!(outcome.stages.pkcs7_ok);
        assert!(!outcome.stages.frame_parse_ok);
    }

    #[test]
    fn wecom_crypto_stage_utf8_failed() {
        // msg_len = 2 with invalid UTF-8 bytes 0xFF 0xFE.
        let key = decode_encoding_aes_key(PARITY_ENCODING_AES_KEY).unwrap();
        let mut plaintext = vec![0u8; 16];
        plaintext.extend_from_slice(&2u32.to_be_bytes());
        plaintext.extend_from_slice(&[0xFF, 0xFE]);
        let padding = 32 - (plaintext.len() % 32);
        plaintext.extend(std::iter::repeat_n(padding as u8, padding));
        let encrypted = encrypt_raw_bytes(&key, &plaintext);
        let outcome = decrypt_message_with_report(PARITY_ENCODING_AES_KEY, &encrypted, "");
        assert!(matches!(outcome.result, Err(WecomCryptoError::Utf8Failed)));
        assert!(outcome.stages.frame_parse_ok);
        assert!(!outcome.stages.message_extract_ok);
    }

    #[test]
    fn wecom_crypto_stage_receive_id_invalid() {
        let outcome = decrypt_message_with_report(
            PARITY_ENCODING_AES_KEY,
            PARITY_CIPHERTEXT_1,
            "other-corp",
        );
        assert!(matches!(
            outcome.result,
            Err(WecomCryptoError::ReceiveIdMismatch)
        ));
        assert_eq!(outcome.stages.receive_id_check, ReceiveIdCheck::Invalid);
        assert!(outcome.stages.message_extract_ok);
    }

    #[test]
    fn wecom_crypto_stage_receive_id_valid_and_skipped() {
        let valid = decrypt_message_with_report(
            PARITY_ENCODING_AES_KEY,
            PARITY_CIPHERTEXT_1,
            PARITY_RECEIVE_ID,
        );
        assert!(valid.result.is_ok());
        assert_eq!(valid.stages.receive_id_check, ReceiveIdCheck::Valid);

        let skipped = decrypt_message_with_report(
            PARITY_ENCODING_AES_KEY,
            PARITY_CIPHERTEXT_1,
            "",
        );
        assert!(skipped.result.is_ok());
        assert_eq!(skipped.stages.receive_id_check, ReceiveIdCheck::Skipped);
    }
}
