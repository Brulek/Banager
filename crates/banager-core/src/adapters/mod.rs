mod python_version;
use crate::events::{EventSink, OpId};
use crate::model::{
    ArtifactKey, InstalledArtifact, InstanceNote, ManagerInstance, OpRequest, Outcome, Plan,
    PlanAction, ReadOnlyReason, Reconciled, ResourceLock, SearchHit, Unavailable, UninstallBlocked,
    UninstallUnsafeReason, UpdateBlocked, UpdateCandidate, UpdateChannel, Warning,
};
use crate::runner::{CommandRunner, CommandSpec, HostEnv, LineCallback, OutputUse, RunLine};
use async_trait::async_trait;
use percent_encoding::{utf8_percent_encode, AsciiSet, CONTROLS};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

pub mod brew;
pub mod cargo;
pub mod npm;
pub mod ollama;
pub mod pip;
pub mod pipx;
pub mod standalone;
pub mod uv;

pub(crate) mod read_file;
pub(crate) mod sanity;

#[cfg(test)]
pub(crate) mod lookup_cases;
#[cfg(test)]
mod robustness;

/// Options a caller passes down to `check_updates`. Adapters ignore fields
/// that do not apply to them; a new field must never change behaviour for an
/// adapter that does not read it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckOptions {
    /// Homebrew only: include casks that update themselves (`brew outdated --greedy`).
    pub include_self_updating: bool,
    /// When the refresh round this check is part of began. Set by
    /// `Session`'s `refresh_round` over whatever the caller passed; `None`,
    /// a check made outside a refresh, counts as a round beginning now.
    /// Homebrew only: see `UpdateRecord::unreported_failure`. Never on the
    /// wire (`serde(skip)`): an `Instant` means nothing outside this
    /// process.
    #[serde(skip)]
    pub round_started: Option<std::time::Instant>,
}

/// Everything one `check_updates` call learned: the per-package
/// candidates, and anything it found out *about the source itself* while
/// looking.
///
/// A struct rather than a tuple because the second element needs a name
/// that says what it is, and because a third thing to report later must
/// not mean editing seven adapters' signatures again -- this is the
/// second time this phase has touched this one (`CheckOptions` was the
/// first).
///
/// Core-internal on purpose: `check_updates` has exactly one production
/// caller (`Session::refresh`) and never crosses IPC, so this type needs
/// **no TypeScript mirror**. That is what makes it cheaper than the
/// alternatives -- `src/lib/types.ts` is hand-written, and a mismatch
/// there is silent at compile time and wrong at runtime. What does reach
/// the front end is `InstanceNote`, which already had to.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CheckOutcome {
    pub candidates: Vec<UpdateCandidate>,
    /// What the source said about itself. Empty for every adapter but
    /// brew, which reports `IndexMayBeStale` when `brew update` failed.
    /// A `brew update` still running is not a note here: brew then reads
    /// no candidates at all and returns `AdapterError::IndexUpdating`,
    /// which `Session::refresh` turns into `InstanceNote::IndexUpdating`.
    pub notes: Vec<InstanceNote>,
}

