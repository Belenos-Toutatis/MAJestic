//! Small blocking HTTP helpers (ureq), with MAJestic's user agent and sane
//! timeouts (connect 5 s, total 20 s).
//!
//! OWNER: intel agent.

use std::sync::LazyLock;
use std::time::{Duration, Instant};

use anyhow::Context;
use ureq::http::header::{CONTENT_LENGTH, CONTENT_RANGE, RANGE};
use ureq::http::{HeaderMap, Response};
use ureq::{Agent, Body};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const GLOBAL_TIMEOUT: Duration = Duration::from_secs(20);
const SIZE_PROBE_TIMEOUT: Duration = Duration::from_secs(8);
const ONLINE_PROBE_TIMEOUT: Duration = Duration::from_secs(3);
/// Upper bound for a text/JSON body (the KEV catalog is ~2 MB).
const MAX_BODY: u64 = 32 * 1024 * 1024;
const ONLINE_PROBE_URL: &str = "https://raw.githubusercontent.com/";

static AGENT: LazyLock<Agent> = LazyLock::new(|| {
    Agent::config_builder()
        .user_agent(crate::USER_AGENT)
        .timeout_connect(Some(CONNECT_TIMEOUT))
        .timeout_global(Some(GLOBAL_TIMEOUT))
        .build()
        .new_agent()
});

fn get(url: &str) -> anyhow::Result<Response<Body>> {
    AGENT.get(url).call().with_context(|| format!("GET {url}"))
}

fn read_text(mut response: Response<Body>, url: &str) -> anyhow::Result<String> {
    response.body_mut().with_config().limit(MAX_BODY).read_to_string().with_context(|| format!("reading {url}"))
}

/// GET a URL as text. Non-2xx statuses are errors.
pub fn get_text(url: &str) -> anyhow::Result<String> {
    read_text(get(url)?, url)
}

/// [`get_text`] with a shorter total timeout (per-package lookups).
pub(crate) fn get_text_within(url: &str, timeout: Duration) -> anyhow::Result<String> {
    let response = AGENT
        .get(url)
        .config()
        .timeout_global(Some(timeout.min(GLOBAL_TIMEOUT)))
        .build()
        .call()
        .with_context(|| format!("GET {url}"))?;
    read_text(response, url)
}

/// HTTP status of a failed request, when the server answered.
pub(crate) fn error_status(err: &anyhow::Error) -> Option<u16> {
    match err.downcast_ref::<ureq::Error>() {
        Some(ureq::Error::StatusCode(code)) => Some(*code),
        _ => None,
    }
}

/// GET a URL and deserialize JSON.
pub fn get_json<T: serde::de::DeserializeOwned>(url: &str) -> anyhow::Result<T> {
    let text = get_text(url)?;
    serde_json::from_str(&text).with_context(|| format!("decoding JSON from {url}"))
}

/// Size of a remote file: HEAD request following redirects, `Content-Length`
/// (falls back to a `Range: bytes=0-0` GET reading `Content-Range`).
/// `None` when unknown. ≤ 8 s.
pub fn content_length(url: &str) -> Option<u64> {
    let deadline = Instant::now() + SIZE_PROBE_TIMEOUT;

    let head = AGENT.head(url).config().timeout_global(Some(SIZE_PROBE_TIMEOUT)).build().call();
    if let Ok(response) = head {
        if let Some(len) = header_u64(response.headers(), CONTENT_LENGTH.as_str()).filter(|&n| n > 0) {
            return Some(len);
        }
    }

    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining < Duration::from_millis(250) {
        return None;
    }
    // The body is never read: dropping the response closes the connection.
    let response =
        AGENT.get(url).header(RANGE, "bytes=0-0").config().timeout_global(Some(remaining)).build().call().ok()?;
    match response.status().as_u16() {
        206 => response.headers().get(CONTENT_RANGE).and_then(|v| v.to_str().ok()).and_then(parse_content_range_total),
        // Server ignored the range: the full length is the file size.
        200 => header_u64(response.headers(), CONTENT_LENGTH.as_str()).filter(|&n| n > 0),
        _ => None,
    }
}

/// Quick connectivity probe (≤ 3 s).
pub fn is_online() -> bool {
    // Any HTTP answer (even a redirect or an error status) proves we are online.
    let result = AGENT
        .head(ONLINE_PROBE_URL)
        .config()
        .timeout_connect(Some(ONLINE_PROBE_TIMEOUT))
        .timeout_global(Some(ONLINE_PROBE_TIMEOUT))
        .max_redirects(0)
        .max_redirects_will_error(false)
        .http_status_as_error(false)
        .build()
        .call();
    match result {
        Ok(_) => true,
        Err(err) => {
            log::debug!("online probe failed: {err}");
            false
        }
    }
}

fn header_u64(headers: &HeaderMap, name: &str) -> Option<u64> {
    headers.get(name)?.to_str().ok()?.trim().parse().ok()
}

/// `bytes 0-0/12345` → 12345 (`*` or garbage → `None`).
fn parse_content_range_total(value: &str) -> Option<u64> {
    let (_, total) = value.trim().rsplit_once('/')?;
    total.trim().parse().ok().filter(|&n: &u64| n > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_range_parsing() {
        assert_eq!(parse_content_range_total("bytes 0-0/12345"), Some(12345));
        assert_eq!(parse_content_range_total(" bytes 0-0/ 987 "), Some(987));
        assert_eq!(parse_content_range_total("bytes 0-0/*"), None);
        assert_eq!(parse_content_range_total("bytes */0"), None);
        assert_eq!(parse_content_range_total("garbage"), None);
    }

    #[test]
    #[ignore = "network"]
    fn online_probe_and_sizes() {
        assert!(is_online());
        let text = get_text("https://raw.githubusercontent.com/microsoft/winget-pkgs/master/README.md").unwrap();
        assert!(!text.is_empty());
        // GitHub release asset: redirects to a CDN.
        let size = content_length("https://github.com/notepad-plus-plus/notepad-plus-plus/releases/download/v8.6.9/npp.8.6.9.Installer.x64.exe");
        println!("npp size: {size:?}");
        assert!(size.is_some_and(|n| n > 1_000_000));
        assert!(get_text("https://raw.githubusercontent.com/microsoft/winget-pkgs/master/does-not-exist").is_err());
    }
}
