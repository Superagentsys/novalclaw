use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::constants::application_max_message_bytes;
use crate::error::BridgeError;

/// Frame a JSON string using Chrome Native Messaging length-prefix rules.
/// Uses native endianness, matching `native_messaging`.
pub fn encode_raw_json(json: &str) -> Result<Vec<u8>, BridgeError> {
    let bytes = json.as_bytes();
    let max = application_max_message_bytes();
    if bytes.len() > max {
        return Err(BridgeError::PayloadTooLarge {
            len: bytes.len(),
            max,
        });
    }
    if bytes.len() > native_messaging::host::MAX_TO_BROWSER {
        return Err(BridgeError::PayloadTooLarge {
            len: bytes.len(),
            max: native_messaging::host::MAX_TO_BROWSER,
        });
    }
    let mut frame = Vec::with_capacity(4 + bytes.len());
    frame.extend_from_slice(&(bytes.len() as u32).to_ne_bytes());
    frame.extend_from_slice(bytes);
    Ok(frame)
}

pub fn decode_raw_json(frame: &[u8]) -> Result<String, BridgeError> {
    if frame.len() < 4 {
        return Err(BridgeError::MalformedFrame {
            detail: "truncated length prefix".into(),
        });
    }
    let mut len_buf = [0u8; 4];
    len_buf.copy_from_slice(&frame[..4]);
    let len = u32::from_ne_bytes(len_buf) as usize;
    let max = application_max_message_bytes();
    if len > max {
        return Err(BridgeError::PayloadTooLarge { len, max });
    }
    if 4 + len > frame.len() {
        return Err(BridgeError::MalformedFrame {
            detail: "truncated payload".into(),
        });
    }
    String::from_utf8(frame[4..4 + len].to_vec()).map_err(|_| BridgeError::MalformedFrame {
        detail: "incoming frame is not UTF-8".into(),
    })
}

pub fn decode_reader<R: std::io::Read>(reader: &mut R) -> Result<Option<String>, BridgeError> {
    native_messaging::host::decode_message_opt(reader, application_max_message_bytes())
        .map_err(BridgeError::from)
}

pub async fn write_frame<W: AsyncWrite + Unpin>(
    writer: &mut W,
    json: &str,
) -> Result<(), BridgeError> {
    let frame = encode_raw_json(json)?;
    writer.write_all(&frame).await?;
    writer.flush().await?;
    Ok(())
}

pub async fn read_frame<R: AsyncRead + Unpin>(
    reader: &mut R,
) -> Result<Option<String>, BridgeError> {
    let mut len_buf = [0u8; 4];
    match reader.read_exact(&mut len_buf).await {
        Ok(_) => {}
        Err(err) if err.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(err) => return Err(BridgeError::Io(err)),
    }
    let len = u32::from_ne_bytes(len_buf) as usize;
    let max = application_max_message_bytes();
    if len > max {
        return Err(BridgeError::PayloadTooLarge { len, max });
    }
    let mut buf = vec![0u8; len];
    reader.read_exact(&mut buf).await?;
    String::from_utf8(buf).map(Some).map_err(|_| BridgeError::MalformedFrame {
        detail: "incoming frame is not UTF-8".into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn encode_decode_round_trip() {
        let json = r#"{"protocol_version":1,"request_id":"r1","operation":"ping"}"#;
        let frame = encode_raw_json(json).unwrap();
        assert_eq!(decode_raw_json(&frame).unwrap(), json);
        let mut cur = Cursor::new(frame);
        assert_eq!(decode_reader(&mut cur).unwrap().unwrap(), json);
    }

    #[test]
    fn oversized_application_payload_is_typed() {
        let huge = "x".repeat(application_max_message_bytes() + 1);
        let err = encode_raw_json(&huge).unwrap_err();
        assert!(matches!(err, BridgeError::PayloadTooLarge { .. }));
    }

    #[test]
    fn oversized_length_prefix_is_typed_without_allocating_the_claim() {
        let mut frame = (u32::MAX).to_ne_bytes().to_vec();
        frame.extend_from_slice(b"nope");
        let err = decode_raw_json(&frame).unwrap_err();
        assert!(matches!(err, BridgeError::PayloadTooLarge { len, .. } if len == u32::MAX as usize));
    }

    #[test]
    fn malformed_truncated_frame_is_typed() {
        let err = decode_raw_json(&[1, 0]).unwrap_err();
        assert!(matches!(err, BridgeError::MalformedFrame { .. }));
    }

    #[test]
    fn native_messaging_decode_rejects_oversize() {
        let len = (application_max_message_bytes() as u32 + 1).to_ne_bytes();
        let mut data = len.to_vec();
        data.extend_from_slice(&[0u8; 8]);
        let mut cur = Cursor::new(data);
        let err = decode_reader(&mut cur).unwrap_err();
        assert!(matches!(err, BridgeError::PayloadTooLarge { .. }));
    }
}