/// Lets the seven adapters with nothing to report about themselves write
/// `Ok(out.into())` instead of naming a struct they never fill.
///
/// This conversion is the reason `CheckOutcome` is not a third block of
/// verbatim seven-adapter duplication: what repeats is one call, not a
/// literal with a `notes: Vec::new()` in it.
impl From<Vec<UpdateCandidate>> for CheckOutcome {
    fn from(candidates: Vec<UpdateCandidate>) -> CheckOutcome {
        CheckOutcome {
            candidates,
            notes: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct AdapterMeta {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub platforms: Vec<String>,
    pub homepage: String,
    pub schema_version: u32,
    pub verified_versions: Vec<String>,
}

impl AdapterMeta {
    pub fn from_toml(s: &str) -> Result<AdapterMeta, toml::de::Error> {
        toml::from_str(s)
    }

    /// `None` when this adapter's metadata lists no verified versions, or
    /// when `detected` is among them (or absent). `Some(detected)` when it
    /// is not, so the UI can mark the source as running an unverified
    /// version (spec §4.1).
    ///
    /// Defined once, here, rather than per adapter: every `detect()` in the
    /// workspace calls this, so the rule can only ever mean one thing. An
    /// earlier draft of this phase had seven copies of it.
    pub fn unverified_version(&self, detected: &Option<String>) -> Option<String> {
        detected
            .as_ref()
            .filter(|v| !self.verified_versions.is_empty() && !self.verified_versions.contains(v))
            .cloned()
    }
}

/// A stable fingerprint for a preview's per-tool record. Sort objects even
/// when another workspace crate enables serde_json's preserve_order feature.
/// Arrays retain their order; no receipt contents leave the backend.
fn plan_basis(mut value: serde_json::Value) -> String {
    use sha2::{Digest, Sha256};
    fn sort(value: &mut serde_json::Value) {
        match value {
            serde_json::Value::Object(object) => {
                object.sort_keys();
                object.values_mut().for_each(sort);
            }
            serde_json::Value::Array(array) => array.iter_mut().for_each(sort),
            _ => {}
        }
    }
    sort(&mut value);
    format!("{:x}", Sha256::digest(value.to_string().as_bytes()))
}

#[derive(Debug, thiserror::Error)]
pub enum AdapterError {
    #[error("runner: {0}")]
    Runner(#[from] crate::runner::RunnerError),
    #[error("parse: {0}")]
    Parse(String),
    #[error("command failed (exit {code:?}): {stderr}")]
    CommandFailed { code: Option<i32>, stderr: String },
    #[error("refused: {0}")]
    Refused(String),
    #[error("invalid name: {0}")]
    InvalidName(String),
    #[error("unsupported: {0}")]
    Unsupported(String),
    /// `Session::issue_plan`'s actionability gate (spec §2.5) refused an
    /// operation the front end should never have offered: both pages hide
    /// every control for an instance that fails `read_only_reason.is_none()
    /// && status.unavailable.is_none()`. A user sees this only through a
    /// stale snapshot or a genuine TOCTOU (detect and the click can be
    /// seconds to hours apart).
    ///
    /// A dedicated variant rather than a `Refused(String)` built from
    /// `format!("{:?}", ...)` because this one *can* reach a real person,
    /// and a `{:?}` of two Rust enums is the worst possible thing to show
    /// someone who does not read Rust -- `ipc.rs` maps these two fields to
    /// the same localised copy the source's own notice already uses,
    /// instead of this variant's own `Display` (below), which stays plain
    /// English for logs and test failure output.
    ///
    /// Lives on `AdapterError` rather than a new `SessionError` because
    /// `issue_plan` constructs it directly, before ever calling
    /// `adapter.plan()` -- no `Adapter` trait method's signature changes,
    /// so no adapter *needs* to know this variant exists. npm's `plan()`
    /// returns it anyway, for a prefix that stopped being writable after
    /// `detect` looked: that is the gate's own read-only reason, found late.
    #[error("not actionable (read-only: {read_only:?}, unavailable: {unavailable:?})")]
    NotActionable {
        read_only: Option<ReadOnlyReason>,
        unavailable: Option<Unavailable>,
    },
    /// `Session::issue_plan` was asked to plan an `Upgrade` of a package
    /// whose own update candidate says the tool will refuse it
    /// (`UpdateCandidate.blocked`): the per-package half of the same gate
    /// `NotActionable` is the per-source half of. The Updates page hides
    /// the button for such a row, so like `NotActionable` this reaches a
    /// person only through a stale page. `plan_operation_error` in
    /// src-tauri/src/ipc.rs sends it as `{"kind": "update_blocked"}` with
    /// the reason, never as this `Display`.
    #[error("the tool will refuse to update this package ({reason:?})")]
    UpdateBlocked { reason: UpdateBlocked },
    /// `Session::issue_plan` was asked to plan an `Uninstall` of a package
    /// whose own inventory entry says the tool will refuse it
    /// (`InstalledArtifact.uninstall_blocked`). The uninstall twin of
    /// `UpdateBlocked`: the Installed page hides the button for such a
    /// row, so this reaches a person only through a stale page, and
    /// `plan_operation_error` in src-tauri/src/ipc.rs sends it as
    /// `{"kind": "uninstall_blocked"}` with the reason.
    #[error("the tool will refuse to uninstall this package ({reason:?})")]
    UninstallBlocked { reason: UninstallBlocked },
    /// A path-list uninstall's `plan()` (`StandaloneAdapter`, through
    /// `removal::plan_removal`) refused one of the paths the recipe names:
    /// `reason` is which check failed, `path` the path with the home folder
    /// abbreviated to `~` (for `OverlapsKept`, the kept path it concerns).
    /// Nothing was moved. Sent to the front end by
    /// `plan_operation_error` (src-tauri/src/ipc.rs) as
    /// `{"kind":"uninstall_unsafe","path":…,"reason":<snake_case>}`, which
    /// `parseUninstallUnsafe` in src/lib/sources.ts words as one of six
    /// sentences -- never as this `Display`, which is for logs. No
    /// `execute` returns it: the same checks failing at run time are
    /// `Fault::PathChanged` (`removal::execute_removal`).
    #[error("unsafe to remove {path}: {reason:?}")]
    UninstallUnsafe {
        path: String,
        reason: UninstallUnsafeReason,
    },
    /// `Session::issue_plan` was asked to plan against an instance that is
    /// not in the snapshot at all -- the source was removed between the
    /// refresh that drew the row and the click. The `issue_plan` twin of
    /// `SubmitError::SourceGone`, and sent to the front end the same way
    /// (`{"kind": "source_gone"}`), so both read as the same localised
    /// sentence.
    ///
    /// It used to be a `Refused(format!("unknown instance ..."))`, which
    /// `ipc.rs` could only pass on as this project's English: a free-form
    /// string gives the front end nothing to translate. The `Display`
    /// keeps that wording for logs and test failure output.
    #[error("unknown instance {instance_id}")]
    SourceGone { instance_id: String },
    /// `Session::issue_listed_plan` -- the window's way to plan -- was
    /// asked for an `Upgrade` of a package the snapshot offers no update
    /// for, or an `Uninstall` of one it does not list as installed. The
    /// window plans only rows it was shown, so this reaches a person only
    /// through a row a refresh has just replaced; `plan_operation_error`
    /// in src-tauri/src/ipc.rs sends it as `{"kind": "not_listed"}`.
    #[error("the snapshot does not list what this request names")]
    NotListed,
    /// The source's package catalogue is being rewritten right now, so the
    /// adapter did not read it. Only brew returns it: its `inventory`
    /// while a `brew update` is running for that instance, its
    /// `check_updates` when the `brew update` it started outlasts the
    /// refresh's patience, and its uninstall `plan` when an update is
    /// running before or begins during its `brew uses`. That update
    /// git-merges Homebrew's own Ruby code and `curl`s the package list
    /// over the file `brew info`, `brew outdated` and `brew uses` read, so
    /// an answer read alongside it could be an error or, worse, a parse of
    /// a half-written file that succeeds.
    ///
    /// Not a failure. `Session::refresh` turns it into
    /// `InstanceNote::IndexUpdating` rather than a `SourceError`, and
    /// carries that instance's previous rows forward through the same
    /// branches a failed read uses. From `plan` it reaches the uninstall
    /// dialog as the `index_updating` kind (`plan_operation_error` in
    /// src-tauri/src/ipc.rs), which asks the user to try again shortly.
    #[error("the package index is being updated")]
    IndexUpdating,
}

/// Percent-encodes one untrusted value for interpolation into a single URL
/// **path segment**.
///
/// Every registry lookup in this crate builds its URL by interpolating a
/// name Banager did not choose: Ollama's model references arrive in the body
/// of `GET {host}/api/tags`, cargo's crate names come out of
/// `.crates2.json`'s keys, pipx's package names out of `pipx list --json`.
/// Raw, such a name can change *which resource the URL addresses* rather
/// than merely name it: `/` adds path segments, `?` starts a query string,
/// `#` truncates the path at a fragment, and `%` lets the value smuggle in
/// its own encoding. The host and scheme are fixed before any interpolation
/// point in all three URLs, so this is not header or origin injection — the
/// request still goes to the right server, at the wrong path.
///
/// The set mirrors the WHATWG URL standard's path percent-encode set plus
/// `/` and `%` — the same set the `url` crate applies in
/// `PathSegmentsMut::push`. Ordinary names (`hexyl`, `qwen3.8:27b-mlx`'s
/// parts) pass through byte-for-byte, so no existing URL changes shape.
///
/// An empty value is an error rather than an empty encoding, because it
/// collapses the path: `https://crates.io/api/v1/crates/` is the crate
/// index, not a crate. The error is a human-readable reason, since all three
/// callers turn a failure into one `checkable: false` row.
pub(crate) fn url_path_segment(value: &str) -> Result<String, String> {
    if value.is_empty() {
        return Err("refusing to build a registry url from an empty name".to_string());
    }
    Ok(utf8_percent_encode(value, URL_PATH_SEGMENT).to_string())
}

/// The WHATWG URL path percent-encode set, plus `/` and `%`. Spelled out
/// rather than imported so the exact bytes are reviewable here.
const URL_PATH_SEGMENT: &AsciiSet = &CONTROLS
    // fragment percent-encode set
    .add(b' ')
    .add(b'"')
    .add(b'<')
    .add(b'>')
    .add(b'`')
    // path percent-encode set
    .add(b'#')
    .add(b'?')
    .add(b'{')
    .add(b'}')
    // and what keeps the value inside its own segment
    .add(b'/')
    .add(b'%');

/// One "Banager could not find out" row: the item is listed at the version
/// it is installed at, with `checkable: false` and the reason attached.
///
/// Every adapter that reaches a registry answers a failed lookup this way,
/// and this function is why they cannot drift apart again. The alternative
/// three answers all shipped at once: cargo built rows like these, pip, uv
/// and pipx's native path returned `Err` (failing the whole source), and
/// npm returned an empty list -- which on the Updates page is
/// indistinguishable from "everything is up to date". A failed lookup is
/// none of those things: it is not knowing.
///
/// `target` is the installed version, not a guess. `UpdateCandidate` has no
/// "unknown" target, and any other value would be a version Banager is
/// claiming exists.
///
/// Its warnings: the reason (`Warning::Message`), then
/// `Warning::SecureConnectionFailed` where no secure connection could be
/// set up (`LookupFailure::secure_connection`),
/// `Warning::TransientLookupFailure` where the failure is one checking
/// again can get past (`LookupFailure::transient`), and
/// `Warning::NotLookedUpHere` where no lookup was made, by design
/// (`LookupFailure::not_looked_up`).
pub(crate) fn uncheckable_candidate(
    key: ArtifactKey,
    current: String,
    channel: UpdateChannel,
    failure: impl Into<LookupFailure>,
) -> UpdateCandidate {
    let failure = failure.into();
    let mut warnings = vec![Warning::Message(failure.reason)];
    if let Some(host) = failure.secure_connection {
        warnings.push(Warning::SecureConnectionFailed { host });
    }
    if failure.transient {
        warnings.push(Warning::TransientLookupFailure);
    }
    if failure.not_looked_up {
        warnings.push(Warning::NotLookedUpHere);
    }
    UpdateCandidate {
        key,
        target: current.clone(),
        current,
        channel,
        checkable: false,
        warnings,
        blocked: None,
        download_bytes: None,
    }
}

/// Why a lookup could not be made: the words for its row (`reason`, the
/// row's `Warning::Message`), and whether checking again can get past it
/// (`transient`, the row's `Warning::TransientLookupFailure`).
///
/// `transient` is claimed only where it is known: the request could not
/// connect or got no answer in time, the registry answered with a status
/// that says "not now" (408, 429, 5xx), or the tool's own words say the
/// network failed. Anything else -- a certificate rustls would not accept,
/// a redirect or host the client refuses, a 404, an answer that would not
/// parse, a version that
/// could not be read, a tool not looked up on this Mac (`not_looked_up`),
/// a command that did
/// not finish -- is not known to mend itself, and a warning that asks the
/// person to check again would then never go away. A plain `String` is
/// such a failure (`From<String>`), so `?` on a helper that fails with
/// words alone says nothing it does not know.
///
/// `secure_connection` is the host a request reached but could not set up
/// a secure connection with (`HttpError::Tls`), for the row's words
/// (`Warning::SecureConnectionFailed`).
///
/// `not_looked_up` is no failure at all: Banager does not look the tool up
/// on this Mac, by design, and asked nothing (`LookupFailure::not_looked_up`,
/// the row's `Warning::NotLookedUpHere`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LookupFailure {
    pub(crate) reason: String,
    pub(crate) transient: bool,
    pub(crate) secure_connection: Option<String>,
    pub(crate) not_looked_up: bool,
}

impl From<String> for LookupFailure {
    fn from(reason: String) -> Self {
        LookupFailure {
            reason,
            transient: false,
            secure_connection: None,
            not_looked_up: false,
        }
    }
}

impl LookupFailure {
    /// No lookup, by design: Banager does not look this tool up on this
    /// Mac, and made no request -- the models of an Ollama on another Mac,
    /// an Ollama model from another registry or whose local manifest is
    /// not there or is in a protected place
    /// (`OllamaAdapter::check_one_model`, `compare_digests`), Antigravity
    /// CLI on an Intel Mac, Claude Code whose settings are kept in a
    /// protected place. `reason` says which, as the row's `Message`; never
    /// transient, as the next check makes no request either.
    pub(crate) fn not_looked_up(reason: String) -> Self {
        LookupFailure {
            not_looked_up: true,
            ..LookupFailure::from(reason)
        }
    }

    /// A request that got no answer: `"{what}: {error}"`, transient when
    /// the network failed or the time ran out (`HttpError::Network`,
    /// `HttpError::Timeout`). Not for what the next check meets again: a
    /// secure connection rustls would not set up (`HttpError::Tls`, whose
    /// host the row names), a request the client refuses -- a redirect, a
    /// host off its list (`HttpError::Refused`) -- a body over the size
    /// limit, or a test's missing canned answer.
    pub(crate) fn request(what: &str, error: &crate::http::HttpError) -> Self {
        use crate::http::HttpError;
        LookupFailure {
            reason: format!("{what}: {error}"),
            transient: matches!(error, HttpError::Network(_) | HttpError::Timeout(_)),
            secure_connection: match error {
                HttpError::Tls { host, .. } => Some(host.clone()),
                _ => None,
            },
            not_looked_up: false,
        }
    }

    /// An answer other than 200: `reason` as the caller words it,
    /// transient for 408 (Request Timeout), 429 (Too Many Requests) and
    /// any server error (5xx). A 404 is the registry saying it has no such
    /// thing, which the next check will say again.
    pub(crate) fn status(reason: String, status: u16) -> Self {
        LookupFailure {
            reason,
            transient: status == 408 || status == 429 || (500..=599).contains(&status),
            secure_connection: None,
            not_looked_up: false,
        }
    }

    /// A failure a tool put into `words` -- its stderr, or the error its
    /// update check reported -- worded as `reason`: transient where those
    /// words say the network failed (`says_network_failed`).
    pub(crate) fn words(reason: String, words: &str) -> Self {
        LookupFailure {
            reason,
            transient: says_network_failed(words),
            secure_connection: None,
            not_looked_up: false,
        }
    }
}

/// Four lookups at a time per source (fewer where a host's own limit in
/// `get_ok` says so: crates.io takes one), with a two-minute budget for
/// this registry phase. Completed answers survive a deadline; unfinished and
/// unstarted lookups are transient failures, never up-to-date answers.
/// Batches preserve inventory order without spawning detached tasks.
pub(crate) async fn registry_checks<F, T>(lookups: Vec<F>) -> Vec<Result<T, LookupFailure>>
where
    F: std::future::Future<Output = Result<T, LookupFailure>>,
{
    registry_checks_until(
        lookups,
        tokio::time::Instant::now() + Duration::from_secs(120),
    )
    .await
}

async fn registry_checks_until<F, T>(
    lookups: Vec<F>,
    deadline: tokio::time::Instant,
) -> Vec<Result<T, LookupFailure>>
where
    F: std::future::Future<Output = Result<T, LookupFailure>>,
{
    async fn one<F, T>(
        future: Option<F>,
        deadline: tokio::time::Instant,
    ) -> Option<Result<T, LookupFailure>>
    where
        F: std::future::Future<Output = Result<T, LookupFailure>>,
    {
        let future = future?;
        let timed_out = || {
            LookupFailure::request(
                "registry request failed",
                &crate::http::HttpError::Timeout(Duration::from_secs(120)),
            )
        };
        if tokio::time::Instant::now() >= deadline {
            return Some(Err(timed_out()));
        }
        Some(
            tokio::time::timeout_at(deadline, future)
                .await
                .unwrap_or_else(|_| Err(timed_out())),
        )
    }
    let mut iter = lookups.into_iter();
    let mut answers = Vec::new();
    loop {
        let Some(first) = iter.next() else { break };
        let (a, b, c, d) = tokio::join!(
            one(Some(first), deadline),
            one(iter.next(), deadline),
            one(iter.next(), deadline),
            one(iter.next(), deadline)
        );
        answers.extend([a, b, c, d].into_iter().flatten());
    }
    answers
}

// Shared across instances: even two installations of a source respect
// the same host limit. The registry phase deadline includes permit waits.
static PYPI_LOOKUPS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(4);
// One at a time, as before: crates.io asks API users for at most one
// request per second (crates.io/data-access, "crates.io API"), so its
// lookups never overlap; only the phase budget is new for Cargo.
static CARGO_LOOKUPS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);
static OLLAMA_LOOKUPS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(4);

/// The one GET of a lookup that asks a server, whose only good answer is a
/// 200: a request with no answer fails as `LookupFailure::request(failed,
/// ..)`, any other status as `"{answerer} returned status {status}"`
/// (`LookupFailure::status`). Every such lookup -- crates.io, PyPI, the
/// Ollama registry, the standalone tools' published versions -- goes
/// through here, so which of its failures a later check can get past is
/// decided in one place (`lookup_cases` holds each caller to it).
pub(crate) async fn get_ok(
    http: &dyn crate::http::HttpClient,
    url: String,
    headers: Vec<(String, String)>,
    failed: &str,
    answerer: &str,
) -> Result<crate::http::HttpResponse, LookupFailure> {
    let limiter = match url::Url::parse(&url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_owned))
        .as_deref()
    {
        Some("pypi.org") => Some(&PYPI_LOOKUPS),
        Some("crates.io") => Some(&CARGO_LOOKUPS),
        Some("registry.ollama.ai") => Some(&OLLAMA_LOOKUPS),
        _ => None,
    };
    let _permit = match limiter {
        Some(limiter) => Some(
            limiter
                .acquire()
                .await
                .expect("registry semaphore stays open"),
        ),
        None => None,
    };
    let resp = http
        .send(crate::http::HttpRequest {
            method: "GET",
            url,
            headers,
            timeout: Duration::from_secs(30),
        })
        .await
        .map_err(|e| LookupFailure::request(failed, &e))?;
    if resp.status != 200 {
        return Err(LookupFailure::status(
            format!("{answerer} returned status {}", resp.status),
            resp.status,
        ));
    }
    Ok(resp)
}

/// Whether `words` say the network failed: a name that would not resolve,
/// a connection refused, reset or never made, a network or host out of
/// reach, a request that timed out -- in the words curl, Node and npm,
/// Python's urllib3 (pip, pipx), and Rust's HTTP clients (cargo, uv, the
/// standalone tools) use for it. Several words each, or an error code with
/// its boundaries, so a package named `timeout` is never taken for one. Not
/// a certificate or TLS error, which a proxy can make permanent, nor a
/// status the registry answered with: those say something else.
pub(crate) fn says_network_failed(words: &str) -> bool {
    const PHRASES: [&str; 16] = [
        "could not resolve host",
        "couldn't resolve host",
        "timed out",
        "connection refused",
        "connection reset",
        "connection aborted",
        "remote end closed connection without response",
        "network is unreachable",
        "no route to host",
        "temporary failure in name resolution",
        "nodename nor servname provided",
        "failed to establish a new connection",
        "failed to lookup address",
        "dns error",
        "error sending request",
        "network error",
    ];
    const CODES: [&str; 6] = [
        "ENOTFOUND",
        "EAI_AGAIN",
        "ECONNREFUSED",
        "ECONNRESET",
        "ETIMEDOUT",
        "ENETUNREACH",
    ];
    let lower = words.to_lowercase();
    PHRASES.iter().any(|phrase| lower.contains(phrase))
        || words
            .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .any(|token| CODES.contains(&token))
}

/// One `uncheckable_candidate` per installed item, for the adapters whose
/// lookup is a single command covering everything at once (`pip list
/// --outdated`, `uv tool list --outdated`, `pipx list --outdated`, `npm
/// outdated -g`). When that one command cannot reach the index, nothing is
/// known about *any* of them, so every installed item gets a row rather
/// than the source reporting an error and the page showing nothing.
pub(crate) fn uncheckable_from_inventory(
    installed: &[InstalledArtifact],
    channel: UpdateChannel,
    failure: &LookupFailure,
) -> Vec<UpdateCandidate> {
    installed
        .iter()
        .map(|a| uncheckable_candidate(a.key.clone(), a.version.clone(), channel, failure.clone()))
        .collect()
}

/// Why the lookup could not be made, as one line to hang on those rows.
///
/// The tool's own first line of stderr, because that is the only thing that
/// distinguishes "your network is down" from "this index is refusing you"
/// -- and because `Warning::Message` is shown verbatim (spec §6 backlogs
/// localising it), a whole multi-line stderr dump would become the row's
/// description. `what` names the command for the case where the tool exits
/// non-zero and says nothing at all, which would otherwise leave the row
/// with an empty reason.
pub(crate) fn lookup_failure_reason(what: &str, code: Option<i32>, stderr: &str) -> String {
    const MAX: usize = 200;
    let line = stderr
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("");
    if line.is_empty() {
        return match code {
            Some(code) => format!("`{what}` exited with code {code}"),
            None => format!("`{what}` did not finish"),
        };
    }
    match line.char_indices().nth(MAX) {
        Some((cut, _)) => format!("{what}: {}...", &line[..cut]),
        None => format!("{what}: {line}"),
    }
}

/// Matches `^[A-Za-z0-9@._+/-]+$`, rejects names starting with `-`, `/` or
/// `.`, rejects a `..` path segment anywhere, and rejects a trailing `.rb`
/// (implemented by hand instead of pulling in the `regex` crate, since this
/// is the only place in the crate that needs pattern matching). The `/`,
/// leading-`.`, `..`-segment and `.rb`-suffix rules exist specifically so
/// `brew install --formula {name}` can never be handed a path: without them
/// `validate_package_name("/tmp/evil.rb")` — or a tap-relative
/// `"../../tmp/evil.rb"` — would pass, and Homebrew treats a `.rb`-suffixed
/// argument as a local formula file to load and run, not a formula name to
/// look up.
pub fn validate_package_name(name: &str) -> Result<(), AdapterError> {
    if name.is_empty()
        || name.starts_with('-')
        || name.starts_with('/')
        || name.starts_with('.')
        || name.ends_with(".rb")
        || name.split('/').any(|segment| segment == "..")
    {
        return Err(AdapterError::InvalidName(name.to_string()));
    }
    let valid = name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '@' | '.' | '_' | '+' | '/' | '-'));
    if !valid {
        return Err(AdapterError::InvalidName(name.to_string()));
    }
    Ok(())
}

