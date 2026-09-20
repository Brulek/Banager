//! `RealHttpClient` wraps a `reqwest::Client` pinned to the rustls TLS
//! backend (never native-tls/openssl — Global Constraints). Every request
//! carries the `canager/{version}` User-Agent and a 30-second client-wide
//! default timeout; `HttpRequest::timeout` overrides that default on a
//! per-request basis. Redirects are not followed, a 3xx is an error rather
//! than a response, and a response body is read to a cap instead of being
//! swallowed whole.

use super::{HttpClient, HttpError, HttpRequest, HttpResponse};
use async_trait::async_trait;

/// The largest response body `RealHttpClient` will read, in bytes.
///
/// `Response::text()` has no bound of its own: the only thing standing
/// between Canager and an arbitrarily large body was the 30-second timeout,
/// and against a *loopback* daemon — which is exactly what the Ollama
/// adapter talks to — thirty seconds is gigabytes of resident memory in one
/// `String`. The largest body Canager legitimately reads is Ollama's
/// 1209-layer manifest at ~246 KiB (see
/// `adapters/fixtures/ollama/0.34.1/`), so 8 MiB is roughly 33x headroom
/// over the real worst case while still bounding the damage.
pub const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;

pub struct RealHttpClient {
    client: reqwest::Client,
    max_body_bytes: usize,
}

impl RealHttpClient {
    pub fn new() -> RealHttpClient {
        RealHttpClient::with_body_limit(MAX_RESPONSE_BYTES)
    }

    /// The same client with a different body cap. Public so the cap's
    /// behaviour can be tested at the byte, at a few kilobytes rather than
    /// a few megabytes.
    pub fn with_body_limit(max_body_bytes: usize) -> RealHttpClient {
        let user_agent = format!("canager/{}", env!("CARGO_PKG_VERSION"));
        let client = reqwest::Client::builder()
            .tls_backend_rustls()
            .user_agent(user_agent)
            // reqwest's default is `Policy::limited(10)`: up to ten
            // redirects, to any host, with no https-only guard — so an
            // https request could be walked to plain http, or to a host
            // Canager never chose, carrying its headers with it. None of
            // the four endpoints this client talks to (a local Ollama
            // daemon, registry.ollama.ai, crates.io, PyPI) ever needs a
            // redirect, so the policy is `none` and `send` below turns a
            // 3xx into an error instead of handing it back as a response.
            .redirect(reqwest::redirect::Policy::none())
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("reqwest client with the rustls TLS backend must build");
        RealHttpClient {
            client,
            max_body_bytes,
        }
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
        let status = response.status();
        if status.is_redirection() {
            let location = response
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("<no Location header>")
                .to_string();
            return Err(HttpError::Network(format!(
                "refusing to follow a redirect: {} answered {} pointing at {location}",
                req.url, status
            )));
        }
        let status = status.as_u16();
        // Accumulated chunk by chunk against the cap rather than through
        // `text()`, so an oversized body is abandoned mid-stream instead of
        // being buffered in full and measured afterwards. `chunk()` is used
        // in preference to `bytes_stream()` because it needs no extra
        // reqwest feature (and so no futures-core dependency) to do the
        // same job.
        let mut response = response;
        let mut bytes: Vec<u8> = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|e| {
            if e.is_timeout() {
                HttpError::Timeout(req.timeout)
            } else {
                HttpError::Network(e.to_string())
            }
        })? {
            if bytes.len() + chunk.len() > self.max_body_bytes {
                return Err(HttpError::BodyTooLarge {
                    limit: self.max_body_bytes,
                });
            }
            bytes.extend_from_slice(&chunk);
        }
        // Lossy for the same reason `text()` is: these endpoints all answer
        // UTF-8 JSON, and a stray invalid byte should not turn a readable
        // response into a hard error.
        let body = String::from_utf8_lossy(&bytes).into_owned();
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

