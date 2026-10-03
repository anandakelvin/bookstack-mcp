//! Per-call usage log (fork-only): one JSON line per MCP request with the
//! response size, never the content. Enabled by `BSMCP_USAGE_LOG=<path>`;
//! unset = no logging. Read with `scripts/usage-report.py`.

use std::io::Write;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};

fn log_path() -> Option<&'static str> {
    static PATH: OnceLock<Option<String>> = OnceLock::new();
    PATH.get_or_init(|| std::env::var("BSMCP_USAGE_LOG").ok().filter(|p| !p.is_empty()))
        .as_deref()
}

/// Characters the client gets back: tool result text for `tools/call`,
/// the instructions for `initialize`, the serialized result otherwise.
fn response_chars(method: &str, response: &Value) -> usize {
    let result = &response["result"];
    match method {
        "tools/call" => result["content"]
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .filter_map(|c| c["text"].as_str())
                    .map(|t| t.chars().count())
                    .sum()
            })
            .unwrap_or(0),
        "initialize" => result["instructions"].as_str().map_or(0, |s| s.chars().count()),
        _ => serde_json::to_string(result).map_or(0, |s| s.chars().count()),
    }
}

pub fn line(session: &str, request: &Value, response: &Value, ms: u128, ts: u64) -> Value {
    let method = request["method"].as_str().unwrap_or("");
    let tool = if method == "tools/call" {
        request["params"]["name"].as_str().unwrap_or("")
    } else {
        method
    };
    let chars = response_chars(method, response);
    json!({
        "ts": ts,
        "session": session,
        "tool": tool,
        "chars": chars,
        "tokens": chars / 4,
        "ms": ms,
        "error": response["result"]["isError"].as_bool().unwrap_or(false)
            || response.get("error").is_some(),
    })
}

pub fn record(session: &str, request: &Value, response: &Value, ms: u128) {
    let Some(path) = log_path() else { return };
    let ts = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let entry = line(session, request, response, ms, ts);
    let result = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .and_then(|mut f| writeln!(f, "{entry}"));
    if let Err(e) = result {
        tracing::warn!(error = %e, path, "usage_log_write_failed");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_call_counts_text_only() {
        let req = json!({"method": "tools/call", "params": {"name": "export_page", "arguments": {"page_id": 44}}});
        let resp = json!({"result": {"content": [{"type": "text", "text": "abcdefgh"}], "_meta": {"time": "x"}}});
        let l = line("s1", &req, &resp, 12, 100);
        assert_eq!(l["tool"], "export_page");
        assert_eq!(l["chars"], 8);
        assert_eq!(l["tokens"], 2);
        assert_eq!(l["ms"], 12);
        assert_eq!(l["error"], false);
        assert!(l.get("arguments").is_none());
    }

    #[test]
    fn initialize_counts_instructions() {
        let req = json!({"method": "initialize"});
        let resp = json!({"result": {"instructions": "hello"}});
        let l = line("s1", &req, &resp, 1, 100);
        assert_eq!(l["tool"], "initialize");
        assert_eq!(l["chars"], 5);
    }

    #[test]
    fn tool_error_is_flagged() {
        let req = json!({"method": "tools/call", "params": {"name": "get_page"}});
        let resp = json!({"result": {"content": [{"type": "text", "text": "Error: x"}], "isError": true}});
        assert_eq!(line("s", &req, &resp, 1, 1)["error"], true);
    }
}