/// Whether `key` is installed, and at what version, given one `inventory()`
/// answer.
///
/// The rule is `kind` **and** `name`, both exactly: a source can hold two
/// artifacts that share a name and differ in kind (Homebrew's `python`
/// formula and its `python` cask), and answering "present, at 3.14.2" for
/// the wrong one would make Banager report an uninstall as having failed,
/// or an install as having already happened.
///
/// `ArtifactKey::instance_id` is deliberately *not* compared. An adapter
/// reconciles against the inventory of the instance it was handed, and the
/// caller is the one that pairs the two; `ensure_instance_match` is where
/// that pairing is checked, on the write path where it matters.
///
/// Extracted because all seven adapters had this body copied out by hand
/// and npm's copy had already lost the `kind` half of the rule -- harmless
/// only because every npm artifact happens to be a `Package`, and nothing
/// kept the seventh copy in step with the other six.
pub fn reconcile_from(artifacts: Vec<InstalledArtifact>, key: &ArtifactKey) -> Reconciled {
    match artifacts
        .into_iter()
        .find(|a| a.key.kind == key.kind && a.key.name == key.name)
    {
        Some(a) => Reconciled {
            present: true,
            version: Some(a.version),
        },
        None => Reconciled {
            present: false,
            version: None,
        },
    }
}

/// The tool environment a source's own program is in, when the source
/// installed itself as one of its tools: `program`, every link followed
/// (`protected::look::real_path`, never into a protected place), is
/// `<tools>/<name>/bin/<file>` with `<tools>` a folder named
/// `tools_folder` -- pipx's `venvs` (`pipx install pipx`, pipx 1.17's
/// `self_install.py`), uv's `tools` (`uv tool install uv`). Removing that
/// tool removes the program every other tool of the source is updated and
/// uninstalled with (`UninstallBlocked::SourceProgram`, as npm's own npm).
/// `None` for a program anywhere else, or one that could not be followed.
pub(crate) fn own_tool_environment(
    program: &std::path::Path,
    tools_folder: &str,
) -> Option<std::path::PathBuf> {
    use crate::protected::{self, Protected};
    let real = protected::look::real_path(program, &Protected::of_this_process()).ok()?;
    let bin = real.parent()?;
    let environment = bin.parent()?;
    let folder = environment.parent()?.file_name()?;
    let named = |name: &std::ffi::OsStr, as_: &str| {
        protected::same_path(std::path::Path::new(name), std::path::Path::new(as_))
    };
    (named(bin.file_name()?, "bin") && named(folder, tools_folder))
        .then(|| environment.to_path_buf())
}

