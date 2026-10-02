//! Every way the one GET of a lookup can end other than with a 200, and
//! what the lookup must then say (`LookupFailure`): its words, whether a
//! later check can get past it (`transient`), the host a secure connection
//! failed with. Each lookup that asks a server -- crates.io (cargo), PyPI
//! (pipx), the Ollama registry, and the standalone tools' three published
//! versions -- holds itself to this one table in its own tests
//! (`hold_to_the_table`), so the six cannot come to tell the same failure
//! apart differently.

use super::LookupFailure;
use crate::http::{HttpError, HttpResponse, MockHttpClient};
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

/// How the GET ends: no answer (`HttpError`, with its words as the reason
/// shows them), or an answer with a status other than 200.
pub(crate) enum Ending {
    Fails(HttpError, &'static str),
    Answers(u16),
}

/// Each ending with whether it is transient and the host it names.
pub(crate) fn cases() -> Vec<(Ending, bool, Option<&'static str>)> {
    vec![
        (
            Ending::Fails(
                HttpError::Network("dns error: failed to lookup address information".to_string()),
                "network error: dns error: failed to lookup address information",
            ),
            true,
            None,
        ),
        (
            Ending::Fails(
                HttpError::Timeout(Duration::from_secs(30)),
                "timed out after 30s",
            ),
            true,
            None,
        ),
        (
            Ending::Fails(
                HttpError::Tls {
                    host: "example.org".to_string(),
                    detail: "invalid peer certificate: UnknownIssuer".to_string(),
                },
                "secure connection to example.org failed: invalid peer certificate: UnknownIssuer",
            ),
            false,
            Some("example.org"),
        ),
        (
            Ending::Fails(
                HttpError::Refused("refusing to follow a redirect".to_string()),
                "refused: refusing to follow a redirect",
            ),
            false,
            None,
        ),
        (
            Ending::Fails(
                HttpError::BodyTooLarge { limit: 8 },
                "response body is larger than the 8-byte limit",
            ),
            false,
            None,
        ),
        (Ending::Answers(408), true, None),
        (Ending::Answers(429), true, None),
        (Ending::Answers(500), true, None),
        (Ending::Answers(503), true, None),
        (Ending::Answers(599), true, None),
        (Ending::Answers(301), false, None),
        (Ending::Answers(403), false, None),
        (Ending::Answers(404), false, None),
        (Ending::Answers(410), false, None),
    ]
}

/// Runs `lookup` once per case, over a fresh `MockHttpClient` that ends
/// the GET to `url` that way, and asserts the one request went out and the
/// failure is the table's: `"{failed}: {error}"` for no answer,
/// `"{answerer} returned status {status}"` for another status.
pub(crate) async fn hold_to_the_table<T, F, Fut>(url: &str, failed: &str, answerer: &str, lookup: F)
where
    F: Fn(Arc<MockHttpClient>) -> Fut,
    Fut: Future<Output = Result<T, LookupFailure>>,
{
    for (ending, transient, host) in cases() {
        let http = Arc::new(MockHttpClient::new());
        let reason = match ending {
            Ending::Fails(error, words) => {
                http.fail_with(url, error);
                format!("{failed}: {words}")
            }
            Ending::Answers(status) => {
                http.respond(
                    url,
                    HttpResponse {
                        status,
                        body: "{}".to_string(),
                    },
                );
                format!("{answerer} returned status {status}")
            }
        };
        let Err(failure) = lookup(http.clone()).await else {
            panic!("{url}: a GET that did not end in a 200 is a failed lookup ({reason})");
        };
        assert_eq!(
            failure,
            LookupFailure {
                reason: reason.clone(),
                transient,
                secure_connection: host.map(str::to_string),
            },
            "{url}: {reason}"
        );
        assert_eq!(http.calls(), vec![url.to_string()], "{reason}");
    }
}
