use std::path::PathBuf;
use std::time::Duration;

use coolzhu_web_console::browser_bridge_protocol::{
    BridgeRequest, BridgeResponse, BROWSER_EXTENSION_ID,
};
use coolzhu_web_console::native_message::{decode_native_message, write_native_message};
use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::{connect_async, tungstenite::Message};

const BROKER_URL: &str = "ws://127.0.0.1:8765/api/computer-use/browser/native";

fn allowed_origin() -> String {
    format!("chrome-extension://{BROWSER_EXTENSION_ID}/")
}

fn validate_origin(arguments: &[String]) -> Result<(), String> {
    let expected = allowed_origin();
    arguments
        .iter()
        .find(|argument| argument.starts_with("chrome-extension://"))
        .filter(|origin| *origin == &expected)
        .map(|_| ())
        .ok_or_else(|| "native_host_origin_rejected".to_string())
}

fn runtime_nonce_path() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("CoolzhuAgent")
        .join("runtime")
        .join("browser-bridge-nonce")
}

fn read_nonce() -> Result<String, String> {
    let nonce = std::fs::read_to_string(runtime_nonce_path())
        .map_err(|_| "native_host_nonce_unavailable".to_string())?;
    let nonce = nonce.trim().to_string();
    if !(16..=256).contains(&nonce.len()) {
        return Err("native_host_nonce_invalid".to_string());
    }
    Ok(nonce)
}

async fn run(arguments: Vec<String>) -> Result<(), String> {
    validate_origin(&arguments)?;
    let nonce = read_nonce()?;
    let (socket, _) = tokio::time::timeout(Duration::from_secs(5), connect_async(BROKER_URL))
        .await
        .map_err(|_| "native_host_broker_timeout".to_string())?
        .map_err(|_| "native_host_broker_unavailable".to_string())?;
    let (mut websocket_writer, mut websocket_reader) = socket.split();

    let hello = BridgeRequest::Hello {
        request_id: "native-host-hello".to_string(),
        nonce,
    };
    websocket_writer
        .send(Message::Text(
            serde_json::to_string(&hello)
                .map_err(|_| "native_host_protocol_error".to_string())?
                .into(),
        ))
        .await
        .map_err(|_| "native_host_broker_unavailable".to_string())?;
    let handshake = tokio::time::timeout(Duration::from_secs(5), websocket_reader.next())
        .await
        .map_err(|_| "native_host_handshake_timeout".to_string())?
        .ok_or_else(|| "native_host_handshake_closed".to_string())?
        .map_err(|_| "native_host_handshake_failed".to_string())?;
    let Message::Text(handshake) = handshake else {
        return Err("native_host_handshake_failed".to_string());
    };
    let handshake: BridgeResponse =
        serde_json::from_str(&handshake).map_err(|_| "native_host_handshake_failed".to_string())?;
    if !handshake.ok || handshake.request_id != "native-host-hello" {
        return Err("native_host_handshake_failed".to_string());
    }

    let (extension_tx, mut extension_rx) = mpsc::unbounded_channel::<BridgeResponse>();
    std::thread::Builder::new()
        .name("coolzhu-native-stdin".to_string())
        .spawn(move || {
            let mut stdin = std::io::stdin().lock();
            while let Ok(response) = decode_native_message::<BridgeResponse>(&mut stdin) {
                if extension_tx.send(response).is_err() {
                    break;
                }
            }
        })
        .map_err(|_| "native_host_reader_failed".to_string())?;

    let mut stdout = std::io::stdout().lock();
    loop {
        tokio::select! {
            response = extension_rx.recv() => {
                let Some(response) = response else {
                    return Err("native_host_closed".to_string());
                };
                let encoded = serde_json::to_string(&response)
                    .map_err(|_| "native_host_protocol_error".to_string())?;
                websocket_writer.send(Message::Text(encoded.into())).await
                    .map_err(|_| "native_host_broker_unavailable".to_string())?;
            }
            message = websocket_reader.next() => {
                let Some(message) = message else {
                    return Err("native_host_broker_closed".to_string());
                };
                let message = message.map_err(|_| "native_host_broker_closed".to_string())?;
                match message {
                    Message::Text(text) => {
                        let request: BridgeRequest = serde_json::from_str(&text)
                            .map_err(|_| "native_host_protocol_error".to_string())?;
                        request.validate().map_err(str::to_string)?;
                        write_native_message(&mut stdout, &request)?;
                    }
                    Message::Close(_) => return Err("native_host_broker_closed".to_string()),
                    Message::Ping(payload) => {
                        websocket_writer.send(Message::Pong(payload)).await
                            .map_err(|_| "native_host_broker_unavailable".to_string())?;
                    }
                    _ => {}
                }
            }
        }
    }
}

#[tokio::main]
async fn main() {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    if let Err(error) = run(arguments).await {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_host_accepts_only_the_stable_extension_origin() {
        assert!(validate_origin(&[allowed_origin()]).is_ok());
        assert_eq!(
            validate_origin(&["chrome-extension://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/".to_string()]),
            Err("native_host_origin_rejected".to_string())
        );
        assert_eq!(
            validate_origin(&[]),
            Err("native_host_origin_rejected".to_string())
        );
    }
}
