//! `RealHttpClient` wraps a `reqwest::Client` pinned to the rustls TLS
//! backend (never native-tls/openssl — Global Constraints). Every request
//! carries the `canager/{version}` User-Agent and a 30-second client-wide
//! default timeout; `HttpRequest::timeout` overrides that default on a
//! per-request basis.

use super::{HttpClient, HttpError, HttpRequest, HttpResponse};
use async_trait::async_trait;

pub struct RealHttpClient {
    client: reqwest::Client,
}

impl RealHttpClient {
    pub fn new() -> RealHttpClient {
        let user_agent = format!("canager/{}", env!("CARGO_PKG_VERSION"));
        let client = reqwest::Client::builder()
            .tls_backend_rustls()
            .user_agent(user_agent)
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("reqwest client with the rustls TLS backend must build");
        RealHttpClient { client }
    }
}

impl Default for RealHttpClient {
    fn default() -> Self {
        RealHttpClient::new()
    }
}

#[async_trait]
impl HttpClient for RealHttpClient {
    async fn send(&self, req: HttpRequest) -> Result<HttpResponse, HttpError> {
        let method = reqwest::Method::from_bytes(req.method.as_bytes())
            .map_err(|e| HttpError::Network(format!("invalid method {:?}: {e}", req.method)))?;
        let mut builder = self
            .client
            .request(method, req.url.as_str())
            .timeout(req.timeout);
        for (name, value) in &req.headers {
            builder = builder.header(name.as_str(), value.as_str());
        }
        let response = builder.send().await.map_err(|e| {
            if e.is_timeout() {
                HttpError::Timeout(req.timeout)
            } else {
                HttpError::Network(e.to_string())
            }
        })?;
        let status = response.status().as_u16();
        let body = response
            .text()
            .await
            .map_err(|e| HttpError::Network(e.to_string()))?;
        Ok(HttpResponse { status, body })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::{HttpClient, HttpError, HttpRequest};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    /// Binds an ephemeral localhost port, accepts exactly one connection,
    /// reads until the blank line ending the request headers, writes back
    /// `response_bytes` verbatim, then closes the socket. Runs entirely
    /// offline (loopback only), so this is deterministic in CI without
    /// depending on real internet access — the same reasoning
    /// `crate::runner::real`'s tests spawn a real `/bin/sh` rather than
    /// mocking the OS.
    async fn serve_one_response(response_bytes: &'static [u8]) -> std::net::SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind ephemeral port");
        let addr = listener.local_addr().expect("local_addr");
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("accept");
            let mut buf = Vec::new();
            let mut chunk = [0u8; 4096];
            loop {
                let n = socket.read(&mut chunk).await.expect("read request");
                if n == 0 {
                    return;
                }
                buf.extend_from_slice(&chunk[..n]);
                if buf.windows(4).any(|w| w == b"\r\n\r\n") {
                    break;
                }
            }
            socket
                .write_all(response_bytes)
                .await
                .expect("write response");
            let _ = socket.shutdown().await;
        });
        addr
    }

    #[tokio::test]
    async fn test_real_http_client_fetches_status_and_body_from_a_real_socket() {
        let addr = serve_one_response(
            b"HTTP/1.1 200 OK\r\nContent-Length: 13\r\nConnection: close\r\n\r\nhello, world!",
        )
        .await;
        let client = RealHttpClient::new();
        let req = HttpRequest {
            method: "GET",
            url: format!("http://{addr}/"),
            headers: vec![],
            timeout: std::time::Duration::from_secs(5),
        };
        let response = client.send(req).await.expect("real request over loopback");
        assert_eq!(response.status, 200);
        assert_eq!(response.body, "hello, world!");
    }

    #[tokio::test]
    async fn test_real_http_client_sends_the_canager_user_agent() {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind ephemeral port");
        let addr = listener.local_addr().expect("local_addr");
        let captured = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("accept");
            let mut buf = Vec::new();
            let mut chunk = [0u8; 4096];
            loop {
                let n = socket.read(&mut chunk).await.expect("read request");
                if n == 0 {
                    break;
                }
                buf.extend_from_slice(&chunk[..n]);
                if buf.windows(4).any(|w| w == b"\r\n\r\n") {
                    break;
                }
            }
            socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                .await
                .expect("write response");
            let _ = socket.shutdown().await;
            String::from_utf8_lossy(&buf).into_owned()
        });

        let client = RealHttpClient::new();
        let req = HttpRequest {
            method: "GET",
            url: format!("http://{addr}/"),
            headers: vec![],
            timeout: std::time::Duration::from_secs(5),
        };
        client.send(req).await.expect("real request over loopback");

        let request_text = captured.await.expect("server task panicked");
        assert!(
            request_text.contains(&format!("canager/{}", env!("CARGO_PKG_VERSION"))),
            "expected the canager User-Agent in the request, got: {request_text}"
        );
    }

    #[tokio::test]
    async fn test_real_http_client_times_out_when_the_server_never_responds() {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind ephemeral port");
        let addr = listener.local_addr().expect("local_addr");
        tokio::spawn(async move {
            let (_socket, _) = listener.accept().await.expect("accept");
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        });

        let client = RealHttpClient::new();
        let req = HttpRequest {
            method: "GET",
            url: format!("http://{addr}/"),
            headers: vec![],
            timeout: std::time::Duration::from_millis(200),
        };
        let result = client.send(req).await;
        match result {
            Err(HttpError::Timeout(d)) => assert_eq!(d, std::time::Duration::from_millis(200)),
            other => panic!("expected Timeout, got {other:?}"),
        }
    }
}