/// `artifacts` with the one whose environment is `own` marked
/// `UninstallBlocked::SourceProgram`, unless another reason already
/// blocks it: its `path`, every link followed as `own`'s were, is `own`
/// as the disk compares names. Only a path whose folder has `own`'s name
/// is followed.
pub(crate) fn block_own_tool(
    mut artifacts: Vec<InstalledArtifact>,
    own: Option<&std::path::Path>,
) -> Vec<InstalledArtifact> {
    use crate::protected::{self, Protected};
    if let Some(own) = own {
        let protected = Protected::of_this_process();
        let is_own = |path: &std::path::Path| {
            path.file_name().zip(own.file_name()).is_some_and(|(a, b)| {
                protected::same_path(std::path::Path::new(a), std::path::Path::new(b))
            }) && protected::look::real_path(path, &protected)
                .is_ok_and(|real| protected::same_path(&real, own))
        };
        for artifact in &mut artifacts {
            if artifact.path.as_deref().is_some_and(is_own) {
                artifact
                    .uninstall_blocked
                    .get_or_insert(UninstallBlocked::SourceProgram);
            }
        }
    }
    artifacts
}

/// What every adapter but Homebrew's answers an `OpKind::Link` with: only a
/// Homebrew formula is linked (`brew link --formula --force`, `NoAnswer::link_fixes`).
/// Asked first in each `plan()`, before anything is read or run, and in
/// every `match` over the kind after it.
pub(crate) fn links_nothing(adapter_id: &str) -> AdapterError {
    AdapterError::Unsupported(format!(
        "{adapter_id} links nothing: only a Homebrew formula is linked"
    ))
}

/// Refuses a plan whose `OpRequest` names a different instance than the one
/// the adapter was handed.
///
/// Every writable adapter's `plan()` opens with this, before it validates a
/// name or builds a single argument: the request carries the instance the
/// user acted on, `inst` is the instance the caller resolved, and if they
/// disagree then the command about to be built would run against the wrong
/// prefix -- a `brew uninstall` in `/usr/local` for a row the user clicked
/// in `/opt/homebrew`. Refusing costs nothing and is not recoverable
/// further down.
///
/// `pip` is the one adapter that does not call this: its `plan()` refuses
/// every operation outright, so there is no wrong prefix to protect.
pub fn ensure_instance_match(req: &OpRequest, inst: &ManagerInstance) -> Result<(), AdapterError> {
    if req.instance_id != inst.id {
        return Err(AdapterError::Refused(format!(
            "plan requested for instance {} but given instance {}",
            req.instance_id, inst.id
        )));
    }
    Ok(())
}

#[async_trait]
pub trait Adapter: Send + Sync {
    fn meta(&self) -> &AdapterMeta;
    async fn detect(&self, env: &HostEnv) -> Vec<ManagerInstance>;
    async fn inventory(
        &self,
        inst: &ManagerInstance,
    ) -> Result<Vec<InstalledArtifact>, AdapterError>;
    async fn check_updates(
        &self,
        inst: &ManagerInstance,
        opts: &CheckOptions,
    ) -> Result<CheckOutcome, AdapterError>;
    async fn search(
        &self,
        inst: &ManagerInstance,
        query: &str,
    ) -> Result<Vec<SearchHit>, AdapterError>;
    async fn plan(&self, inst: &ManagerInstance, req: &OpRequest) -> Result<Plan, AdapterError>;
    async fn execute(
        &self,
        plan: &Plan,
        sink: Arc<dyn EventSink>,
        op_id: OpId,
        cancel: CancellationToken,
    ) -> Result<Outcome, AdapterError>;
    async fn reconcile(
        &self,
        inst: &ManagerInstance,
        key: &ArtifactKey,
    ) -> Result<Reconciled, AdapterError>;
    /// The reading `run_operation` (ops/mod.rs) takes after an uninstall,
    /// which asks one question only: is the artifact still there
    /// (`Reconciled::present`)? Every other operation is verified with
    /// `reconcile`. Defaults to `reconcile`, which is what every source's
    /// uninstall was verified with before this existed.
    ///
    /// `StandaloneAdapter` (adapters/standalone/mod.rs) is what this is
    /// for, and its path-list uninstall (phase 4 step C) brings the one
    /// production override: its `reconcile` refuses a launcher it cannot
    /// read a version from -- after an upgrade that exits 0, such a
    /// launcher is no evidence of success -- while after an uninstall that
    /// launcher, the dangling link such an uninstall leaves when it stops
    /// partway, is exactly the evidence that the tool is still there.
    ///
    /// An adapter that cannot tell -- a permission error where the
    /// launcher should be, say -- answers `Err`, never `present: false`:
    /// `run_operation` turns an `Err` into `Unconfirmed`, so "could not
    /// tell" is never reported as a finished uninstall.
    ///
    /// `plan` is the plan the uninstall carried out -- the one `execute`
    /// was handed -- for an adapter whose answer depends on what it did:
    /// the standalone path-list uninstall reads which paths it moved, and
    /// counts a listed path that is there afterwards as the tool still
    /// there (`removal::left_behind`). The default ignores it.
    async fn reconcile_after_uninstall(
        &self,
        inst: &ManagerInstance,
        key: &ArtifactKey,
        _plan: &Plan,
    ) -> Result<Reconciled, AdapterError> {
        self.reconcile(inst, key).await
    }

    /// Whether one update of this source can update another of its
    /// packages along with it, so that a later update of the same batch
    /// may find its package already new (`AlreadyUpdated::ByEarlierUpdate`,
    /// ops/mod.rs). Homebrew does: it upgrades a formula's outdated
    /// dependencies before the formula. No other source does -- npm's,
    /// cargo's, pipx's and uv's tools are each installed apart, a model is
    /// pulled by its own name, and a standalone installer is one tool -- so
    /// the default is `false`, and such a package found already new is only
    /// said to be (`AlreadyUpdated::BeforeItsTurn`).
    fn one_update_can_update_others(&self) -> bool {
        false
    }

    /// The locks a refresh reads `inst` under (`Session::refresh_round`):
    /// its detection, when last round found `inst`, and its inventory, and
    /// -- those of them `check_locks` keeps -- its update check. The ones
    /// this source's plans take for it, so that no operation another plan
    /// runs on the same files is under way while the refresh reads them --
    /// a refresh that finds one held carries last round's rows instead.
    /// The instance's own lock, by default; npm's adds the Homebrew prefix
    /// its plans take (`NpmAdapter::instance_locks`).
    fn refresh_locks(&self, inst: &ManagerInstance) -> Vec<ResourceLock> {
        vec![ResourceLock(inst.id.clone())]
    }

    /// Which of `refresh_locks` a refresh keeps for `inst`'s update check,
    /// once its inventory is read (`Session::refresh_round`); the others
    /// are let go then, so an operation waiting for one starts without
    /// waiting for the check. All of them, by default. npm's own alone:
    /// its check asks the npm registry, up to a minute, and a Homebrew
    /// operation at its prefix waits for its listing, not for that.
    fn check_locks(&self, inst: &ManagerInstance) -> Vec<ResourceLock> {
        self.refresh_locks(inst)
    }

    /// The reading after a link (`OpKind::Link`): `None` when `key` is not
    /// installed, and otherwise whether its tool says it is linked now --
    /// `brew link` exits 0 having linked nothing too (`run_operation`,
    /// `Attention::NotLinkedAfterLink`). Only Homebrew links; every other
    /// adapter refuses, as its `plan` does (`links_nothing`).
    async fn reconcile_link(
        &self,
        _inst: &ManagerInstance,
        _key: &ArtifactKey,
    ) -> Result<Option<bool>, AdapterError> {
        Err(links_nothing(&self.meta().id))
    }
}

/// Runs a plan through the runner, streaming each line to the sink, and maps
/// the result the way every adapter must: a clean exit is `Succeeded`, a
/// run that did not reach its exit is `Unconfirmed` (the operation may or
/// may not have taken effect — only `reconcile` can say), and a non-zero
/// exit is `Failed` carrying the last five stderr lines.
///
/// Three things end a run short of its exit, and all three are
/// `Unconfirmed`: the runner stopped it at the user's Cancel (`cancelled`)
/// or at the plan's deadline (`timed_out`), or a signal the run did not
/// send ended it — Activity Monitor, `kill`, a crash — which `RealRunner`
/// reports as no exit code with neither flag set (runner/real.rs, the
/// `child_code` arm of `run`). A command ended that way reported no
/// failure: whatever it wrote to stderr before it died is not a verdict,
/// and `run_operation` (ops/mod.rs) shows a `Failed` as the tool's own
/// verdict and reconciles it with nothing, so mapping it there — as this
/// did until B's Astra finding B-3 — reported an uninstall killed after
/// it had removed the tool as a failure. The `Ok(Outcome::Unconfirmed)`
/// arm judges by the reading after instead, and calls the stop
/// `Cancelled` only when the user's own token fired.
///
/// Every adapter's `execute()` calls this function to turn a finished run
/// into an `Outcome`; brew is the only one that does anything else first
/// (`refuse_if_root`, since Homebrew itself refuses to run as root). It
/// lives here so the did-not-finish rule and the five-line summary can
/// only ever mean one thing; an earlier draft of this phase had six
/// byte-identical copies of it.
pub async fn run_plan(
    runner: &Arc<dyn CommandRunner>,
    plan: &Plan,
    sink: Arc<dyn EventSink>,
    op_id: OpId,
    cancel: CancellationToken,
) -> Result<Outcome, AdapterError> {
    let sink_for_line = sink.clone();
    let on_line: LineCallback = Arc::new(move |run_line| {
        sink_for_line.emit(match run_line {
            RunLine::Output(stream, line) => crate::events::OperationEvent::Log {
                op_id,
                stream,
                line,
            },
            RunLine::Note(note) => crate::events::OperationEvent::Note { op_id, note },
        });
    });
    // The one thing this function spawns is a `Command`. A `TrashPaths`
    // plan is carried out by `StandaloneAdapter::execute` itself, item by
    // item (adapters/standalone/removal.rs); reaching here with one is a
    // bug in an adapter, refused before anything is started.
    let PlanAction::Command { program, args, env } = &plan.action else {
        return Err(AdapterError::Refused(
            "run_plan was handed a plan that runs no command (TrashPaths)".to_string(),
        ));
    };
    let spec = CommandSpec {
        program: program.clone(),
        args: args.clone(),
        env: env.clone(),
        cwd: None,
        timeout: Duration::from_secs(plan.timeout_secs),
        // A build log on its way to the log drawer. Nothing reads this
        // command's stdout as data -- the outcome comes from the exit
        // code and the last five stderr lines -- so a runaway build is
        // better shortened than turned into a failed operation.
        output_use: OutputUse::Transcript,
    };
    // The user's Cancel landed before the command started (between the
    // operation turning `Running` and here): nothing was started, so
    // nothing on the Mac changed, and the cancel is the whole story --
    // `Cancelled`, which `run_operation` lets stand. Once the command has
    // started, a cancel ends in `Unconfirmed` below, as the runner cannot
    // say how far the command got. (`RealRunner::run` checks the token
    // again before it spawns; a cancel landing between the two checks is
    // reported the cautious way, `Unconfirmed`.)
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    let output = runner.run(spec, Some(on_line), cancel).await?;
    if output.cancelled || output.timed_out {
        return Ok(Outcome::Unconfirmed);
    }
    match output.exit_code {
        Some(0) => Ok(Outcome::Succeeded),
        // The summary is the last lines of stderr, a login masked out of
        // them; the cause was read off the same lines as the tool wrote
        // them (`CommandOutput::failure_cause`), as the mask can take the
        // words that say it (re-check 2's N1).
        Some(code) => Ok(Outcome::Failed {
            exit_code: Some(code),
            summary: crate::runner::failure_summary(&output.stderr),
            cause: output.failure_cause(),
        }),
        // Neither flag is set, so the runner stopped nothing: a signal the
        // run did not send ended the command before it could exit (the doc
        // comment above). No exit code, no verdict.
        None => Ok(Outcome::Unconfirmed),
    }
}

