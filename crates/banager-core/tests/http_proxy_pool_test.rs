//! R5 of the f19 review, through the real client: a proxy setting that
//! holds a login gives that login to that proxy alone, even when the
//! settings change while a connection to another proxy waits in the pool.
//!
//! reqwest picks the proxy when it opens a connection, and puts the
//! `Proxy-Authorization` of a plain-http request on at each request, from
//! the settings as they are then; hyper-util hands the request any idle
//! connection to the same destination. So without a fresh pool for fresh
//! settings, a read of the login shell that brings a proxy with a login
//! after a first request went through another one sends that login to the
//! other one. Two local listeners stand in for the two proxies, each
//! keeping its connections open as a real proxy does; the request is for a
//! name that does not exist, and only ever reaches one of them. A file of
//! its own: `accept` sets state for the whole process, and each file under
//! tests/ runs in a process of its own.

use banager_core::http::{HttpClient, HttpRequest, RealHttpClient};
use banager_core::runner::login_path::{self, LoginEnv};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Each request head a proxy got, with the number of the connection it
/// came on (1 for the first connection it accepted).
type Heads = Arc<Mutex<Vec<(usize, String)>>>;

/// A proxy on this Mac that keeps every connection open and answers each
/// request on it with an Ollama-shaped body, keeping the head first.
fn keep_alive_proxy() -> (u16, Heads) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().unwrap().port();
    let heads: Heads = Arc::new(Mutex::new(Vec::new()));
    let log = heads.clone();
    let connections = Arc::new(AtomicUsize::new(0));
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let connection = connections.fetch_add(1, Ordering::SeqCst) + 1;
            let log = log.clone();
            std::thread::spawn(move || loop {
                let mut head = Vec::new();
                let mut byte = [0u8; 1];
                while !head.ends_with(b"\r\n\r\n") {
                    match stream.read(&mut byte) {
                        Ok(1) => head.push(byte[0]),
                        // Closed by the client: this connection is done.
                        _ => return,
                    }
                }
                log.lock()
                    .unwrap()
                    .push((connection, String::from_utf8_lossy(&head).into_owned()));
                let body = r#"{"models":[]}"#;
                let answer = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/json\r\n\r\n{body}",
                    body.len()
                );
                if stream.write_all(answer.as_bytes()).is_err() {
                    return;
                }
            });
        }
    });
    (port, heads)
}

fn get(url: &str) -> HttpRequest {
    HttpRequest {
        method: "GET",
        url: url.to_string(),
        headers: Vec::new(),
        timeout: Duration::from_secs(10),
    }
}

/// The login shell's settings, with `http_proxy` alone for a proxy.
/// `no_proxy` is set so that one in the environment the tests run in
/// cannot send the request straight.
fn read_login_shell(http_proxy: &str) {
    login_path::accept(&LoginEnv {
        path: "/usr/bin:/bin".to_string(),
        imported: vec![
            ("http_proxy".to_string(), http_proxy.to_string()),
            ("no_proxy".to_string(), "straight.invalid".to_string()),
        ],
    });
}

#[tokio::test]
async fn test_a_proxys_login_never_goes_to_another_proxy_after_the_settings_change() {
    let (a_port, a_heads) = keep_alive_proxy();
    let (b_port, b_heads) = keep_alive_proxy();
    // An Ollama on another machine, over plain http, as `OLLAMA_HOST` can
    // name one: the one kind of request whose proxy login reqwest sends
    // with the request rather than in a `CONNECT`.
    let url = "http://elsewhere.invalid:11434/api/tags";

    // A first request through proxy A, which holds no login.
    read_login_shell(&format!("http://127.0.0.1:{a_port}"));
    let client = RealHttpClient::new();
    let answer = client.send(get(url)).await.expect("proxy A answers");
    assert_eq!(answer.body, r#"{"models":[]}"#);
    assert_eq!(a_heads.lock().unwrap().len(), 1);

    // Then a read of the login shell names proxy B, with a login, while
    // the connection to A is still open and idle.
    read_login_shell(&format!("http://someone:p2-secret@127.0.0.1:{b_port}"));
    let answer = client.send(get(url)).await.expect("proxy B answers");
    assert_eq!(answer.body, r#"{"models":[]}"#);

    // A never heard of B's login, nor of the second request.
    {
        let a = a_heads.lock().unwrap();
        for (_, head) in a.iter() {
            let head = head.to_ascii_lowercase();
            assert!(
                !head.contains("proxy-authorization"),
                "proxy A was sent a login: {head}"
            );
        }
        assert_eq!(a.len(), 1, "the second request went to proxy A: {a:?}");
    }
    // B got the request, with its own login ("someone:p2-secret").
    {
        let b = b_heads.lock().unwrap();
        assert_eq!(b.len(), 1, "{b:?}");
        assert!(
            b[0].1
                .starts_with("GET http://elsewhere.invalid:11434/api/tags HTTP/1.1\r\n"),
            "{b:?}"
        );
        assert!(
            b[0].1
                .to_ascii_lowercase()
                .contains("proxy-authorization: basic c29tzw9uztpwmi1zzwnyzxq="),
            "{b:?}"
        );
    }

    // While the settings stay as they are, connections are still kept and
    // reused: a third request goes to B on the connection the second one
    // opened.
    client.send(get(url)).await.expect("proxy B answers again");
    let b = b_heads.lock().unwrap();
    assert_eq!(b.len(), 2, "{b:?}");
    assert_eq!(b[1].0, b[0].0, "a new connection to B: {b:?}");
    assert!(
        b[1].1
            .to_ascii_lowercase()
            .contains("proxy-authorization: basic c29tzw9uztpwmi1zzwnyzxq="),
        "{b:?}"
    );
}
