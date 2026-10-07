//! `RealHttpClient` wraps a `reqwest::Client` pinned to the rustls TLS
//! backend (never native-tls/openssl — Global Constraints). Every request
//! carries the `banager/{version}` User-Agent and a 30-second client-wide
//! default timeout; `HttpRequest::timeout` overrides that default on a
//! per-request basis. Redirects are not followed, a 3xx is an error rather
//! than a response, a response body is read to a cap instead of being
//! swallowed whole, and an `https` request to a host outside
//! `ALLOWED_HTTPS_HOSTS` is refused before any connection is opened.
//!
//! Its errors say which kind of failure each one was (`request_error`):
//! the network (`HttpError::Network`), the time (`Timeout`), a certificate
//! rustls would not accept (`Tls`), or this client's own refusal
//! (`Refused`) -- a check that ends in either of the last two would end the
//! same way next time, and is not asked to be tried again. Any other way a
//! TLS handshake fails -- a server that answers in plain HTTP, as a Wi-Fi
//! sign-in page does, an alert, a reset -- is the network: it may well
//! clear by itself.

use super::{HttpClient, HttpError, HttpRequest, HttpResponse};
use async_trait::async_trait;

/// The largest response body `RealHttpClient` will read, in bytes.
///
/// `Response::text()` has no bound of its own: the only thing standing
/// between Banager and an arbitrarily large body was the 30-second timeout,
/// and against a *loopback* daemon — which is exactly what the Ollama
/// adapter talks to — thirty seconds is gigabytes of resident memory in one
/// `String`. The largest body Banager legitimately reads is Ollama's
/// 1209-layer manifest at ~246 KiB (see
/// `adapters/fixtures/ollama/0.34.1/`), so 8 MiB is roughly 33x headroom
/// over the real worst case while still bounding the damage.
pub const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;

/// The only hosts `RealHttpClient` will open an `https` connection to.
///
/// Every https URL this crate builds names one of these: crates.io
/// (`CargoAdapter::latest_stable_version`), pypi.org
/// (`PipxAdapter::latest_pypi_version`), registry.ollama.ai
/// (`OllamaAdapter::compare_digests`), downloads.claude.ai
/// (`StandaloneAdapter::check_updates`, Claude Code's channel pointer),
/// static.rust-lang.org (the same, rustup's release file) and
/// antigravity-cli-auto-updater-974169037036.us-central1.run.app (the
/// same, Antigravity CLI's version manifest, a Google Cloud Run service;
/// on Apple silicon only).
/// `send` refuses any other https host
/// before a connection is opened -- fail closed, so a URL built from data
/// off disk or off the network (a crate name, a model reference) can at
/// worst re-point a request within one of these hosts, never at another
/// one. Plain `http` is exempt: the one http caller is the Ollama daemon at
/// `HostEnv::ollama_host` (default `http://127.0.0.1:11434`), which may
/// legitimately be any machine the user named. The exemption is by scheme,
/// so an `https://` `OLLAMA_HOST` is refused here like any other host off
/// the list -- a known gap (spec §十一: `with_extra_host`, to be passed in
/// by `Session::new`), stated in `docs/what-we-run.md` and pinned by
/// `tests/what_we_run_test.rs` so the document and this refusal change
/// together.
///
/// Adding a host here is a reviewed change with two other halves: the
/// adapter that contacts it, and `docs/what-we-run.md`, which must name
/// every host Banager connects to.
pub const ALLOWED_HTTPS_HOSTS: &[&str] = &[
    "crates.io",
    "pypi.org",
    "registry.ollama.ai",
    "downloads.claude.ai",
    "static.rust-lang.org",
    "antigravity-cli-auto-updater-974169037036.us-central1.run.app",
];

