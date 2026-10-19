//! Small blocking HTTP helpers (ureq), with MAJestic's user agent and sane
//! timeouts (connect 5 s, total 20 s).
//!
//! OWNER: intel agent.

/// GET a URL as text. Non-2xx statuses are errors.
pub fn get_text(url: &str) -> anyhow::Result<String> {
    let _ = url;
    todo!("http::get_text")
}

/// GET a URL and deserialize JSON.
pub fn get_json<T: serde::de::DeserializeOwned>(url: &str) -> anyhow::Result<T> {
    let _ = url;
    todo!("http::get_json")
}

/// Size of a remote file: HEAD request following redirects, `Content-Length`
/// (falls back to a `Range: bytes=0-0` GET reading `Content-Range`).
/// `None` when unknown. ≤ 8 s.
pub fn content_length(url: &str) -> Option<u64> {
    let _ = url;
    todo!("http::content_length")
}

/// Quick connectivity probe (≤ 3 s).
pub fn is_online() -> bool {
    todo!("http::is_online")
}