/// What the read an adapter takes right before a confirmed command, to
/// see that what the preview was worked out from still holds (npm's
/// `npm prefix -g`, uv's `uv tool list --show-paths`), leaves the
/// operation with (r20 R20-2).
///
/// Only a read that answers can say something changed since the
/// preview: one that exited 0 is `Answered`, for the adapter to compare,
/// and an answer too long to read (`OutputTooLarge`) is "changed since
/// shown" as an answer it cannot read would be. A read that did not
/// answer says nothing about the plan -- the same program would not have
/// run the command either -- so the operation ends as that program's own
/// failure would, and nothing is written:
/// - the program is not there, or macOS would not start it: the runner's
///   own error, returned as `run_plan` returns it (`Fault::ProgramMissing`,
///   `Fault::SpawnFailed`);
/// - it exited non-zero: `Failed` with the read's exit code, its last
///   five lines of stderr and their cause, as `run_plan` reports a command
///   that exits non-zero -- npm's `env: node: No such file or directory`
///   when `node` is gone, kept with its line (`FailureCause::NotFound`);
/// - it did not finish: `Failed` with no exit code, and, where the read's
///   deadline stopped it, `FailureCause::TimedOut`, the read having taken
///   too long; a signal it did not send leaves the cause to its stderr.
///   Not `Unconfirmed`, as a command stopped the same way is: that says
///   the command may have taken effect, and none was started.
///
/// A `Failed` read's stderr goes to the operation's log first, line by
/// line, as `run_plan` streams a command's: the summary is lines the log
/// has (the drawer says a log "is no longer available" only where a
/// failure's summary has words and the log none of its lines,
/// `missingLogSummary` in src/components/MissingFailureLog.tsx). Its
/// stdout, the answer it was asked for, does not: it is held as the
/// program wrote it, with no login masked out of it, and the log is for a
/// person. A read that answered is compared, not logged.
///
/// Each read is handed the operation's own token, and one a Cancel
/// stopped never reaches here: the adapter answers `Outcome::Cancelled`
/// first, nothing having started (`NpmAdapter::execute`,
/// `UvAdapter::execute`; r28 R28-1).
pub(crate) fn read_before_run(
    read: Result<crate::runner::CommandOutput, AdapterError>,
    sink: &dyn EventSink,
    op_id: OpId,
) -> Result<ReadBeforeRun, AdapterError> {
    let output = match read {
        Ok(output) => output,
        Err(AdapterError::Runner(crate::runner::RunnerError::OutputTooLarge { .. })) => {
            return Ok(ReadBeforeRun::Ends(Outcome::BanagerFailed(
                crate::model::Fault::ChangedSinceShown,
            )))
        }
        Err(error) => return Err(error),
    };
    let finished = !output.timed_out && !output.cancelled;
    if finished && output.exit_code == Some(0) {
        return Ok(ReadBeforeRun::Answered(output));
    }
    // Masked on every path (`CommandOutput::stderr`), as a command's
    // lines are on their way to the log.
    for line in output.stderr.lines() {
        sink.emit(crate::events::OperationEvent::Log {
            op_id,
            stream: crate::events::Stream::Stderr,
            line: line.to_string(),
        });
    }
    let cause = if output.timed_out && !output.cancelled {
        Some(crate::history::FailureCause::TimedOut)
    } else {
        output.failure_cause()
    };
    Ok(ReadBeforeRun::Ends(Outcome::Failed {
        exit_code: output.exit_code.filter(|_| finished),
        summary: crate::runner::failure_summary(&output.stderr),
        cause,
    }))
}

/// See [`read_before_run`].
pub(crate) enum ReadBeforeRun {
    /// The read exited 0: its answer, for the adapter to compare with
    /// the preview's.
    Answered(crate::runner::CommandOutput),
    /// The operation ends here, with nothing written.
    Ends(Outcome),
}

