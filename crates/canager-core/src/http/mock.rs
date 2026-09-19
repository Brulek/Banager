use super::{HttpClient, HttpError, HttpRequest, HttpResponse};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Mutex;

/// Mirrors `crate::runner::MockRunner`'s ergonomics: canned responses and
/// failures keyed by exact URL, plus a call log in request order.
pub struct MockHttpClient {
    responses: Mutex<HashMap<String, HttpResponse>>,
    failures: Mutex<HashMap<String, String>>,
    calls: Mutex<Vec<String>>,
    requests: Mutex<Vec<HttpRequest>>,
}

impl MockHttpClient {
    pub fn new() -> MockHttpClient {
        MockHttpClient {
            responses: Mutex::new(HashMap::new()),
            failures: Mutex::new(HashMap::new()),
            calls: Mutex::new(Vec::new()),
            requests: Mutex::new(Vec::new()),
        }
    }

    pub fn respond(&self, url: &str, response: HttpResponse) {
        self.responses
            .lock()
            .unwrap()
            .insert(url.to_string(), response);
    }

    pub fn fail(&self, url: &str, error: &str) {
        self.failures
            .lock()
            .unwrap()
            .insert(url.to_string(), error.to_string());
    }

    pub fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }

    /// Every request this client has been sent, in order, with its method,
    /// headers and timeout intact. `calls()` keeps only the urls, which is
    /// enough for most assertions; a test that has to prove a *header* went
    /// out needs this one — Task 10's registry `Accept:` header is the only
    /// reason the anonymous Ollama registry returns a v2 manifest at all,
    /// and nothing else in this crate can observe it.
    pub fn requests(&self) -> Vec<HttpRequest> {
        self.requests.lock().unwrap().clone()
    }
}

impl Default for MockHttpClient {
    fn default() -> Self {
        MockHttpClient::new()
    }
}

#[async_trait]
impl HttpClient for MockHttpClient {
    async fn send(&self, req: HttpRequest) -> Result<HttpResponse, HttpError> {
        self.calls.lock().unwrap().push(req.url.clone());
        self.requests.lock().unwrap().push(req.clone());
        if let Some(error) = self.failures.lock().unwrap().get(&req.url) {
            return Err(HttpError::Network(error.clone()));
        }
        self.responses
            .lock()
            .unwrap()
            .get(&req.url)
            .cloned()
            .ok_or_else(|| HttpError::NoMock(req.url.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Inside `mod tests` deliberately: see Step 3.
    fn get_request(url: &str) -> HttpRequest {
        HttpRequest {
            method: "GET",
            url: url.to_string(),
            headers: vec![],
            timeout: std::time::Duration::from_secs(30),
        }
    }

    #[tokio::test]
    async fn test_mock_http_client_returns_canned_response_and_records_the_url() {
        let client = MockHttpClient::new();
        client.respond(
            "https://pypi.org/pypi/jq/json",
            HttpResponse {
                status: 200,
                body: "{}".to_string(),
            },
        );
        let response = client
            .send(get_request("https://pypi.org/pypi/jq/json"))
            .await
            .expect("mocked call");
        assert_eq!(response.status, 200);
        assert_eq!(response.body, "{}");
        assert_eq!(
            client.calls(),
            vec!["https://pypi.org/pypi/jq/json".to_string()]
        );
    }

    #[tokio::test]
    async fn test_mock_http_client_fail_returns_a_network_error() {
        let client = MockHttpClient::new();
        client.fail("https://pypi.org/pypi/missing/json", "connection refused");
        let err = client
            .send(get_request("https://pypi.org/pypi/missing/json"))
            .await
            .expect_err("expected a failure");
        match err {
            HttpError::Network(msg) => assert_eq!(msg, "connection refused"),
            other => panic!("expected Network, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_mock_http_client_errors_on_an_unconfigured_url() {
        let client = MockHttpClient::new();
        let result = client
            .send(get_request("https://example.invalid/unset"))
            .await;
        assert!(matches!(result, Err(HttpError::NoMock(_))));
    }
}