/// `Ok(())` when `url` is one `send` may fetch: any `http` URL, or an
/// `https` URL whose host is in `ALLOWED_HTTPS_HOSTS` exactly (no
/// subdomains: `api.crates.io` is not `crates.io`). Anything else -- another
/// https host, another scheme, a URL that does not parse -- is
/// `HttpError::Refused` naming the reason, the same error a refused
/// redirect gets, since both mean "this client will not go there".
///
/// `Url` lowercases an ASCII host, so the comparison is case-insensitive
/// without the list carrying uppercase spellings.
pub fn host_allowed(url: &str) -> Result<(), HttpError> {
    let parsed = url::Url::parse(url)
        .map_err(|e| HttpError::Refused(format!("invalid url {url:?}: {e}")))?;
    match parsed.scheme() {
        "http" => Ok(()),
        "https" => {
            let host = parsed.host_str().unwrap_or("");
            if ALLOWED_HTTPS_HOSTS.contains(&host) {
                Ok(())
            } else {
                Err(HttpError::Refused(format!(
                    "host not allowed: {host:?} is not one of {ALLOWED_HTTPS_HOSTS:?} (from {url})"
                )))
            }
        }
        other => Err(HttpError::Refused(format!(
            "scheme not allowed: {other:?} in {url}"
        ))),
    }
}

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
        let user_agent = format!("banager/{}", env!("CARGO_PKG_VERSION"));
        let client = reqwest::Client::builder()
            .tls_backend_rustls()
            .user_agent(user_agent)
            // reqwest's default is `Policy::limited(10)`: up to ten
            // redirects, to any host, with no https-only guard — so an
            // https request could be walked to plain http, or to a host
            // Banager never chose, carrying its headers with it. None of
            // the hosts this client talks to -- `ALLOWED_HTTPS_HOSTS` over
            // https, and the Ollama daemon over http -- ever needs a
            // redirect, so the policy is `none` and `send` below turns a
            // 3xx into an error instead of handing it back as a response.
            .redirect(reqwest::redirect::Policy::none())
            // The proxy the login shell's settings name, else the
            // process environment's, else this Mac's own network settings
            // (System Settings > Network > Proxies, which a proxy app in
            // its "system proxy" mode sets), looked up at each request, so
            // a read that ends after this client is built, or a proxy app
            // turned on or off, still counts -- and never for this Mac
            // itself, as the Ollama daemon usually is, or for what
            // `no_proxy` names (`super::proxy`, U12). Naming a proxy here
            // turns off reqwest's own reading of the environment and of
            // the network settings (`ClientBuilder::proxy`), which had no
            // exception for this Mac: `system_proxy` reads the network
            // settings the way it did.
            .proxy(reqwest::Proxy::custom(|url| {
                super::proxy::proxy_for(
                    url,
                    crate::runner::login_path::command_var,
                    super::proxy::system_proxy,
                )
            }))
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