/// `"cargo 1.98.1 (…)"` -> `Some("1.98.1")`. Several tools (cargo, uv, pip)
/// print their version as the second whitespace-separated token of the first
/// line; this is that rule, once. Tools that print it differently — pipx's
/// bare `1.17.3`, Ollama's `ollama version is 0.34.1` — keep their own
/// parser. A token with a control character in it is no version
/// (`sanity::version_token`).
pub fn second_token(text: &str) -> Option<String> {
    let mut parts = text.lines().next()?.split_whitespace();
    let _label = parts.next()?;
    sanity::version_token(parts.next().map(|token| token.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The read before a confirmed command (r20 R20-2): only an exit 0
    /// is an answer to compare; anything else ends the operation as the
    /// program's own failure would, its stderr in the operation's log,
    /// nothing written.
    #[test]
    fn test_a_read_before_the_command_that_did_not_answer_ends_as_its_programs_failure_would() {
        use crate::events::{OperationEvent, Stream, VecSink};
        use crate::history::FailureCause;
        use crate::runner::{CommandOutput, RunnerError};
        let output = |exit_code: Option<i32>, stderr: &str| CommandOutput {
            stderr_cause: Default::default(),
            exit_code,
            stdout: "/opt/homebrew\n".to_string(),
            stderr: stderr.to_string(),
            timed_out: false,
            cancelled: false,
        };
        // The outcome it ends in, if any, and the lines it logged.
        let ends = |read| {
            let sink = VecSink::new();
            let outcome = match read_before_run(read, &sink, 7) {
                Ok(ReadBeforeRun::Ends(outcome)) => Some(outcome),
                Ok(ReadBeforeRun::Answered(_)) => None,
                Err(error) => panic!("{error}"),
            };
            (outcome, sink.snapshot())
        };
        let stderr = |line: &str| OperationEvent::Log {
            op_id: 7,
            stream: Stream::Stderr,
            line: line.to_string(),
        };
        // An answer is compared, not logged: neither its stdout nor the
        // warnings npm writes beside it.
        assert_eq!(
            ends(Ok(output(Some(0), "npm warn config\n"))),
            (None, vec![])
        );
        assert_eq!(
            ends(Ok(output(
                Some(127),
                "env: node: No such file or directory\n"
            ))),
            (
                Some(Outcome::Failed {
                    exit_code: Some(127),
                    summary: "env: node: No such file or directory".to_string(),
                    cause: Some(FailureCause::NotFound),
                }),
                vec![stderr("env: node: No such file or directory")]
            )
        );
        // Every line of stderr to the log, the last five in the summary.
        let long = "one\ntwo\nthree\nfour\nfive\nsix\n";
        let (outcome, logged) = ends(Ok(output(Some(1), long)));
        assert_eq!(
            outcome,
            Some(Outcome::Failed {
                exit_code: Some(1),
                summary: "two\nthree\nfour\nfive\nsix".to_string(),
                cause: None,
            })
        );
        assert_eq!(
            logged,
            ["one", "two", "three", "four", "five", "six"].map(stderr)
        );
        // Stopped at its deadline: it took too long, no exit code to
        // report, even one the runner saw during the stop.
        assert_eq!(
            ends(Ok(CommandOutput {
                timed_out: true,
                ..output(Some(143), "")
            })),
            (
                Some(Outcome::Failed {
                    exit_code: None,
                    summary: String::new(),
                    cause: Some(FailureCause::TimedOut),
                }),
                vec![]
            )
        );
        // Stopped by a signal it did not send, or by the runner's own
        // cancel: no exit code, and no cause but what it wrote.
        for stopped in [
            CommandOutput {
                cancelled: true,
                ..output(Some(0), "")
            },
            output(None, ""),
        ] {
            assert_eq!(
                ends(Ok(stopped)),
                (
                    Some(Outcome::Failed {
                        exit_code: None,
                        summary: String::new(),
                        cause: None,
                    }),
                    vec![]
                )
            );
        }
        // An answer too long to read is one that cannot be compared.
        assert_eq!(
            ends(Err(AdapterError::Runner(RunnerError::OutputTooLarge {
                limit: 1
            }))),
            (
                Some(Outcome::BanagerFailed(
                    crate::model::Fault::ChangedSinceShown
                )),
                vec![]
            )
        );
        // The program gone is the runner's error, as `run_plan` returns it.
        assert!(matches!(
            read_before_run(
                Err(AdapterError::Runner(RunnerError::NotFound(
                    "/opt/homebrew/bin/npm".into()
                ))),
                &VecSink::new(),
                7
            ),
            Err(AdapterError::Runner(RunnerError::NotFound(_)))
        ));
    }

    #[tokio::test]
    async fn test_registry_deadline_keeps_completed_answers_and_cancels_the_rest() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let started = AtomicUsize::new(0);
        let dropped = AtomicUsize::new(0);
        struct Guard<'a>(&'a AtomicUsize);
        impl Drop for Guard<'_> {
            fn drop(&mut self) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }
        let answers = registry_checks_until(
            (0..12)
                .map(|i| {
                    let started = &started;
                    let dropped = &dropped;
                    async move {
                        let _guard = Guard(dropped);
                        started.fetch_add(1, Ordering::SeqCst);
                        if i == 1 {
                            std::future::pending::<()>().await;
                        }
                        Ok(i)
                    }
                })
                .collect(),
            // Long enough that a loaded machine cannot pass it before the
            // first four have even been polled; item 1 never answers.
            tokio::time::Instant::now() + Duration::from_millis(250),
        )
        .await;
        assert_eq!(answers.len(), 12);
        assert_eq!(started.load(Ordering::SeqCst), 4);
        assert_eq!(dropped.load(Ordering::SeqCst), 4);
        assert_eq!(*answers[0].as_ref().unwrap(), 0);
        assert_eq!(*answers[2].as_ref().unwrap(), 2);
        assert_eq!(*answers[3].as_ref().unwrap(), 3);
        for i in [1, 4, 5, 6, 7, 8, 9, 10, 11] {
            assert!(answers[i].as_ref().unwrap_err().transient);
        }
    }

    #[test]
    fn test_uncheckable_candidate_marks_only_a_transient_failure_so() {
        let npm_key = |name: &str| ArtifactKey {
            instance_id: "npm:/usr/local".to_string(),
            kind: crate::model::ArtifactKind::Package,
            name: name.to_string(),
        };
        let transient = uncheckable_candidate(
            npm_key("a"),
            "1.0.0".to_string(),
            UpdateChannel::Native,
            LookupFailure::words(
                "npm outdated -g: npm error code ENOTFOUND".to_string(),
                "npm error code ENOTFOUND\nnpm error syscall getaddrinfo",
            ),
        );
        assert_eq!(
            transient.warnings,
            vec![
                Warning::Message("npm outdated -g: npm error code ENOTFOUND".to_string()),
                Warning::TransientLookupFailure
            ]
        );
        assert!(!transient.checkable);
        // Words alone are a failure not known to mend itself.
        let lasting = uncheckable_candidate(
            npm_key("b"),
            "1.0.0".to_string(),
            UpdateChannel::Native,
            "cannot read the installed version now".to_string(),
        );
        assert_eq!(
            lasting.warnings,
            vec![Warning::Message(
                "cannot read the installed version now".to_string()
            )]
        );
        let shared = LookupFailure::words("x".to_string(), "connection refused");
        let rows = uncheckable_from_inventory(&[], UpdateChannel::Native, &shared);
        assert!(rows.is_empty());
    }

    #[test]
    fn test_uncheckable_candidate_marks_a_tool_never_looked_up_here_and_nothing_else_so() {
        // Independent review r6, F5: the Overview's all good is kept away
        // by a lookup that did not succeed, not by one Banager never makes
        // on this Mac -- which only this mark tells apart on the wire.
        let key = ArtifactKey {
            instance_id: "ollama:http://server:11434".to_string(),
            kind: crate::model::ArtifactKind::Model,
            name: "qwen3:8b".to_string(),
        };
        let never = uncheckable_candidate(
            key.clone(),
            "abc".to_string(),
            UpdateChannel::Digest,
            LookupFailure::not_looked_up(
                "remote daemon manifests cannot be checked from this Mac".to_string(),
            ),
        );
        assert!(!never.checkable);
        assert_eq!(
            never.warnings,
            vec![
                Warning::Message(
                    "remote daemon manifests cannot be checked from this Mac".to_string()
                ),
                Warning::NotLookedUpHere,
            ]
        );
        // Not transient, and no constructor of a failed lookup marks it.
        assert!(!LookupFailure::not_looked_up(String::new()).transient);
        use crate::http::HttpError;
        for failure in [
            LookupFailure::from("could not parse registry manifest".to_string()),
            LookupFailure::request("x", &HttpError::Network("dns error".to_string())),
            LookupFailure::request(
                "x",
                &HttpError::Tls {
                    host: "crates.io".to_string(),
                    detail: "UnknownIssuer".to_string(),
                },
            ),
            LookupFailure::request("x", &HttpError::Refused("redirect".to_string())),
            LookupFailure::status(String::new(), 404),
            LookupFailure::words(String::new(), "connection refused"),
        ] {
            let row = uncheckable_candidate(
                key.clone(),
                "abc".to_string(),
                UpdateChannel::Digest,
                failure,
            );
            assert!(
                !row.warnings.contains(&Warning::NotLookedUpHere),
                "{:?}",
                row.warnings
            );
        }
    }

    #[test]
    fn test_lookup_failure_is_transient_only_for_no_answer_or_a_not_now_status() {
        use crate::http::HttpError;
        let network = LookupFailure::request(
            "crates.io request failed",
            &HttpError::Network("dns error".to_string()),
        );
        assert_eq!(
            network.reason,
            "crates.io request failed: network error: dns error"
        );
        assert!(network.transient);
        assert!(
            LookupFailure::request("x", &HttpError::Timeout(Duration::from_secs(30))).transient
        );
        assert!(!LookupFailure::request("x", &HttpError::BodyTooLarge { limit: 1 }).transient);
        assert!(!LookupFailure::request("x", &HttpError::NoMock("u".to_string())).transient);
        for status in [408, 429, 500, 502, 503, 599] {
            assert!(
                LookupFailure::status(String::new(), status).transient,
                "{status}"
            );
        }
        // 404: a model made with `ollama create`, a crate removed from
        // crates.io -- the next check gets the same answer.
        for status in [301, 400, 401, 403, 404, 410, 418] {
            assert!(
                !LookupFailure::status(String::new(), status).transient,
                "{status}"
            );
        }
        assert_eq!(
            LookupFailure::from("could not parse registry manifest".to_string()),
            LookupFailure {
                reason: "could not parse registry manifest".to_string(),
                transient: false,
                secure_connection: None,
                not_looked_up: false,
            }
        );
    }

    #[test]
    fn test_lookup_failure_is_lasting_for_a_secure_connection_or_a_refusal() {
        // Round-5 review finding 6: both used to be `HttpError::Network`,
        // and so "check again" on every check, for good.
        use crate::http::HttpError;
        let tls = LookupFailure::request(
            "crates.io request failed",
            &HttpError::Tls {
                host: "crates.io".to_string(),
                detail: "invalid peer certificate: UnknownIssuer".to_string(),
            },
        );
        assert_eq!(
            tls,
            LookupFailure {
                reason: "crates.io request failed: secure connection to crates.io failed: invalid peer certificate: UnknownIssuer".to_string(),
                transient: false,
                secure_connection: Some("crates.io".to_string()),
                not_looked_up: false,
            }
        );
        for refused in [
            "refusing to follow a redirect: https://pypi.org/pypi/Django/json answered 301 Moved Permanently pointing at /pypi/django/json",
            "host not allowed: \"example.com\" is not one of [\"crates.io\"] (from https://example.com/)",
        ] {
            let failure =
                LookupFailure::request("PyPI request failed", &HttpError::Refused(refused.to_string()));
            assert!(!failure.transient, "{refused}");
            assert_eq!(failure.secure_connection, None, "{refused}");
            assert_eq!(failure.reason, format!("PyPI request failed: refused: {refused}"));
        }
        // Only the network and the time are transient, and neither names
        // a secure connection.
        for error in [
            HttpError::Network("dns error".to_string()),
            HttpError::Timeout(Duration::from_secs(30)),
        ] {
            let failure = LookupFailure::request("x", &error);
            assert!(failure.transient, "{error:?}");
            assert_eq!(failure.secure_connection, None, "{error:?}");
        }
    }

    #[test]
    fn test_uncheckable_candidate_names_the_host_of_a_failed_secure_connection_and_never_says_try_again(
    ) {
        use crate::http::HttpError;
        let row = uncheckable_candidate(
            ArtifactKey {
                instance_id: "cargo:/Users/a/.cargo".to_string(),
                kind: crate::model::ArtifactKind::Package,
                name: "hexyl".to_string(),
            },
            "0.16.0".to_string(),
            UpdateChannel::Native,
            LookupFailure::request(
                "crates.io request failed",
                &HttpError::Tls {
                    host: "crates.io".to_string(),
                    detail: "invalid peer certificate: UnknownIssuer".to_string(),
                },
            ),
        );
        assert!(!row.checkable);
        assert_eq!(
            row.warnings,
            vec![
                Warning::Message(
                    "crates.io request failed: secure connection to crates.io failed: invalid peer certificate: UnknownIssuer".to_string()
                ),
                Warning::SecureConnectionFailed {
                    host: "crates.io".to_string()
                },
            ]
        );
        // A refused redirect: only the words, no mark.
        let refused = uncheckable_candidate(
            ArtifactKey {
                instance_id: "pipx:/Users/a/.local/pipx".to_string(),
                kind: crate::model::ArtifactKind::Package,
                name: "Django".to_string(),
            },
            "5.0".to_string(),
            UpdateChannel::Native,
            LookupFailure::request(
                "PyPI request failed",
                &HttpError::Refused("refusing to follow a redirect".to_string()),
            ),
        );
        assert_eq!(
            refused.warnings,
            vec![Warning::Message(
                "PyPI request failed: refused: refusing to follow a redirect".to_string()
            )]
        );
    }

    #[test]
    fn test_says_network_failed_reads_the_network_in_each_tools_words_and_nothing_else() {
        // As the tools put it offline (the preview's `OFFLINE_REASONS` are
        // these): npm, pip (urllib3), pipx, uv, cargo, grok's own check.
        for words in [
            "npm error code ENOTFOUND",
            "npm ERR! code EAI_AGAIN",
            "npm error errno ECONNRESET",
            "WARNING: Retrying (Retry(total=4)) after connection broken by 'NewConnectionError('<HTTPSConnection>: Failed to establish a new connection: [Errno 8] nodename nor servname provided, or not known')'",
            "ReadTimeoutError: HTTPSConnectionPool(host='pypi.org', port=443): Read timed out. (read timeout=15)",
            "error: could not reach https://pypi.org/simple/ (network is unreachable)",
            "error: Request failed after 3 retries\n  Caused by: error sending request for url (https://pypi.org/simple/ruff/)\n  Caused by: dns error: failed to lookup address information",
            "the update check reported: could not reach the update server: error sending request: dns error",
            "curl: (6) Could not resolve host: formulae.brew.sh",
            "Connection refused (os error 61)",
            "No route to host",
        ] {
            assert!(says_network_failed(words), "{words}");
        }
        for words in [
            "",
            "registry returned status 404",
            "npm error code E401",
            "npm error code E404 Not Found - GET https://registry.npmjs.org/left-pad",
            "could not read local manifest /Users/me/.ollama/x: Permission denied (os error 13)",
            "SSL: CERTIFICATE_VERIFY_FAILED",
            "Antigravity CLI's update manifest is not yet verified on Intel Macs (x86_64)",
            "cannot read the installed version now",
            "npm ERR! 404 'timeout@9.9.9' is not in this registry",
            "ENOTFOUNDISH",
        ] {
            assert!(!says_network_failed(words), "{words}");
        }
    }

    #[test]
    fn test_says_network_failed_reads_every_shared_case_and_each_difference_from_the_window_is_listed(
    ) {
        // `network_words_cases.json` is read here and by
        // src/lib/failureCause.test.ts: `transient` is this function's
        // answer (a later check can get past it), `network` the cause the
        // window shows for a lookup's words (`lookupFailureCause`, the
        // same rule as `history::failure_cause`). Where the two differ,
        // the case says why, so a difference is never by accident.
        #[derive(serde::Deserialize)]
        struct Case {
            name: String,
            text: String,
            transient: bool,
            network: bool,
            why: Option<String>,
        }
        let cases: Vec<Case> =
            serde_json::from_str(include_str!("network_words_cases.json")).expect("cases parse");
        assert!(cases.len() >= 30, "the shared cases are all there");
        for case in cases {
            assert_eq!(
                says_network_failed(&case.text),
                case.transient,
                "{}",
                case.name
            );
            assert_eq!(
                crate::history::failure_cause(&case.text)
                    == Some(crate::history::FailureCause::Network),
                case.network,
                "{}",
                case.name
            );
            assert_eq!(
                case.why.is_some(),
                case.transient != case.network,
                "{}: only a difference is listed, and every one is",
                case.name
            );
        }
    }

    #[test]
    fn regression_second_token_refuses_a_version_with_a_control_character() {
        // Found by `adapters/robustness.rs` (a flipped byte in cargo's line).
        assert_eq!(second_token("cargo 1.9\u{0}.1 (797e8a9bc)\n"), None);
        assert_eq!(second_token("uv 0.12.1\u{17} (Homebrew)\n"), None);
        assert_eq!(
            second_token("cargo 1.98.1 (797e8a9bc 2026-08-05)\n"),
            Some("1.98.1".to_string())
        );
    }

    #[test]
    fn test_from_toml_parses_the_committed_brew_meta_file() {
        // cargo runs tests with cwd = the package manifest directory
        // (crates/banager-core), so this reaches the repo-root file.
        let s = std::fs::read_to_string("../../adapters/meta/brew.toml")
            .expect("read adapters/meta/brew.toml");
        let meta = AdapterMeta::from_toml(&s).expect("parse brew.toml");
        assert_eq!(meta.id, "brew");
        assert_eq!(meta.name, "Homebrew");
        assert_eq!(meta.platforms, vec!["macos".to_string()]);
    }

    #[test]
    fn test_unverified_version_flags_only_a_version_outside_a_non_empty_verified_list() {
        let meta = AdapterMeta {
            id: "fake".to_string(),
            name: "fake".to_string(),
            kind: "fake".to_string(),
            platforms: vec!["macos".to_string()],
            homepage: "https://example.invalid".to_string(),
            schema_version: 1,
            verified_versions: vec!["1.0".to_string()],
        };
        assert_eq!(meta.unverified_version(&Some("1.0".to_string())), None);
        assert_eq!(
            meta.unverified_version(&Some("9.9".to_string())),
            Some("9.9".to_string())
        );
        assert_eq!(meta.unverified_version(&None), None);

        // An adapter whose meta file lists no verified versions has nothing
        // to compare against, so it never flags anything.
        let unpinned = AdapterMeta {
            verified_versions: vec![],
            ..meta
        };
        assert_eq!(unpinned.unverified_version(&Some("9.9".to_string())), None);
    }

    #[test]
    fn test_url_path_segment_encodes_everything_that_could_leave_the_segment() {
        // Ordinary names must survive untouched, or every registry lookup
        // in the app changes shape.
        assert_eq!(url_path_segment("hexyl").expect("plain name"), "hexyl");
        assert_eq!(
            url_path_segment("qwen3.8_v2-beta+1~x").expect("punctuated name"),
            "qwen3.8_v2-beta+1~x"
        );

        // The characters that would otherwise change which resource the URL
        // addresses rather than merely naming it.
        assert_eq!(url_path_segment("a/b").expect("slash"), "a%2Fb");
        assert_eq!(
            url_path_segment("latest?x=1").expect("query"),
            "latest%3Fx=1"
        );
        assert_eq!(
            url_path_segment("tag#frag").expect("fragment"),
            "tag%23frag"
        );
        assert_eq!(url_path_segment("a%2Fb").expect("percent"), "a%252Fb");
        assert_eq!(url_path_segment("a b").expect("space"), "a%20b");
        assert_eq!(url_path_segment("a\r\nb").expect("crlf"), "a%0D%0Ab");
        assert_eq!(url_path_segment("..").expect("dotdot"), "..");
    }

    #[test]
    fn test_url_path_segment_refuses_an_empty_value() {
        // An empty segment collapses the path and addresses a *different*
        // endpoint (`https://crates.io/api/v1/crates/` is the crate index,
        // not one crate), so it is refused rather than encoded.
        assert!(url_path_segment("").is_err());
    }

    #[test]
    fn test_validate_package_name_accepts_normal_names() {
        assert!(validate_package_name("jq").is_ok());
        assert!(validate_package_name("node@20").is_ok());
        assert!(validate_package_name("some.tool_v2+beta").is_ok());
    }

    #[test]
    fn test_validate_package_name_rejects_shell_metacharacters() {
        assert!(validate_package_name("-rf").is_err());
        assert!(validate_package_name("a;b").is_err());
        assert!(validate_package_name("").is_err());
    }

    #[test]
    fn test_validate_package_name_rejects_an_absolute_path() {
        assert!(validate_package_name("/tmp/evil.rb").is_err());
    }

    #[test]
    fn test_validate_package_name_rejects_a_leading_dot() {
        assert!(validate_package_name(".hidden").is_err());
    }

    #[test]
    fn test_validate_package_name_rejects_a_dotdot_segment() {
        assert!(validate_package_name("foo/../evil").is_err());
        assert!(validate_package_name("../evil").is_err());
    }

    #[test]
    fn test_validate_package_name_rejects_an_rb_suffix() {
        assert!(validate_package_name("evil.rb").is_err());
        assert!(validate_package_name("some/tap/evil.rb").is_err());
    }

    #[test]
    fn test_validate_package_name_still_accepts_a_tap_qualified_cask_name() {
        assert!(validate_package_name("gautham-v/tap/claudebar").is_ok());
    }

    #[test]
    fn test_second_token_reads_the_version_out_of_a_labelled_version_line() {
        assert_eq!(
            second_token("cargo 1.98.1 (797e8a9bc 2026-08-05)\n"),
            Some("1.98.1".to_string())
        );
        assert_eq!(
            second_token("uv 0.12.17 (Homebrew)"),
            Some("0.12.17".to_string())
        );
        assert_eq!(second_token(""), None);
        assert_eq!(second_token("onlyoneword\n"), None);
    }

    /// An install plan that runs `/bin/fake` with `args`, for the
    /// `run_plan` tests: what `run_plan` returns depends only on the
    /// `CommandOutput` the runner answers with.
    fn plan_for(args: Vec<&str>) -> Plan {
        use crate::model::{CancelPolicy, OpKind, OpRequest, ResourceLock};
        use std::path::PathBuf;

        Plan {
            request: OpRequest {
                kind: OpKind::Install,
                instance_id: "fake:1".to_string(),
                artifact_kind: ArtifactKind::Package,
                name: "jq".to_string(),
            },
            action: PlanAction::Command {
                program: PathBuf::from("/bin/fake"),
                args: args.into_iter().map(|a| a.to_string()).collect(),
                env: Vec::new(),
            },
            needs_password: false,
            locks: vec![ResourceLock("fake:1".to_string())],
            cancel_policy: CancelPolicy::KillThenReconcile,
            warnings: Vec::new(),
            affected: Vec::new(),
            basis: None,
            timeout_secs: 60,
        }
    }

    #[tokio::test]
    async fn test_run_plan_maps_a_cancelled_run_to_unconfirmed_and_a_failure_to_the_last_stderr_lines(
    ) {
        use crate::events::VecSink;
        use crate::runner::{CommandOutput, MockRunner};
        use tokio_util::sync::CancellationToken;

        let runner_raw = MockRunner::new();
        runner_raw.respond(
            vec!["/bin/fake", "cancelled"],
            CommandOutput {
                stderr_cause: Default::default(),
                exit_code: None,
                stdout: String::new(),
                stderr: String::new(),
                timed_out: false,
                cancelled: true,
            },
        );
        runner_raw.respond(
            vec!["/bin/fake", "failed"],
            CommandOutput {
                stderr_cause: Default::default(),
                exit_code: Some(2),
                stdout: String::new(),
                stderr: "l1\nl2\nl3\nl4\nl5\nl6\nl7".to_string(),
                timed_out: false,
                cancelled: false,
            },
        );
        let runner: Arc<dyn CommandRunner> = Arc::new(runner_raw);

        let sink = Arc::new(VecSink::new());
        assert_eq!(
            run_plan(
                &runner,
                &plan_for(vec!["cancelled"]),
                sink.clone(),
                1,
                CancellationToken::new()
            )
            .await
            .expect("run_plan"),
            Outcome::Unconfirmed
        );
        assert_eq!(
            run_plan(
                &runner,
                &plan_for(vec!["failed"]),
                sink,
                2,
                CancellationToken::new()
            )
            .await
            .expect("run_plan"),
            Outcome::Failed {
                exit_code: Some(2),
                summary: "l3\nl4\nl5\nl6\nl7".to_string(),
                cause: None,
            }
        );
    }

    #[tokio::test]
    async fn test_run_plan_answers_cancelled_and_starts_nothing_for_a_cancel_before_the_command() {
        // The user's Cancel landed after the operation turned Running but
        // before its command was started: nothing ran, so nothing changed,
        // and the outcome is the cancel itself -- also for an uninstall,
        // whose interrupted run is otherwise never `Cancelled`.
        use crate::events::VecSink;
        use crate::runner::MockRunner;
        use tokio_util::sync::CancellationToken;

        let runner_raw = Arc::new(MockRunner::new());
        let runner: Arc<dyn CommandRunner> = runner_raw.clone();
        let cancel = CancellationToken::new();
        cancel.cancel();
        let mut plan = plan_for(vec!["uninstall"]);
        plan.request.kind = crate::model::OpKind::Uninstall;
        assert_eq!(
            run_plan(&runner, &plan, Arc::new(VecSink::new()), 1, cancel)
                .await
                .expect("run_plan"),
            Outcome::Cancelled
        );
        assert!(runner_raw.calls().is_empty(), "nothing is started");
    }

    #[tokio::test]
    async fn test_run_plan_maps_a_run_ended_by_a_signal_it_did_not_send_to_unconfirmed() {
        // No exit code and neither flag set is how `RealRunner` reports a
        // child that a signal the run did not send ended -- Activity
        // Monitor, `kill`, a crash (runner/real.rs, the `child_code` arm of
        // `run`). Such a command did not run to its end and reported no
        // failure: whatever it wrote to stderr before it died is not a
        // verdict, so it is not `Failed`, which `run_operation` shows as
        // the tool's own and never reconciles. `Unconfirmed` is reconciled
        // like a Cancel or a timeout.
        use crate::events::VecSink;
        use crate::runner::{CommandOutput, MockRunner};
        use tokio_util::sync::CancellationToken;

        let runner_raw = MockRunner::new();
        runner_raw.respond(
            vec!["/bin/fake", "killed"],
            CommandOutput {
                stderr_cause: Default::default(),
                exit_code: None,
                stdout: "==> Pouring jq".to_string(),
                stderr: "Warning: partial\n".to_string(),
                timed_out: false,
                cancelled: false,
            },
        );
        let runner: Arc<dyn CommandRunner> = Arc::new(runner_raw);

        assert_eq!(
            run_plan(
                &runner,
                &plan_for(vec!["killed"]),
                Arc::new(VecSink::new()),
                3,
                CancellationToken::new()
            )
            .await
            .expect("run_plan"),
            Outcome::Unconfirmed
        );
    }

    #[tokio::test]
    async fn test_run_plan_sends_a_runner_note_to_the_log_as_a_note_not_as_text() {
        // The one door between a runner's callback and the log drawer. A
        // note that came out the other side as `Log { line }` would need
        // English text to carry it, which is the thing `LogNote` exists to
        // keep out of the log.
        use crate::events::{LogNote, OperationEvent, Stream, VecSink};
        use crate::model::{CancelPolicy, OpKind, OpRequest, ResourceLock};
        use crate::runner::{CommandOutput, RunnerError};
        use tokio_util::sync::CancellationToken;

        struct NotingRunner;
        #[async_trait::async_trait]
        impl CommandRunner for NotingRunner {
            async fn run(
                &self,
                _spec: CommandSpec,
                on_line: Option<LineCallback>,
                _cancel: CancellationToken,
            ) -> Result<CommandOutput, RunnerError> {
                let cb = on_line.expect("run_plan always streams");
                cb(RunLine::Output(
                    Stream::Stdout,
                    "==> Pouring jq".to_string(),
                ));
                cb(RunLine::Note(LogNote::ReadFailed {
                    stream: Stream::Stdout,
                    error: "Input/output error (os error 5)".to_string(),
                }));
                Ok(CommandOutput {
                    stderr_cause: Default::default(),
                    exit_code: Some(0),
                    stdout: "==> Pouring jq".to_string(),
                    stderr: String::new(),
                    timed_out: false,
                    cancelled: false,
                })
            }
        }

        let plan = Plan {
            request: OpRequest {
                kind: OpKind::Install,
                instance_id: "fake:1".to_string(),
                artifact_kind: ArtifactKind::Package,
                name: "jq".to_string(),
            },
            action: PlanAction::Command {
                program: std::path::PathBuf::from("/bin/fake"),
                args: Vec::new(),
                env: Vec::new(),
            },
            needs_password: false,
            locks: vec![ResourceLock("fake:1".to_string())],
            cancel_policy: CancelPolicy::KillThenReconcile,
            warnings: Vec::new(),
            affected: Vec::new(),
            basis: None,
            timeout_secs: 60,
        };
        let runner: Arc<dyn CommandRunner> = Arc::new(NotingRunner);
        let sink = Arc::new(VecSink::new());
        run_plan(&runner, &plan, sink.clone(), 9, CancellationToken::new())
            .await
            .expect("run_plan");

        assert_eq!(
            sink.snapshot(),
            vec![
                OperationEvent::Log {
                    op_id: 9,
                    stream: Stream::Stdout,
                    line: "==> Pouring jq".to_string(),
                },
                OperationEvent::Note {
                    op_id: 9,
                    note: LogNote::ReadFailed {
                        stream: Stream::Stdout,
                        error: "Input/output error (os error 5)".to_string(),
                    },
                },
            ]
        );
    }

    #[tokio::test]
    async fn test_run_plan_refuses_a_plan_that_runs_no_command() {
        // A `TrashPaths` plan is carried out by `StandaloneAdapter::execute`
        // itself (adapters/standalone/removal.rs), never by a runner. Handing
        // one to `run_plan` is a bug in an adapter, refused before anything
        // could be spawned.
        use crate::events::VecSink;
        use crate::model::{CancelPolicy, OpKind, OpRequest, PlanAction, ResourceLock};
        use crate::runner::MockRunner;
        use std::path::PathBuf;
        use tokio_util::sync::CancellationToken;

        let plan = Plan {
            request: OpRequest {
                kind: OpKind::Uninstall,
                instance_id: "standalone-claude".to_string(),
                artifact_kind: ArtifactKind::Binary,
                name: "claude".to_string(),
            },
            action: PlanAction::TrashPaths {
                paths: vec![PathBuf::from("/Users/someone/.local/bin/claude")],
                previewed: Vec::new(),
            },
            needs_password: false,
            locks: vec![ResourceLock("standalone-claude".to_string())],
            cancel_policy: CancelPolicy::KillThenReconcile,
            warnings: Vec::new(),
            affected: Vec::new(),
            basis: None,
            timeout_secs: 120,
        };
        let runner_raw = Arc::new(MockRunner::new());
        let runner: Arc<dyn CommandRunner> = runner_raw.clone();
        let result = run_plan(
            &runner,
            &plan,
            Arc::new(VecSink::new()),
            3,
            CancellationToken::new(),
        )
        .await;
        assert!(
            matches!(result, Err(AdapterError::Refused(_))),
            "{result:?}"
        );
        assert!(runner_raw.calls().is_empty(), "nothing was spawned");
    }

    use crate::model::ArtifactKind;

    fn installed(kind: ArtifactKind, name: &str, version: &str) -> InstalledArtifact {
        InstalledArtifact {
            version: version.to_string(),
            ..crate::testing::installed_artifact("inst", kind, name)
        }
    }

    fn key(kind: ArtifactKind, name: &str) -> ArtifactKey {
        ArtifactKey {
            instance_id: "inst".to_string(),
            kind,
            name: name.to_string(),
        }
    }

    #[test]
    fn test_reconcile_from_reports_the_installed_version_of_a_matching_key() {
        let artifacts = vec![
            installed(ArtifactKind::Package, "lodash", "4.17.21"),
            installed(ArtifactKind::Package, "typescript", "5.9.2"),
        ];
        assert_eq!(
            reconcile_from(artifacts, &key(ArtifactKind::Package, "typescript")),
            Reconciled {
                present: true,
                version: Some("5.9.2".to_string()),
            }
        );
    }

    #[test]
    fn test_reconcile_from_reports_absent_for_a_name_that_is_not_installed() {
        let artifacts = vec![installed(ArtifactKind::Package, "lodash", "4.17.21")];
        assert_eq!(
            reconcile_from(artifacts, &key(ArtifactKind::Package, "does-not-exist")),
            Reconciled {
                present: false,
                version: None,
            }
        );
    }

    #[test]
    fn test_reconcile_from_does_not_match_the_same_name_of_another_kind() {
        // The rule npm's hand-written copy had already lost.
        let artifacts = vec![installed(ArtifactKind::Formula, "python", "3.14.2")];
        assert_eq!(
            reconcile_from(artifacts, &key(ArtifactKind::Cask, "python")),
            Reconciled {
                present: false,
                version: None,
            }
        );
    }

    #[test]
    fn test_ensure_instance_match_accepts_a_request_for_the_instance_it_was_given() {
        use crate::model::{OpKind, OpRequest};
        let inst = crate::testing::manager_instance("npm", "npm:/opt/homebrew");
        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Package,
            name: "jq".to_string(),
        };
        assert!(ensure_instance_match(&req, &inst).is_ok());
    }

    #[test]
    fn test_ensure_instance_match_refuses_a_request_meant_for_another_instance() {
        use crate::model::{OpKind, OpRequest};
        let inst = crate::testing::manager_instance("npm", "npm:/opt/homebrew");
        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: "npm:/usr/local".to_string(),
            artifact_kind: ArtifactKind::Package,
            name: "jq".to_string(),
        };
        match ensure_instance_match(&req, &inst) {
            Err(AdapterError::Refused(message)) => {
                assert!(
                    message.contains("npm:/usr/local") && message.contains("npm:/opt/homebrew"),
                    "the refusal must name both instances, got {message}"
                );
            }
            other => panic!("expected Refused, got {other:?}"),
        }
    }
}