    /// Like `serve_one_response`, but flips `was_contacted` as soon as a
    /// connection arrives, so a test can prove a request was *not* made.
    async fn serve_one_response_watched(
        response_bytes: &'static [u8],
        was_contacted: std::sync::Arc<std::sync::atomic::AtomicBool>,
    ) -> std::net::SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind ephemeral port");
        let addr = listener.local_addr().expect("local_addr");
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("accept");
            was_contacted.store(true, std::sync::atomic::Ordering::SeqCst);
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
    async fn test_real_http_client_does_not_follow_a_redirect_and_reports_it_as_an_error() {
        // reqwest's default policy follows up to ten redirects, to any host,
        // with no https-only guard. None of Canager's requests -- a local
        // Ollama daemon, crates.io, PyPI, registry.ollama.ai -- ever needs
        // one, so a 3xx means something has gone wrong and following it
        // would carry the request (and its headers) somewhere Canager never
        // chose. The destination here is a second, *watched* server: if it
        // is ever contacted, the redirect was followed.
        let followed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let destination = serve_one_response_watched(
            b"HTTP/1.1 200 OK\r\nContent-Length: 8\r\nConnection: close\r\n\r\nfollowed",
            followed.clone(),
        )
        .await;
        let redirect_response = Box::leak(
            format!(
                "HTTP/1.1 302 Found\r\nLocation: http://{destination}/\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            )
            .into_bytes()
            .into_boxed_slice(),
        );
        let addr = serve_one_response(redirect_response).await;

        let client = RealHttpClient::new();
        let result = client
            .send(HttpRequest {
                method: "GET",
                url: format!("http://{addr}/"),
                headers: vec![],
                timeout: std::time::Duration::from_secs(5),
            })
            .await;

        match result {
            Err(HttpError::Network(message)) => assert!(
                message.contains("302"),
                "the error must name the status it refused to follow, got {message:?}"
            ),
            other => panic!("expected a redirect to be an error, got {other:?}"),
        }
        assert!(
            !followed.load(std::sync::atomic::Ordering::SeqCst),
            "the redirect destination must never be contacted"
        );
    }

    /// Serves one response whose body is `body_len` bytes of `x`, written
    /// from a small buffer rather than a `body_len`-sized allocation, so a
    /// multi-megabyte body costs the test almost nothing.
    async fn serve_body_of_length(body_len: usize) -> std::net::SocketAddr {
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
            let header = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {body_len}\r\nConnection: close\r\n\r\n"
            );
            socket
                .write_all(header.as_bytes())
                .await
                .expect("write header");
            let filler = [b'x'; 8192];
            let mut written = 0usize;
            while written < body_len {
                let n = filler.len().min(body_len - written);
                if socket.write_all(&filler[..n]).await.is_err() {
                    // The client is entitled to hang up once it has seen
                    // enough: that is the whole point of the cap.
                    return;
                }
                written += n;
            }
            let _ = socket.shutdown().await;
        });
        addr
    }

    #[tokio::test]
    async fn test_real_http_client_refuses_a_body_past_the_default_limit() {
        // `response.text()` read the whole body into a String with no bound
        // at all; the only limit was the 30s timeout, and against a loopback
        // daemon that is gigabytes of resident memory. The largest body
        // Canager legitimately reads is the 1209-layer Ollama manifest, at
        // ~246 KiB, so the default cap leaves roughly 33x headroom.
        let addr = serve_body_of_length(MAX_RESPONSE_BYTES + 1).await;
        let client = RealHttpClient::new();
        let result = client
            .send(HttpRequest {
                method: "GET",
                url: format!("http://{addr}/"),
                headers: vec![],
                timeout: std::time::Duration::from_secs(30),
            })
            .await;
        match result {
            Err(HttpError::BodyTooLarge { limit }) => assert_eq!(limit, MAX_RESPONSE_BYTES),
            other => panic!("expected BodyTooLarge, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_real_http_client_accepts_a_body_that_exactly_fills_the_limit() {
        let addr = serve_body_of_length(1024).await;
        let client = RealHttpClient::with_body_limit(1024);
        let response = client
            .send(HttpRequest {
                method: "GET",
                url: format!("http://{addr}/"),
                headers: vec![],
                timeout: std::time::Duration::from_secs(5),
            })
            .await
            .expect("a body exactly at the limit is not over it");
        assert_eq!(response.body.len(), 1024);
    }

    #[tokio::test]
    async fn test_real_http_client_refuses_a_body_one_byte_past_the_limit() {
        let addr = serve_body_of_length(1025).await;
        let client = RealHttpClient::with_body_limit(1024);
        let result = client
            .send(HttpRequest {
                method: "GET",
                url: format!("http://{addr}/"),
                headers: vec![],
                timeout: std::time::Duration::from_secs(5),
            })
            .await;
        match result {
            Err(HttpError::BodyTooLarge { limit }) => assert_eq!(limit, 1024),
            other => panic!("expected BodyTooLarge, got {other:?}"),
        }
    }
}
