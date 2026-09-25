use std::io::{Read, Write};

use serde::{de::DeserializeOwned, Serialize};

use crate::browser_bridge_protocol::NATIVE_MESSAGE_MAX_BYTES;

pub fn encode_native_message<T: Serialize>(value: &T) -> Result<Vec<u8>, String> {
    let payload = serde_json::to_vec(value).map_err(|error| error.to_string())?;
    if payload.len() > NATIVE_MESSAGE_MAX_BYTES {
        return Err("native_message_too_large".to_string());
    }
    let length = u32::try_from(payload.len()).map_err(|_| "native_message_too_large")?;
    let mut framed = Vec::with_capacity(payload.len() + 4);
    framed.extend_from_slice(&length.to_le_bytes());
    framed.extend_from_slice(&payload);
    Ok(framed)
}

pub fn decode_native_message<T: DeserializeOwned>(reader: &mut impl Read) -> Result<T, String> {
    let mut length = [0u8; 4];
    reader
        .read_exact(&mut length)
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::UnexpectedEof => "native_host_closed".to_string(),
            _ => format!("native_message_read_failed:{error}"),
        })?;
    let length = u32::from_le_bytes(length) as usize;
    if length == 0 || length > NATIVE_MESSAGE_MAX_BYTES {
        return Err("native_message_too_large".to_string());
    }
    let mut payload = vec![0u8; length];
    reader
        .read_exact(&mut payload)
        .map_err(|error| format!("native_message_read_failed:{error}"))?;
    serde_json::from_slice(&payload).map_err(|error| format!("native_message_invalid_json:{error}"))
}

pub fn write_native_message<T: Serialize>(
    writer: &mut impl Write,
    value: &T,
) -> Result<(), String> {
    let framed = encode_native_message(value)?;
    writer
        .write_all(&framed)
        .and_then(|_| writer.flush())
        .map_err(|error| format!("native_message_write_failed:{error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn native_message_is_four_byte_little_endian_json() {
        let framed = encode_native_message(&json!({"ok":true})).unwrap();
        let length = u32::from_le_bytes(framed[..4].try_into().unwrap()) as usize;
        assert_eq!(length, framed.len() - 4);
        let decoded: serde_json::Value = decode_native_message(&mut framed.as_slice()).unwrap();
        assert_eq!(decoded, json!({"ok":true}));
    }

    #[test]
    fn native_message_rejects_oversize_and_eof() {
        let length = ((NATIVE_MESSAGE_MAX_BYTES + 1) as u32).to_le_bytes();
        assert_eq!(
            decode_native_message::<serde_json::Value>(&mut length.as_slice()).unwrap_err(),
            "native_message_too_large"
        );
        assert_eq!(
            decode_native_message::<serde_json::Value>(&mut [].as_slice()).unwrap_err(),
            "native_host_closed"
        );
    }
}
