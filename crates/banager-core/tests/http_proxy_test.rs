//! U12 of the decisions round, through the real client: once the login
//! shell's proxy settings are read (`runner::login_path::accept`),
//! Banager's own requests go through the proxy they name -- a plain
//! request as a forwarded one, an https one as a `CONNECT` tunnel -- and a
//! request to this Mac itself, as the one to Ollama is, never does. Two
//! local listeners stand in for the proxy and for Ollama; nothing leaves
//! this Mac: each request is for this Mac, named in `no_proxy`, or of a
//! scheme whose proxy the test sets itself, so a proxy set in the
//! environment the tests run in, or in System Settings, is never asked.
//! A file of its own: `accept` sets state for the whole process, and each
//! file under tests/ runs in a process of its own.

use banager_core::http::{HttpClient, HttpError, HttpRequest, RealHttpClient};
use banager_core::runner::login_path::{self, LoginEnv};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::time::Duration;

type Log = Arc<Mutex<Vec<String>>>;

/// A listener on this Mac that answers every request with `body`, or a
/// `CONNECT` with 502 (no tunnel is ever opened), and keeps the first line
/// of each request it got, and each whole head.
fn listener(body: &'static str) -> (u16, Log, Log) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().unwrap().port();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = seen.clone();
    let heads = Arc::new(Mutex::new(Vec::new()));
    let head_log = heads.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut head = Vec::new();
            let mut byte = [0u8; 1];
            while !head.ends_with(b"\r\n\r\n") {
                match stream.read(&mut byte) {
                    Ok(1) => head.push(byte[0]),
                    _ => break,
                }
            }
            let head = String::from_utf8_lossy(&head).into_owned();
            let first = head.lines().next().unwrap_or_default().to_string();
            let answer = if first.starts_with("CONNECT ") {
                "HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    .to_string()
            } else {
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
            };
            log.lock().unwrap().push(first);
            head_log.lock().unwrap().push(head);
            let _ = stream.write_all(answer.as_bytes());
        }
    });
    (port, seen, heads)
}

/// A listener that keeps the first byte each connection sends, and closes
/// it: a SOCKS5 client starts with its version, 5, an HTTP one with a
/// letter.
fn first_bytes() -> (u16, Arc<Mutex<Vec<u8>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().unwrap().port();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = seen.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut byte = [0u8; 1];
            if let Ok(1) = stream.read(&mut byte) {
                log.lock().unwrap().push(byte[0]);
            }
        }
    });
    (port, seen)
}

fn get(url: &str) -> HttpRequest {
    HttpRequest {
        method: "GET",
        url: url.to_string(),
        headers: Vec::new(),
        timeout: Duration::from_secs(10),
    }
}

#[tokio::test]
async fn test_own_requests_go_through_the_login_shells_proxy_but_never_for_this_mac() {
    let (proxy_port, proxied, proxy_heads) = listener("through the proxy");
    let (ollama_port, straight, ollama_heads) = listener("straight to ollama");
    // With a login, which goes to the proxy and nowhere else.
    let proxy = format!("http://someone:u12-secret@127.0.0.1:{proxy_port}");
    login_path::accept(&LoginEnv {
        path: "/usr/bin:/bin".to_string(),
        imported: vec![
            ("http_proxy".to_string(), proxy.clone()),
            ("https_proxy".to_string(), proxy.clone()),
            // Set, so that one in the environment the tests run in cannot
            // send everything straight.
            ("no_proxy".to_string(), "straight.invalid".to_string()),
        ],
    });
    let client = RealHttpClient::new();

    // A plain request to another machine: forwarded by the proxy.
    let answer = client
        .send(get("http://elsewhere.invalid:11434/api/tags"))
        .await
        .expect("the proxy answers");
    assert_eq!(answer.body, "through the proxy");
    assert_eq!(
        *proxied.lock().unwrap(),
        ["GET http://elsewhere.invalid:11434/api/tags HTTP/1.1"]
    );

    // The Ollama on this Mac: straight, though a proxy is set and
    // `no_proxy` does not name it.
    for host in ["127.0.0.1", "0.0.0.0"] {
        let answer = client
            .send(get(&format!("http://{host}:{ollama_port}/api/tags")))
            .await
            .expect("Ollama answers");
        assert_eq!(answer.body, "straight to ollama", "{host}");
    }
    assert_eq!(straight.lock().unwrap().len(), 2);
    assert_eq!(proxied.lock().unwrap().len(), 1);
    // The proxy's login went to the proxy ("someone:u12-secret"), and not
    // to Ollama.
    assert!(
        proxy_heads.lock().unwrap()[0]
            .to_ascii_lowercase()
            .contains("proxy-authorization: basic c29tzw9uztp1mtitc2vjcmv0"),
        "{:?}",
        proxy_heads.lock().unwrap()
    );
    for head in ollama_heads.lock().unwrap().iter() {
        let head = head.to_ascii_lowercase();
        assert!(!head.contains("authorization"), "{head}");
    }

    // An https check: a tunnel through the proxy, to the host on the
    // list -- which this proxy refuses, so nothing leaves this Mac.
    let refused = client
        .send(get(
            "https://crates.io/api/v1/crates/banager-u12-proxy-test",
        ))
        .await;
    assert!(matches!(refused, Err(HttpError::Network(_))), "{refused:?}");
    assert_eq!(
        proxied.lock().unwrap().last().map(String::as_str),
        Some("CONNECT crates.io:443 HTTP/1.1")
    );

    // What `no_proxy` names goes straight: here to a name that does not
    // exist, so the request fails without the proxy hearing of it.
    let before = proxied.lock().unwrap().len();
    assert!(client.send(get("http://straight.invalid/")).await.is_err());
    assert_eq!(proxied.lock().unwrap().len(), before);

    // A SOCKS proxy, as Clash and Surge print `all_proxy`: spoken to as
    // one, not sent an HTTP `CONNECT`. `https_proxy` is set to it as well:
    // `all_proxy` comes after `https_proxy` and `HTTPS_PROXY`, and a name
    // the login shell did not set is taken from the environment the tests
    // run in, so a terminal that exports a proxy would otherwise send this
    // request there (`http::proxy::proxy_for`, whose own tests cover the
    // order).
    let (socks_port, greeted) = first_bytes();
    let socks = format!("socks5://127.0.0.1:{socks_port}");
    login_path::accept(&LoginEnv {
        path: "/usr/bin:/bin".to_string(),
        imported: vec![
            ("https_proxy".to_string(), socks.clone()),
            ("all_proxy".to_string(), socks),
            ("no_proxy".to_string(), "straight.invalid".to_string()),
        ],
    });
    let client = RealHttpClient::new();
    assert!(client
        .send(get(
            "https://crates.io/api/v1/crates/banager-u12-proxy-test"
        ))
        .await
        .is_err());
    assert_eq!(*greeted.lock().unwrap(), [5u8]);
}