/// The rustls error somewhere in `error`'s chain of causes, if there is
/// one: what a failed TLS handshake leaves there. tokio-rustls hands it on
/// inside an `io::Error` (`InvalidData`), which hyper-rustls wraps in
/// another, and `io::Error::source` skips the error it wraps -- it gives
/// that error's own source -- so each `io::Error` is opened with
/// `get_ref` as well as followed.
fn rustls_error_in<'a>(error: &'a (dyn std::error::Error + 'static)) -> Option<&'a rustls::Error> {
    let mut next = Some(error);
    while let Some(err) = next {
        if let Some(tls) = err.downcast_ref::<rustls::Error>() {
            return Some(tls);
        }
        if let Some(inner) = err
            .downcast_ref::<std::io::Error>()
            .and_then(|io| io.get_ref())
        {
            if let Some(tls) = rustls_error_in(inner) {
                return Some(tls);
            }
        }
        next = err.source();
    }
    None
}

/// Whether rustls refused the server's certificate: `InvalidCertificate`
/// -- which is how the macOS platform verifier reports every trust
/// failure, by name for the four Apple codes it maps (another name,
/// no chain to a trusted root, wrong key usage, revoked) and as `Other`
/// with Apple's own words for the rest ("“localhost” certificate is not
/// trusted: -67843") -- or a server that showed none
/// (`NoCertificatesPresented`). Asking again meets the same certificate.
/// No other rustls error is known to: a corrupt record (plain HTTP on the
/// TLS port, as a Wi-Fi sign-in page answers), an alert, a failed
/// decryption may be gone on the next try.
fn is_certificate_error(error: &rustls::Error) -> bool {
    matches!(
        error,
        rustls::Error::InvalidCertificate(_) | rustls::Error::NoCertificatesPresented
    )
}

/// What a `reqwest` error was, for `req`: the time running out
/// (`Timeout`); a certificate rustls would not accept (`Tls`, with
/// rustls's words and the request's host -- not reqwest's own "error
/// sending request", which says nothing of it, `is_certificate_error`); a
/// request that could not be built (`Refused`); else the network
/// (`Network`) -- with rustls's words after reqwest's where the handshake
/// failed some other way.
fn request_error(e: reqwest::Error, req: &HttpRequest) -> HttpError {
    if e.is_timeout() {
        return HttpError::Timeout(req.timeout);
    }
    match rustls_error_in(&e) {
        Some(tls) if is_certificate_error(tls) => {
            let host = url::Url::parse(&req.url)
                .ok()
                .and_then(|url| url.host_str().map(str::to_string))
                .unwrap_or_default();
            HttpError::Tls {
                host,
                detail: tls.to_string(),
            }
        }
        Some(tls) => HttpError::Network(format!("{e}: {tls}")),
        None if e.is_builder() => HttpError::Refused(format!("could not build the request: {e}")),
        None => HttpError::Network(e.to_string()),
    }
}

impl RealHttpClient {
    /// `send` without the host check: the request as it goes out once
    /// `host_allowed` has passed it. Separate only so a test can reach a
    /// loopback https server, which the check would refuse.
    async fn fetch(&self, req: HttpRequest) -> Result<HttpResponse, HttpError> {
        let method = reqwest::Method::from_bytes(req.method.as_bytes())
            .map_err(|e| HttpError::Refused(format!("invalid method {:?}: {e}", req.method)))?;
        let mut builder = self
            .client
            .request(method, req.url.as_str())
            .timeout(req.timeout);
        for (name, value) in &req.headers {
            builder = builder.header(name.as_str(), value.as_str());
        }
        let response = builder.send().await.map_err(|e| request_error(e, &req))?;
        let status = response.status();
        if status.is_redirection() {
            let location = response
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("<no Location header>")
                .to_string();
            return Err(HttpError::Refused(format!(
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
        while let Some(chunk) = response.chunk().await.map_err(|e| request_error(e, &req))? {
            if bytes.len() + chunk.len() > self.max_body_bytes {
                return Err(HttpError::BodyTooLarge {
                    limit: self.max_body_bytes,
                });
            }
            bytes.extend_from_slice(&chunk);
        }
        // Lossy for the same reason `text()` is: these endpoints all answer
        // UTF-8 -- JSON, or Claude Code's bare version number -- and a stray
        // invalid byte should not turn a readable response into a hard error.
        let body = String::from_utf8_lossy(&bytes).into_owned();
        Ok(HttpResponse { status, body })
    }
}

#[async_trait]
impl HttpClient for RealHttpClient {
    async fn send(&self, req: HttpRequest) -> Result<HttpResponse, HttpError> {
        // Before the request is even built: a refused host must never
        // resolve, connect, or carry a header anywhere.
        host_allowed(&req.url)?;
        self.fetch(req).await
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
    async fn test_real_http_client_sends_the_banager_user_agent() {
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
            request_text.contains(&format!("banager/{}", env!("CARGO_PKG_VERSION"))),
            "expected the banager User-Agent in the request, got: {request_text}"
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
        // with no https-only guard. None of Banager's requests -- the
        // Ollama daemon over http, `ALLOWED_HTTPS_HOSTS` over https -- ever
        // needs one, so a 3xx means something has gone wrong and following
        // it would carry the request (and its headers) somewhere Banager
        // never chose. The destination here is a second, *watched* server:
        // if it is ever contacted, the redirect was followed.
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
            Err(HttpError::Refused(message)) => assert!(
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
        // Banager legitimately reads is the 1209-layer Ollama manifest, at
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

    /// Accepts one connection, reads whatever arrives first -- for an
    /// https request, the TLS ClientHello -- and then does `then` with the
    /// socket: answer in plain HTTP, hang up, reset.
    async fn serve_after_the_client_hello(
        then: impl FnOnce(
                tokio::net::TcpStream,
            ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>
            + Send
            + 'static,
    ) -> std::net::SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind ephemeral port");
        let addr = listener.local_addr().expect("local_addr");
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("accept");
            let mut chunk = [0u8; 4096];
            let _ = socket.read(&mut chunk).await;
            then(socket).await;
        });
        addr
    }

    /// `fetch` -- `send` past the host check, which would refuse a
    /// loopback https host before connecting -- of `https://{addr}/`.
    async fn fetch_https(addr: std::net::SocketAddr) -> Result<HttpResponse, HttpError> {
        RealHttpClient::new()
            .fetch(HttpRequest {
                method: "GET",
                url: format!("https://{addr}/"),
                headers: vec![],
                timeout: std::time::Duration::from_secs(5),
            })
            .await
    }

    /// A self-signed certificate for `localhost` and 127.0.0.1 (P-256,
    /// valid to 2126) and its PKCS#8 key, DER in base64: made for this
    /// test alone with `openssl req -x509`, trusted by nothing, protecting
    /// nothing.
    const UNTRUSTED_CERT: &str = "MIIBmzCCAUGgAwIBAgIUZMYRWQvH8GsneQTG+3GxNikiKJ4wCgYIKoZIzj0EAwIwFDESMBAGA1UEAwwJbG9jYWxob3N0MCAXDTI2MTAwMjExMDY0NVoYDzIxMjYwOTA4MTEwNjQ1WjAUMRIwEAYDVQQDDAlsb2NhbGhvc3QwWTATBgcqhkjOPQIBBggqhkjOPQMBBwNCAASsogJHbE96DqRB8vnKhNjWKebNvoq2pQsJMWXFrM92UUW/KdmHJCFSM8aHlNfHZB07HUFUhpk7+9Gwe9ZrAvANo28wbTAdBgNVHQ4EFgQUkZPDt782qqIFrqD8q17N4pzzh/QwHwYDVR0jBBgwFoAUkZPDt782qqIFrqD8q17N4pzzh/QwDwYDVR0TAQH/BAUwAwEB/zAaBgNVHREEEzARhwR/AAABgglsb2NhbGhvc3QwCgYIKoZIzj0EAwIDSAAwRQIhAMLRJ8Usf7+1nRWu/r8/eaMJbbpsMYraHnsVc5DFOeyoAiBfyFS/9NB2WMw0PhOLCnelbaeHW6jxmBWQY9oUPxz63Q==";
    const UNTRUSTED_KEY: &str = "MIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQgiPkxgmWoci2kKASFp0CioJnrvcIg3Y+VfQhJff4y0kGhRANCAASsogJHbE96DqRB8vnKhNjWKebNvoq2pQsJMWXFrM92UUW/KdmHJCFSM8aHlNfHZB07HUFUhpk7+9Gwe9ZrAvAN";

    /// Accepts one connection and answers its TLS handshake with
    /// `UNTRUSTED_CERT`, as a proxy that reads https traffic does with a
    /// certificate of its own that this Mac does not trust. rustls on a
    /// thread of its own, over a blocking socket: the server half needs no
    /// async, and rustls (with reqwest's aws-lc-rs provider) is all it needs.
    fn serve_an_untrusted_certificate() -> std::net::SocketAddr {
        use base64::Engine;
        let decode = |b64: &str| {
            base64::engine::general_purpose::STANDARD
                .decode(b64)
                .expect("test certificate base64")
        };
        let certificate = rustls::pki_types::CertificateDer::from(decode(UNTRUSTED_CERT));
        let key = rustls::pki_types::PrivateKeyDer::Pkcs8(
            rustls::pki_types::PrivatePkcs8KeyDer::from(decode(UNTRUSTED_KEY)),
        );
        let config = rustls::ServerConfig::builder_with_provider(std::sync::Arc::new(
            rustls::crypto::aws_lc_rs::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .expect("protocol versions")
        .with_no_client_auth()
        .with_single_cert(vec![certificate], key)
        .expect("test certificate and key");
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
        let addr = listener.local_addr().expect("local_addr");
        std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().expect("accept");
            let mut connection = rustls::ServerConnection::new(std::sync::Arc::new(config))
                .expect("server connection");
            // Until the client gives up on the certificate (its alert ends
            // the handshake as an error here) or the socket closes.
            while connection.is_handshaking() {
                if connection.complete_io(&mut socket).is_err() {
                    break;
                }
            }
        });
        addr
    }

    #[test]
    fn test_is_certificate_error_is_only_a_refused_certificate() {
        use rustls::{AlertDescription, CertificateError, Error, InvalidMessage, OtherError};
        for certificate in [
            Error::InvalidCertificate(CertificateError::UnknownIssuer),
            Error::InvalidCertificate(CertificateError::NotValidForName),
            Error::InvalidCertificate(CertificateError::Expired),
            Error::InvalidCertificate(CertificateError::Revoked),
            // The macOS verifier's words for every code it does not name.
            Error::InvalidCertificate(CertificateError::Other(OtherError(std::sync::Arc::new(
                std::io::Error::other("“localhost” certificate is not trusted: -67843"),
            )))),
            Error::NoCertificatesPresented,
        ] {
            assert!(is_certificate_error(&certificate), "{certificate:?}");
        }
        for other in [
            // Plain HTTP on the TLS port: a Wi-Fi sign-in page.
            Error::InvalidMessage(InvalidMessage::InvalidContentType),
            Error::AlertReceived(AlertDescription::HandshakeFailure),
            Error::AlertReceived(AlertDescription::InternalError),
            Error::DecryptError,
        ] {
            assert!(!is_certificate_error(&other), "{other:?}");
        }
    }

    #[tokio::test]
    async fn test_real_http_client_reports_an_untrusted_certificate_as_tls_not_network() {
        // Round-5 review finding 6: a certificate this Mac does not trust
        // was `Network`, which a lookup counts as "check again" forever.
        // Checked by the platform verifier, as every real request is.
        let addr = serve_an_untrusted_certificate();
        match fetch_https(addr).await {
            Err(error @ HttpError::Tls { .. }) => {
                let HttpError::Tls { host, detail } = &error else {
                    unreachable!()
                };
                assert_eq!(host, "127.0.0.1");
                assert!(detail.starts_with("invalid peer certificate"), "{detail}");
                // rustls's words, never reqwest's "error sending request",
                // which the window would read as the network failing.
                let words = error.to_string();
                assert!(
                    !words.contains("error sending request") && !words.contains("network"),
                    "{words}"
                );
            }
            other => panic!("expected Tls, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_real_http_client_reports_plain_http_on_the_tls_port_as_network() {
        // What a Wi-Fi sign-in page or a broken middlebox does: rustls
        // reads a corrupt record, which may be gone once the person has
        // signed in -- the network, with rustls's words after reqwest's.
        let addr = serve_after_the_client_hello(|mut socket| {
            Box::pin(async move {
                let _ = socket
                    .write_all(b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                    .await;
                let _ = socket.shutdown().await;
            })
        })
        .await;
        match fetch_https(addr).await {
            Err(HttpError::Network(message)) => {
                assert!(message.contains("corrupt message"), "{message}")
            }
            other => panic!("expected Network, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_real_http_client_reports_a_server_that_hangs_up_in_the_handshake_as_network() {
        let addr = serve_after_the_client_hello(|mut socket| {
            Box::pin(async move {
                let _ = socket.shutdown().await;
            })
        })
        .await;
        let result = fetch_https(addr).await;
        assert!(
            matches!(result, Err(HttpError::Network(_))),
            "expected Network, got {result:?}"
        );
    }

    #[tokio::test]
    async fn test_real_http_client_reports_a_reset_in_the_handshake_as_network() {
        let addr = serve_after_the_client_hello(|socket| {
            Box::pin(async move {
                // Closed with no linger: the client gets a reset, not a
                // hang-up.
                socket.set_zero_linger().expect("SO_LINGER 0");
                drop(socket);
            })
        })
        .await;
        let result = fetch_https(addr).await;
        assert!(
            matches!(result, Err(HttpError::Network(_))),
            "expected Network, got {result:?}"
        );
    }

    #[tokio::test]
    async fn test_real_http_client_reports_a_refused_connection_as_network() {
        // A port nothing listens on: the connection is refused, which the
        // next check may well get past.
        let addr = {
            let listener = TcpListener::bind("127.0.0.1:0")
                .await
                .expect("bind ephemeral port");
            listener.local_addr().expect("local_addr")
        };
        let client = RealHttpClient::new();
        let result = client
            .send(HttpRequest {
                method: "GET",
                url: format!("http://{addr}/"),
                headers: vec![],
                timeout: std::time::Duration::from_secs(5),
            })
            .await;
        assert!(
            matches!(result, Err(HttpError::Network(_))),
            "expected Network, got {result:?}"
        );
    }

    /// An error whose cause is `inner`, as hyper's and reqwest's wrap the
    /// connector's.
    #[derive(Debug)]
    struct Wrapped(Box<dyn std::error::Error + Send + Sync>);

    impl std::fmt::Display for Wrapped {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("error sending request")
        }
    }

    impl std::error::Error for Wrapped {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            Some(self.0.as_ref())
        }
    }

    #[test]
    fn test_rustls_error_in_finds_a_certificate_error_inside_two_io_errors() {
        // The chain a certificate rustls does not trust leaves: tokio-rustls
        // puts the rustls error in an `InvalidData` io::Error, hyper-rustls
        // puts that in another (`io::Error::other`), and hyper and reqwest
        // wrap that. `io::Error::source` skips what it wraps, so following
        // `source()` alone would never reach rustls's error.
        let certificate =
            rustls::Error::InvalidCertificate(rustls::CertificateError::UnknownIssuer);
        let chain = Wrapped(Box::new(std::io::Error::other(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            certificate.clone(),
        ))));
        assert_eq!(rustls_error_in(&chain), Some(&certificate));
        assert_eq!(
            rustls_error_in(&chain).map(|e| e.to_string()).as_deref(),
            Some("invalid peer certificate: UnknownIssuer")
        );
        // And nothing where there is no rustls error: a refused connection.
        let refused = Wrapped(Box::new(std::io::Error::other(std::io::Error::new(
            std::io::ErrorKind::ConnectionRefused,
            "Connection refused (os error 61)",
        ))));
        assert_eq!(rustls_error_in(&refused), None);
    }

    #[test]
    fn test_host_allowed_accepts_the_three_https_urls_the_adapters_build() {
        // The exact URL shapes `CargoAdapter::latest_stable_version`,
        // `PipxAdapter::latest_pypi_version` and
        // `OllamaAdapter::compare_digests` build.
        host_allowed("https://crates.io/api/v1/crates/hexyl").expect("crates.io");
        host_allowed("https://pypi.org/pypi/cowsay/json").expect("pypi.org");
        host_allowed("https://registry.ollama.ai/v2/library/qwen3/manifests/8b")
            .expect("registry.ollama.ai");
    }

    #[test]
    fn test_host_allowed_exempts_plain_http_whatever_the_host() {
        // The Ollama daemon: its default, a loopback with an ephemeral port
        // (every loopback test in this module), and a machine the user
        // named through `OLLAMA_HOST`.
        host_allowed("http://127.0.0.1:11434/api/tags").expect("the default daemon");
        host_allowed("http://127.0.0.1:49152/").expect("a loopback test server");
        host_allowed("http://ollama.lan:11434/api/tags").expect("a machine the user named");
    }

    #[test]
    fn test_host_allowed_refuses_an_https_host_off_the_list() {
        for url in [
            "https://example.com/",
            // The list is exact, not a suffix match.
            "https://api.crates.io/api/v1/crates/hexyl",
            "https://crates.io.example.com/",
            // Userinfo does not make evil.example into crates.io.
            "https://crates.io@evil.example/",
        ] {
            match host_allowed(url) {
                Err(HttpError::Refused(message)) => assert!(
                    message.contains("host not allowed"),
                    "{url}: the error must say the host is not allowed, got {message:?}"
                ),
                other => panic!("{url}: expected a host-not-allowed error, got {other:?}"),
            }
        }
    }

    #[test]
    fn test_host_allowed_compares_hosts_case_insensitively() {
        // `Url` lowercases an ASCII domain, so the list needs no uppercase
        // spellings and a request cannot dodge it with one.
        host_allowed("https://CRATES.IO/api/v1/crates/hexyl").expect("uppercase spelling");
    }

    #[test]
    fn test_host_allowed_refuses_other_schemes_and_unparseable_urls() {
        match host_allowed("ftp://crates.io/") {
            Err(HttpError::Refused(message)) => {
                assert!(message.contains("scheme not allowed"), "got {message:?}")
            }
            other => panic!("expected a scheme error, got {other:?}"),
        }
        match host_allowed("not a url") {
            Err(HttpError::Refused(message)) => {
                assert!(message.contains("invalid url"), "got {message:?}")
            }
            other => panic!("expected an invalid-url error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_real_http_client_refuses_an_https_host_off_the_list_before_connecting() {
        // `.invalid` is reserved never to resolve (RFC 2606): had this
        // request reached reqwest, the error would be a DNS failure in
        // reqwest's words. The allowlist's own words prove the check ran
        // before any connection was attempted.
        let client = RealHttpClient::new();
        let result = client
            .send(HttpRequest {
                method: "GET",
                url: "https://not-on-the-list.invalid/".to_string(),
                headers: vec![],
                timeout: std::time::Duration::from_secs(5),
            })
            .await;
        match result {
            Err(HttpError::Refused(message)) => assert!(
                message.contains("host not allowed"),
                "expected the allowlist's refusal, got {message:?}"
            ),
            other => panic!("expected a host-not-allowed error, got {other:?}"),
        }
    }

    #[test]
    fn test_host_allowed_accepts_claude_codes_channel_pointers() {
        // The exact URLs `StandaloneAdapter::check_updates` builds for the
        // `CLAUDE` recipe (adapters/standalone/recipes.rs), phase 4 step B.
        host_allowed("https://downloads.claude.ai/claude-code-releases/latest")
            .expect("downloads.claude.ai, latest");
        host_allowed("https://downloads.claude.ai/claude-code-releases/stable")
            .expect("downloads.claude.ai, stable");
    }

    #[test]
    fn test_host_allowed_accepts_rustups_release_file() {
        // The exact URL `StandaloneAdapter::check_updates` builds for the
        // `RUSTUP` recipe (adapters/standalone/recipes.rs), phase 4 step E.
        host_allowed("https://static.rust-lang.org/rustup/release-stable.toml")
            .expect("static.rust-lang.org");
    }

    #[test]
    fn test_host_allowed_accepts_agys_version_manifest() {
        // The exact URL `StandaloneAdapter::check_updates` requests for the
        // `AGY` recipe (adapters/standalone/recipes.rs), phase 4 step D --
        // on Apple silicon only (`latest::manifest_arch_allowed`).
        host_allowed(
            "https://antigravity-cli-auto-updater-974169037036.us-central1.run.app/manifests/darwin_arm64.json",
        )
        .expect("antigravity-cli-auto-updater-974169037036.us-central1.run.app");
    }

    async fn f08_read_request_headers(socket: &mut tokio::net::TcpStream) {
        let mut header = Vec::new();
        let mut buffer = [0; 4096];
        while !header.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
            let read = socket.read(&mut buffer).await.unwrap();
            assert!(read > 0, "client closed before sending request headers");
            header.extend_from_slice(&buffer[..read]);
        }
    }

    // Headers have already succeeded: exercise the separate body-read path.
    #[tokio::test]
    async fn f08_g11_deadline_expires_after_headers_and_partial_body() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (sent, received) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            f08_read_request_headers(&mut socket).await;
            socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\n{")
                .await
                .unwrap();
            sent.send(()).unwrap();
            std::future::pending::<()>().await;
        });
        let timeout = std::time::Duration::from_millis(200);
        let started = std::time::Instant::now();
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(3),
            RealHttpClient::new().send(HttpRequest {
                method: "GET",
                url: format!("http://{addr}/"),
                headers: vec![],
                timeout,
            }),
        )
        .await;
        server.abort();
        received
            .await
            .expect("server actually sent headers and a body prefix");
        assert!(matches!(result.unwrap(), Err(HttpError::Timeout(d)) if d == timeout));
        assert!(started.elapsed() < std::time::Duration::from_secs(3));
    }

    #[tokio::test]
    async fn f08_g12_truncated_framing_never_returns_a_partial_response() {
        // 100 bytes promised and 13 sent; a 16-byte (0x10) chunk cut off
        // after 13. The server closes the connection after either.
        const SHORT_OF_ITS_LENGTH: &[u8] = concat!(
            "HTTP/1.1 200 OK\r\nContent-Length: 100\r\nConnection: close\r\n\r\n",
            "{\"version\":\"1"
        )
        .as_bytes();
        const UNFINISHED_CHUNK: &[u8] = concat!(
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n",
            "10\r\n{\"version\":\"1"
        )
        .as_bytes();
        for response in [SHORT_OF_ITS_LENGTH, UNFINISHED_CHUNK] {
            let addr = serve_one_response(response).await;
            let result = RealHttpClient::new()
                .send(HttpRequest {
                    method: "GET",
                    url: format!("http://{addr}/"),
                    headers: vec![],
                    timeout: std::time::Duration::from_secs(2),
                })
                .await;
            assert!(
                matches!(result, Err(HttpError::Network(_))),
                "truncated body is a network failure: {result:?}"
            );
        }
    }

    async fn f08_stream_without_length(chunked: bool, over: bool) {
        // Keep the over-limit connection open, so a Content-Length-only
        // check (or waiting for EOF before checking) fails the deadline.
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (sent, received) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            f08_read_request_headers(&mut socket).await;
            let header = if chunked {
                "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n"
            } else {
                "HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n"
            };
            socket.write_all(header.as_bytes()).await.unwrap();
            let mut sent = Some(sent);
            let mut written = 0;
            for size in [511, 513, usize::from(over)] {
                if size == 0 {
                    continue;
                }
                if chunked {
                    socket
                        .write_all(format!("{size:x}\r\n").as_bytes())
                        .await
                        .unwrap();
                }
                socket.write_all(&vec![b'x'; size]).await.unwrap();
                written += size;
                if written == 1024 + usize::from(over) {
                    sent.take().unwrap().send(()).unwrap();
                }
                if chunked {
                    // Once the last over-limit byte arrives the client may
                    // close immediately, before the chunk's trailing CRLF.
                    let _ = socket.write_all(b"\r\n").await;
                }
                if written < 1024 + usize::from(over) {
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                }
            }
            if over {
                std::future::pending::<()>().await;
            }
            if chunked {
                socket.write_all(b"0\r\n\r\n").await.unwrap();
            }
            socket.shutdown().await.unwrap();
        });
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(3),
            RealHttpClient::with_body_limit(1024).send(HttpRequest {
                method: "GET",
                url: format!("http://{addr}/"),
                headers: vec![],
                timeout: std::time::Duration::from_secs(5),
            }),
        )
        .await;
        if over {
            server.abort();
        } else {
            server.await.unwrap();
        }
        received
            .await
            .expect("the test stream crossed the boundary");
        let result =
            result.expect("client must abandon an over-limit stream without waiting for EOF");
        if over {
            assert!(
                matches!(result, Err(HttpError::BodyTooLarge { limit: 1024 })),
                "{result:?}"
            );
        } else {
            assert_eq!(result.unwrap().body, "x".repeat(1024));
        }
    }

    #[tokio::test]
    async fn f08_g13_chunked_exact_limit() {
        f08_stream_without_length(true, false).await;
    }
    #[tokio::test]
    async fn f08_g13_chunked_one_byte_over_limit() {
        f08_stream_without_length(true, true).await;
    }
    #[tokio::test]
    async fn f08_g13_close_delimited_exact_limit() {
        f08_stream_without_length(false, false).await;
    }
    #[tokio::test]
    async fn f08_g13_close_delimited_one_byte_over_limit() {
        f08_stream_without_length(false, true).await;
    }
}
