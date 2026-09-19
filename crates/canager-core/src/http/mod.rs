//! `HttpClient`: the network seam every adapter that needs the internet
//! goes through, mirroring how `crate::runner::CommandRunner` makes
//! subprocess work testable (see `crate::runner`). No adapter is allowed to
//! call `reqwest` directly — Global Constraints — so every network path in
//! this crate has a `MockHttpClient` double in tests.

use async_trait::async_trait;

pub mod mock;
pub use mock::MockHttpClient;

/// `method` is always `"GET"` in this phase — no adapter added in this plan
/// ever writes over HTTP (Ollama's writes go through its CLI; see the
/// ruling in the phase 3 plan).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpRequest {
    pub method: &'static str,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub timeout: std::time::Duration,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: u16,
    pub body: String,
}

#[derive(Debug, thiserror::Error)]
pub enum HttpError {
    #[error("network error: {0}")]
    Network(String),
    #[error("timed out after {0:?}")]
    Timeout(std::time::Duration),
    #[error("no canned response for {0}")]
    NoMock(String),
}

#[async_trait]
pub trait HttpClient: Send + Sync {
    async fn send(&self, req: HttpRequest) -> Result<HttpResponse, HttpError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_http_error_display_messages_match_the_documented_wording() {
        assert_eq!(
            HttpError::Network("dns lookup failed".to_string()).to_string(),
            "network error: dns lookup failed"
        );
        assert_eq!(
            HttpError::Timeout(std::time::Duration::from_secs(30)).to_string(),
            "timed out after 30s"
        );
        assert_eq!(
            HttpError::NoMock("https://pypi.org/pypi/jq/json".to_string()).to_string(),
            "no canned response for https://pypi.org/pypi/jq/json"
        );
    }
}
