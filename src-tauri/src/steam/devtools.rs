//! A session with Steam's built-in DevTools endpoint, where its client API lives.

use crate::Error;
use serde_json::{json, Value};
use std::{net::TcpStream, time::Duration};

/// Shown when Steam was not started with `-cef-enable-debugging`.
pub const UNAVAILABLE: &str =
    "Steam's client controls are unavailable. Restart Steam from RBX Tools once so it starts with fast offline controls.";

/// A DevTools session in Steam's shared UI context, where its client API lives.
pub struct DevTools {
    socket: tungstenite::WebSocket<TcpStream>,
    next_id: u64,
}

impl DevTools {
    /// Connects to the `SharedJSContext` page at a loopback `endpoint`.
    pub fn connect(endpoint: &str) -> Result<Self, Error> {
        let url = Self::shared_context_url(endpoint)?;
        // Only ever talk to the loopback endpoint that was asked for.
        if !url.starts_with(&format!("ws://{endpoint}/")) {
            return Err(UNAVAILABLE.into());
        }
        let stream = TcpStream::connect(endpoint).map_err(|_| UNAVAILABLE.to_string())?;
        stream.set_read_timeout(Some(Duration::from_secs(5)))?;
        let (socket, _) = tungstenite::client(url, stream).map_err(control_failed)?;
        Ok(Self { socket, next_id: 0 })
    }

    /// Evaluates JavaScript in Steam's UI and returns its (awaited) value.
    pub fn eval(&mut self, expression: &str) -> Result<Value, Error> {
        self.next_id += 1;
        let id = self.next_id;
        let request = json!({
            "id": id,
            "method": "Runtime.evaluate",
            "params": { "expression": expression, "awaitPromise": true, "returnByValue": true },
        });
        self.socket
            .send(tungstenite::Message::text(request.to_string()))
            .map_err(control_failed)?;
        loop {
            let message = self.socket.read().map_err(control_failed)?;
            let Ok(text) = message.to_text() else {
                continue;
            };
            let Ok(reply) = serde_json::from_str::<Value>(text) else {
                continue;
            };
            if reply["id"] != id {
                continue;
            }
            let result = &reply["result"];
            if let Some(details) = result.get("exceptionDetails") {
                return Err(control_failed(&details["text"]));
            }
            return Ok(result["result"]["value"].clone());
        }
    }

    /// Runs a Steam console command.
    pub fn console(&mut self, command: &str) -> Result<(), Error> {
        let command = serde_json::to_string(command)?;
        self.eval(&format!("SteamClient.Console.ExecCommand({command}), true"))
            .map(|_| ())
    }

    fn shared_context_url(endpoint: &str) -> Result<String, Error> {
        let pages: Value = reqwest::blocking::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(3))
            .build()
            .map_err(|e| e.to_string())?
            .get(format!("http://{endpoint}/json"))
            .send()
            .and_then(|response| response.text())
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .ok_or_else(|| UNAVAILABLE.to_string())?;
        pages
            .as_array()
            .and_then(|pages| {
                pages
                    .iter()
                    .find(|page| page["title"] == "SharedJSContext")
                    .and_then(|page| page["webSocketDebuggerUrl"].as_str())
            })
            .map(str::to_string)
            .ok_or_else(|| "Steam is still starting. Try again in a few seconds.".into())
    }
}

/// A failed exchange with Steam's client controls.
fn control_failed(cause: impl std::fmt::Display) -> Error {
    format!("Steam control failed: {cause}").into()
}
