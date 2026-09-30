# Phase 4 Step A: Trust File and Guards Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the three promises phase 4 builds on true *before* any standalone-installer code lands, and make the one trust document honest about the seven sources Banager already manages. Concretely: (1) a test that Homebrew plans never carry `--zap`, `--force` or `--ignore-dependencies`; (2) a compile-time https host allowlist that `RealHttpClient::send` enforces fail-closed, with the stale "four endpoints" comment replaced; (3) `warningKey` in `src/lib/warnings.ts` made exhaustive with a `never` default, so a `Warning` variant added without copy fails `tsc` instead of vanishing from the uninstall dialog; (4) `docs/what-we-run.md` rewritten from its "Phase 0–1: Homebrew only" state to cover Homebrew, npm, pipx, uv, pip, Cargo and Ollama — every command, environment variable, file, host, and what is never done — with a test that pins the parts a test can pin.

**Architecture:** No new production concept. Task 1 is a test in an existing module. Task 2 adds one `pub const` and one `pub fn` to `crates/banager-core/src/http/real.rs` and calls the function at the top of `send`. Task 3 rewrites two functions in `src/lib/warnings.ts` in the shape `faultKey`/`faultArgs` in `src/lib/format.ts` already have. Task 4 is a document plus an integration test (`crates/banager-core/tests/what_we_run_test.rs`, the same shape as `fixtures_layout_test.rs`) that reads the document and checks it against the registered adapters, `ALLOWED_HTTPS_HOSTS`, `BrewAdapter::ENV`, `NpmAdapter::ENV` and the three flags. This is spec §十 row A: "无，可最先合" — nothing else in phase 4 depends on the order of these four, but steps B–F all depend on A having merged (B reads `ALLOWED_HTTPS_HOSTS`; C adds `Warning` variants that must fail `tsc` when unhandled; every step appends its own section to `what-we-run.md`).

**Tech Stack:** Rust (banager-core, `url` 2 already a dependency, tokio for tests), TypeScript 5 with `strict` (`tsconfig.json`), vitest.

**Baseline:** branch `feat/phase-4-standalone` at `26bc640` (= `8ba6f52` + the spec commit). Every line number below was read at that HEAD on 2026-09-24. The spec is `docs/superpowers/2026-09-24-phase-4-standalone-spec.md`; the sections this step implements are §十 row A, §4.2 (allowlist), §6.5 "必须一起改" (warningKey), §6.7 (`--zap`), §9.5 (what-we-run.md), and the `http/real.rs` / `brew/mod.rs` lines of §9.4.

## Global Constraints

Copied from the spec's binding rules (spec lines 20–23, verbatim), which bind every task here:

> 产品规则一条不让（spec §1、§6）：每一步说人话；后台工作绝不问密码；执行前先看到确切命令；
> 结果诚实——版本没动是 `NeedsAttention(UnchangedAfterUpgrade)`，中途停止是 `Unconfirmed`，
> 没有证据绝不说成功；fixture 只收真机录制；Banager 不跑 shell、不把下载管进 `sh`；
> 界面绝不提供 Rust 会拒绝的操作；所有文案 en + zh-CN。

And from spec §十 ("每一步只带**该步有生产者**的变体与字段") and §2.2/§2.3 ("每个新字段点名生产读取方"):

- **Every new field, variant, constant or function names its production reader in the same task.** In this step: `ALLOWED_HTTPS_HOSTS` is read by `host_allowed`, which is read by `RealHttpClient::send`; both are also read by `docs/what-we-run.md` (the "Banager only connects to these hosts" list) and by `tests/what_we_run_test.rs`. Nothing defined here waits for a later step to be used.
- **Honest outcomes.** Nothing in this step touches `run_operation`; the document describes the existing rules (`ops/mod.rs:677-720`, `:787-799`) and must not overstate them.
- **Fixtures come from real machines only.** This step records nothing. The tests here use inline data, a loopback socket, or the document itself, none of which is a fixture.
- **No shell.** The document must state truthfully that the one shell Banager ever runs is `fix-path-env`'s login-shell read at launch (`src-tauri/src/lib.rs:18`), and that no package command goes through one.
- **The UI never offers what Rust refuses.** Not exercised by this step; nothing here changes a gate.
- **en + zh-CN for all copy.** This step adds no user-visible copy and no i18n key. `docs/what-we-run.md` is an English engineering document, as it is today.
- **The five gates**, from README.md "Tests — all five must pass before anything is committed" — note the TypeScript gate is now `pnpm typecheck` (two `tsc` programs), not `pnpm exec tsc -p tsconfig.json`:

  ```bash
  cargo fmt --all --check
  cargo clippy --workspace --all-targets -- -D warnings
  cargo test --workspace
  pnpm test
  pnpm typecheck
  ```

- **Commits:** `git add <exact paths>` (never `-A`), a plain imperative subject line in the style of the recent history (`git log --format=%s -6`: "Record which backlog entries are closed, and by which commit"), a body when the subject needs one, then a blank line and `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## What already exists (do not rebuild)

- `crates/banager-core/src/http/real.rs`: `RealHttpClient` with rustls (`:40`), `banager/{version}` UA (`:38`), no redirects (`:50`, `:87-97`), 30 s default timeout (`:51`), 8 MiB body cap (`:22`, `:115-119`), a test module with loopback servers (`:130-441`). The comment at `:42-49` says "the four endpoints this client talks to (a local Ollama daemon, registry.ollama.ai, crates.io, PyPI)" — a sentence, not a check, and the "four" counts the http Ollama daemon; the https hosts are three. The test comment at `:294-301` repeats the same list.
- Three https callers, all `GET`: `CargoAdapter::latest_stable_version` (`adapters/cargo.rs:218-231`, `https://crates.io/api/v1/crates/{name}`), `PipxAdapter::latest_pypi_version` (`adapters/pipx.rs:279-299`, `https://pypi.org/pypi/{name}/json`), `OllamaAdapter::compare_digests` (`adapters/ollama/mod.rs:389-407`, `https://registry.ollama.ai/v2/{ns}/{name}/manifests/{tag}`). One http caller: the Ollama daemon (`adapters/ollama/mod.rs:135`, `:141-145`, `:273-283`, `:331-339`) at `HostEnv::ollama_host` or `http://127.0.0.1:11434`. `url = "2"` is already in `crates/banager-core/Cargo.toml:35` and used at `adapters/ollama/mod.rs:196` and `runner/path_env.rs:2`.
- `BrewAdapter::plan` (`adapters/brew/mod.rs:1189-1311`): Install argv `["install", flag, name]` (`:1211`), Uninstall `["uninstall", flag, name]` (`:1277`), Upgrade `["upgrade", flag, name]` (`:1300`). `grep -n -- '--zap\|--force' crates/banager-core/src/adapters/brew/mod.rs` is empty; `--ignore-dependencies` appears once, in a doc comment (`:567`). The test module `plan_execute_tests` (`:2239-`) has `test_instance()` (`:2245-2252`) and per-kind plan tests (`:2289-2543`); `MockRunner::respond` (`runner/mock.rs:32-35`) is keyed by the full argv including the program.
- `src/lib/warnings.ts`: `warningKey` (`:19-35`) with `default: return null` for bare strings and `return null` for unknown objects; `warningArgs` (`:38-47`); `warningText`/`warningTexts` (`:74-84`), the latter dropping `null`. `src/lib/format.ts:77-94` `faultKey` and `:98-105` `faultArgs` are the exhaustive model. `src/lib/types.ts:66-85` documents that a new variant "lands in `warningText`'s default branch rather than failing at compile time"; `src/lib/types.test.ts:197-224` is the wire-shape test; `src/lib/warnings.test.ts` has three tests built on `"SomeFutureVariant" as unknown as Warning` (`:27-32`, `:92-94`, `:97-105`), and `src/components/UninstallDialog.test.tsx:140-155` is a fourth, through the rendered dialog (`"SomeFutureVariant" as unknown as Plan["warnings"][number]`, asserting the "Before you continue:" heading is absent) — four in all; `grep -rn SomeFutureVariant src` finds no fifth. Consumers: `src/components/UninstallDialog.tsx:93` (`warningTexts`, comment at `:88-92`), `src/pages/UpdatesPage.tsx:412-416` and `:947`.
- `docs/what-we-run.md`: 78 lines, title "What Banager Runs (Phase 0–1: Homebrew only)", Homebrew only. Last touched by `adc8bdb`.
- `crates/banager-core/tests/fixtures_layout_test.rs:13-27`: the pattern for an integration test that builds `Session::new(Arc::new(VecSink::new()), None)`, calls `session.adapter_ids()`, and reads repo files relative to the crate directory (`../../adapters/...`).

---

## File Structure

```
crates/banager-core/src/adapters/brew/mod.rs   MOD  one test in `plan_execute_tests` (Task 1): the three-flag promise
crates/banager-core/src/http/real.rs           MOD  ALLOWED_HTTPS_HOSTS, host_allowed, the check in send, two comment fixes, six tests (Task 2)
src/lib/warnings.ts                            MOD  warningKey/warningArgs exhaustive with `never` defaults; comments (Task 3)
src/lib/warnings.test.ts                       MOD  drop the three "unrecognised variant" unit tests; assert every variant has a key (Task 3)
src/lib/types.ts                               MOD  the `Warning` doc comment no longer claims drift is silent (Task 3)
src/lib/types.test.ts                          MOD  the comment on the Warning shape test (Task 3)
src/components/UninstallDialog.tsx             MOD  one comment (Task 3)
src/components/UninstallDialog.test.tsx        MOD  the fourth "unrecognised variant" test now expects the raw key rendered, not dropped (Task 3)
docs/what-we-run.md                            REWRITE  seven sources, files, hosts, never-list (Task 4)
crates/banager-core/tests/what_we_run_test.rs  NEW  the document's checkable claims, checked (Task 4)
```

Single responsibility of each: `real.rs` owns *which hosts may be contacted*; `warnings.ts` owns *how a Warning becomes text*; `what-we-run.md` owns *what a person is told Banager does*; `what_we_run_test.rs` owns *that the document and the code agree on the parts a test can compare*.

## Task List

| # | Task | Deliverable |
|---|---|---|
| 1 | brew never passes `--zap`, `--force` or `--ignore-dependencies` | spec §6.7: "从「碰巧没写」变成「承诺」" |
| 2 | `ALLOWED_HTTPS_HOSTS` + fail-closed `send` + stale comment | spec §4.2 / Q8, current three hosts only |
| 3 | `warningKey` exhaustive with `never` | spec §6.5 "必须一起改"; the `Warning` row of §9.1's table moves from "今天抓不住" to "`tsc` 失败" |
| 4 | `docs/what-we-run.md` for seven sources + `what_we_run_test.rs` | spec §9.5 / D12: the trust file stops lying |

Order: 1 → 2 → 3 → 4. Task 4's test reads `ALLOWED_HTTPS_HOSTS` (Task 2) and its document cites the test from Task 1. Tasks 1–3 are independent of each other.

---

### Task 1: brew never passes `--zap`, `--force` or `--ignore-dependencies`

**Files:**
- Modify: `crates/banager-core/src/adapters/brew/mod.rs` — insert one test after `test_plan_upgrade_cask_passes_cask_flag` (which ends at `:2543`), inside `mod plan_execute_tests` (`:2239-`).
- Test: the same file.

**Interfaces:**
- Consumes: `BrewAdapter::new(runner)` (`brew/mod.rs:179`), `BrewAdapter::plan(&self, inst, req) -> Result<Plan, AdapterError>` (`:1189-1193`), `Plan.args: Vec<String>` (`model.rs:379`), `OpRequest { kind, instance_id, artifact_kind, name }` (`model.rs:344-349`), `OpKind::{Install, Uninstall, Upgrade}` (`model.rs:338-342`), `ArtifactKind::{Formula, Cask}`, `MockRunner::{new, respond}` (`runner/mock.rs:16, :32`), `CommandOutput` (`runner/mod.rs:57-73`), the module's `test_instance()` (`:2245-2252`). All already imported at the top of `brew/mod.rs` (`:8-13`) or in the module (`:2241-2243`).
- Produces: one test, `test_plan_never_passes_zap_force_or_ignore_dependencies`. It is named in `docs/what-we-run.md` (Task 4) as the thing that keeps the never-list's Homebrew line true.

- [ ] **Step 1: Write the test**

Insert into `crates/banager-core/src/adapters/brew/mod.rs` immediately after the closing `}` of `test_plan_upgrade_cask_passes_cask_flag` (line 2543), before the doc comment of `test_execute_refuses_as_root` (`:2545`):

```rust
    /// Homebrew's `--zap` removes everything a cask's zap stanza names --
    /// for `claude-code` that is the *native* install's `~/.local/bin/claude`
    /// and `~/.local/share/claude`, and the shared `~/.claude` -- and
    /// `--force` and `--ignore-dependencies` override refusals Homebrew makes
    /// on the user's behalf. None of the three has ever been passed here, but
    /// until now that was an absence, not a promise: `docs/what-we-run.md`
    /// says Banager never passes them, and this is what keeps that sentence
    /// true when `plan` is next edited. Every plan brew builds, for both
    /// artifact kinds, is exactly the verb, the kind flag and the name.
    #[tokio::test]
    async fn test_plan_never_passes_zap_force_or_ignore_dependencies() {
        let runner = Arc::new(MockRunner::new());
        // An Uninstall plan runs `brew uses --installed {name}` first; an
        // empty answer keeps the plan free of dependents, which is not what
        // this test is about.
        for name in ["jq", "docker"] {
            runner.respond(
                vec!["/opt/homebrew/bin/brew", "uses", "--installed", name],
                CommandOutput {
                    exit_code: Some(0),
                    stdout: String::new(),
                    stderr: String::new(),
                    timed_out: false,
                    cancelled: false,
                },
            );
        }
        let adapter = BrewAdapter::new(runner);
        let inst = test_instance();
        const FORBIDDEN: [&str; 3] = ["--zap", "--force", "--ignore-dependencies"];
        for (artifact_kind, flag, name) in [
            (ArtifactKind::Formula, "--formula", "jq"),
            (ArtifactKind::Cask, "--cask", "docker"),
        ] {
            for (kind, verb) in [
                (OpKind::Install, "install"),
                (OpKind::Uninstall, "uninstall"),
                (OpKind::Upgrade, "upgrade"),
            ] {
                let req = OpRequest {
                    kind,
                    instance_id: inst.id.clone(),
                    artifact_kind,
                    name: name.to_string(),
                };
                let plan = adapter
                    .plan(&inst, &req)
                    .await
                    .unwrap_or_else(|e| panic!("plan {verb} {flag} {name}: {e}"));
                for forbidden in FORBIDDEN {
                    assert!(
                        !plan.args.iter().any(|arg| arg.as_str() == forbidden),
                        "brew {verb} {flag} {name} must never carry {forbidden}, got {:?}",
                        plan.args
                    );
                }
                assert_eq!(
                    plan.args,
                    vec![verb, flag, name],
                    "brew {verb} {flag} {name} is exactly the verb, the kind flag and the name"
                );
            }
        }
    }
```

- [ ] **Step 2: Run it — it passes, because the property already holds**

Run: `cargo test -p banager-core --lib adapters::brew::plan_execute_tests::test_plan_never_passes_zap_force_or_ignore_dependencies`
Expected: PASS (1 passed). This is a regression guard for a property the code already has (`brew/mod.rs:1211`, `:1277`, `:1300`); there is nothing to implement. Step 3 proves the guard is live.

- [ ] **Step 3: Prove the assertion bites (mutation check), then revert**

Temporarily edit `crates/banager-core/src/adapters/brew/mod.rs` at the Upgrade arm: change line 1300 from

```rust
                    args: vec!["upgrade".to_string(), flag.to_string(), req.name.clone()],
```

to

```rust
                    args: vec!["upgrade".to_string(), "--force".to_string(), flag.to_string(), req.name.clone()],
```

Run: `cargo test -p banager-core --lib adapters::brew::plan_execute_tests::test_plan_never_passes_zap_force_or_ignore_dependencies`
Expected: FAIL — `brew upgrade --formula jq must never carry --force, got ["upgrade", "--force", "--formula", "jq"]`.

Revert the edit (`git checkout -- crates/banager-core/src/adapters/brew/mod.rs` would also discard the new test, so revert the one line by hand or with `git diff` as a guide). Run the test again.
Expected: PASS. `git diff --stat` shows only the added test (about 60 lines, no other hunk).

- [ ] **Step 4: Run the gates**

Run: `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && pnpm test && pnpm typecheck`
Expected: all clean. (`cargo fmt` may want the long `assert!` message on its own lines; run `cargo fmt --all` and re-check rather than hand-wrapping.)

- [ ] **Step 5: Commit**

```bash
git add crates/banager-core/src/adapters/brew/mod.rs
git commit -m "$(cat <<'EOF'
Promise in a test that brew plans never carry --zap, --force or --ignore-dependencies

The three flags have never been passed (the only mention in the file is
a doc comment), but nothing kept it that way. Homebrew's --zap for the
claude-code cask would remove the native install and the shared
~/.claude, which phase 4 lists beside the cask; the promise has to hold
before that row exists. Every install, uninstall and upgrade plan, for a
formula and for a cask, is asserted to be exactly the verb, the kind
flag and the name.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 2: `ALLOWED_HTTPS_HOSTS`, fail-closed in `RealHttpClient::send`

**Files:**
- Modify: `crates/banager-core/src/http/real.rs`
  - `:1-7` module doc: add the allowlist sentence.
  - after `:22` (`MAX_RESPONSE_BYTES`): add `ALLOWED_HTTPS_HOSTS` and `host_allowed`.
  - `:42-49` the redirect-policy comment: replace the "four endpoints" sentence.
  - `:69-71` top of `send`: call `host_allowed(&req.url)?` first.
  - `:294-301` the redirect test's comment: same list, same fix.
  - `mod tests` (`:130-`): six new tests.
- Test: the same file's `#[cfg(test)] mod tests`.

**Interfaces:**
- Consumes: `HttpError::Network(String)` (`http/mod.rs:33-34`), `HttpRequest { method, url, headers, timeout }` (`http/mod.rs:18-23`), `url::Url` (`Cargo.toml:35`).
- Produces (authoritative; step B's recipe test and Task 4's document test use these names verbatim):
  ```rust
  // crates/banager-core/src/http/real.rs
  pub const ALLOWED_HTTPS_HOSTS: &[&str] = &["crates.io", "pypi.org", "registry.ollama.ai"];
  pub fn host_allowed(url: &str) -> Result<(), HttpError>;
  ```
  Reachable as `banager_core::http::real::{ALLOWED_HTTPS_HOSTS, host_allowed}` (`http/mod.rs:11` is `pub mod real;`). No re-export is added to `http/mod.rs`: the two existing `pub use` lines re-export types only, and step B's test can name the module.
  Production readers: `RealHttpClient::send` (this task); `docs/what-we-run.md` and `tests/what_we_run_test.rs` (Task 4); step B's `RECIPES` test (spec §4.2, §9.4) later.

Why exactly these three hosts: they are the only https hosts the crate contacts today — `adapters/cargo.rs:225-228`, `adapters/pipx.rs:287`, `adapters/ollama/mod.rs:389-394`. `grep -rn 'https://' crates/banager-core/src --include='*.rs'` outside comments and fixture paths finds nothing else. The spec's list (§4.2) already includes three phase-4 hosts; per the step-A brief those are added by the steps that add their producers (B: `downloads.claude.ai`; D: the antigravity Cloud Run host; E: `static.rust-lang.org`), so that a host never sits on the list without a caller.

Why `http://` is exempt: the crate's one http caller is the Ollama daemon at `HostEnv::ollama_host` (`runner/path_env.rs:15-20`, normalised at `:40-57`), which may point at another machine; and every loopback test in `real.rs` uses `http://{addr}`. The known gap (`OLLAMA_HOST=https://…` to a remote machine is refused) is recorded in spec §十一 and §十二 Q8; it is not handled here.

- [ ] **Step 1: Write the failing tests**

Append inside `mod tests` in `crates/banager-core/src/http/real.rs`, after `test_real_http_client_refuses_a_body_one_byte_past_the_limit` (`:424-440`), before the module's closing `}` (`:441`):

```rust
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
                Err(HttpError::Network(message)) => assert!(
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
            Err(HttpError::Network(message)) => assert!(
                message.contains("scheme not allowed"),
                "got {message:?}"
            ),
            other => panic!("expected a scheme error, got {other:?}"),
        }
        match host_allowed("not a url") {
            Err(HttpError::Network(message)) => {
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
            Err(HttpError::Network(message)) => assert!(
                message.contains("host not allowed"),
                "expected the allowlist's refusal, got {message:?}"
            ),
            other => panic!("expected a host-not-allowed error, got {other:?}"),
        }
    }
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p banager-core --lib http::real::tests`
Expected: FAIL to compile — `error[E0425]: cannot find function `host_allowed` in this scope`, once per call site: ten of them, every `host_allowed(` in the five unit tests (3 + 3 + 1 + 1 + 2). The last test would also fail at runtime once it compiled: today `send` hands the URL to reqwest, whose DNS error does not contain "host not allowed".

- [ ] **Step 3: Add the constant, the function, the check, and fix the three comments**

Modify `crates/banager-core/src/http/real.rs`.

Replace the module doc (`:1-7`) with:

```rust
//! `RealHttpClient` wraps a `reqwest::Client` pinned to the rustls TLS
//! backend (never native-tls/openssl — Global Constraints). Every request
//! carries the `banager/{version}` User-Agent and a 30-second client-wide
//! default timeout; `HttpRequest::timeout` overrides that default on a
//! per-request basis. Redirects are not followed, a 3xx is an error rather
//! than a response, a response body is read to a cap instead of being
//! swallowed whole, and an `https` request to a host outside
//! `ALLOWED_HTTPS_HOSTS` is refused before any connection is opened.
```

After `pub const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;` (`:22`) and before `pub struct RealHttpClient` (`:24`), insert:

```rust

/// The only hosts `RealHttpClient` will open an `https` connection to.
///
/// Every https URL this crate builds names one of these: crates.io
/// (`CargoAdapter::latest_stable_version`), pypi.org
/// (`PipxAdapter::latest_pypi_version`) and registry.ollama.ai
/// (`OllamaAdapter::compare_digests`). `send` refuses any other https host
/// before a connection is opened -- fail closed, so a URL built from data
/// off disk or off the network (a crate name, a model reference) can at
/// worst re-point a request within one of these hosts, never at another
/// one. Plain `http` is exempt: the one http caller is the Ollama daemon at
/// `HostEnv::ollama_host` (default `http://127.0.0.1:11434`), which may
/// legitimately be any machine the user named.
///
/// Adding a host here is a reviewed change with two other halves: the
/// adapter that contacts it, and the "Banager only connects to these
/// hosts" list in `docs/what-we-run.md`, which
/// `tests/what_we_run_test.rs` checks names every entry.
pub const ALLOWED_HTTPS_HOSTS: &[&str] = &["crates.io", "pypi.org", "registry.ollama.ai"];

/// `Ok(())` when `url` is one `send` may fetch: any `http` URL, or an
/// `https` URL whose host is in `ALLOWED_HTTPS_HOSTS` exactly (no
/// subdomains: `api.crates.io` is not `crates.io`). Anything else -- another
/// https host, another scheme, a URL that does not parse -- is
/// `HttpError::Network` naming the reason, the same error a refused
/// redirect gets, since both mean "this client will not go there".
///
/// `Url` lowercases an ASCII host, so the comparison is case-insensitive
/// without the list carrying uppercase spellings.
pub fn host_allowed(url: &str) -> Result<(), HttpError> {
    let parsed = url::Url::parse(url)
        .map_err(|e| HttpError::Network(format!("invalid url {url:?}: {e}")))?;
    match parsed.scheme() {
        "http" => Ok(()),
        "https" => {
            let host = parsed.host_str().unwrap_or("");
            if ALLOWED_HTTPS_HOSTS.contains(&host) {
                Ok(())
            } else {
                Err(HttpError::Network(format!(
                    "host not allowed: {host:?} is not one of {ALLOWED_HTTPS_HOSTS:?} (from {url})"
                )))
            }
        }
        other => Err(HttpError::Network(format!(
            "scheme not allowed: {other:?} in {url}"
        ))),
    }
}
```

Replace the comment at `:42-49` (inside `with_body_limit`, above `.redirect(...)`) with:

```rust
            // reqwest's default is `Policy::limited(10)`: up to ten
            // redirects, to any host, with no https-only guard -- so an
            // https request could be walked to plain http, or to a host
            // Banager never chose, carrying its headers with it. None of
            // the hosts this client talks to -- `ALLOWED_HTTPS_HOSTS` over
            // https, and the Ollama daemon over http -- ever needs a
            // redirect, so the policy is `none` and `send` below turns a
            // 3xx into an error instead of handing it back as a response.
```

Change the top of `send` (`:69-71`) from:

```rust
    async fn send(&self, req: HttpRequest) -> Result<HttpResponse, HttpError> {
        let method = reqwest::Method::from_bytes(req.method.as_bytes())
```

to:

```rust
    async fn send(&self, req: HttpRequest) -> Result<HttpResponse, HttpError> {
        // Before the request is even built: a refused host must never
        // resolve, connect, or carry a header anywhere.
        host_allowed(&req.url)?;
        let method = reqwest::Method::from_bytes(req.method.as_bytes())
```

Replace the comment at `:294-301` (the first lines of `test_real_http_client_does_not_follow_a_redirect_and_reports_it_as_an_error`) with:

```rust
        // reqwest's default policy follows up to ten redirects, to any host,
        // with no https-only guard. None of Banager's requests -- the
        // Ollama daemon over http, `ALLOWED_HTTPS_HOSTS` over https -- ever
        // needs one, so a 3xx means something has gone wrong and following
        // it would carry the request (and its headers) somewhere Banager
        // never chose. The destination here is a second, *watched* server:
        // if it is ever contacted, the redirect was followed.
```

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p banager-core --lib http::real::tests`
Expected: PASS — 13 tests (the 7 existing loopback tests, all over `http://`, plus the 6 new ones). Then `cargo test -p banager-core` in full: the adapter tests use `MockHttpClient`, which does not go through `send`, so nothing else changes.

- [ ] **Step 5: Run the gates**

Run: `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && pnpm test && pnpm typecheck`
Expected: all clean. (The multi-line `assert!` blocks in Step 1 and the 100-column format string in `host_allowed` are exactly the shapes rustfmt rewraps: run `cargo fmt --all` and re-check rather than hand-wrapping.)

- [ ] **Step 6: Commit**

```bash
git add crates/banager-core/src/http/real.rs
git commit -m "$(cat <<'EOF'
Refuse an https request to any host but crates.io, pypi.org and registry.ollama.ai

RealHttpClient's comment named "the four endpoints this client talks
to"; that was a sentence, not a check, and the four counted the http
Ollama daemon. ALLOWED_HTTPS_HOSTS now lists the three https hosts the
adapters contact, host_allowed checks a URL against it, and send calls
that before building a request, so a refused host never resolves or
connects. Plain http is exempt for the daemon, which OLLAMA_HOST may
point at any machine. Phase 4's standalone installers each add their
host with the recipe that contacts it.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 3: `warningKey` exhaustive with a `never` default

**Files:**
- Modify: `src/lib/warnings.ts` — `:10-35` (`warningKey` and its doc), `:37-47` (`warningArgs`), `:67-84` (the two doc comments on `warningText`/`warningTexts`; code unchanged).
- Modify: `src/lib/warnings.test.ts` — `:12-33` (`warningKey` block), `:80-95` (`warningText` block), `:97-110` (`warningTexts` block).
- Modify: `src/lib/types.ts` — `:66-78`, the `Warning` doc comment.
- Modify: `src/lib/types.test.ts` — `:198-201`, the comment inside the `Warning` shape test.
- Modify: `src/components/UninstallDialog.tsx` — `:88-92`, one comment.
- Modify: `src/components/UninstallDialog.test.tsx` — `:140-155`, the one rendered-dialog test built on an unrecognised variant (it asserts the heading is absent, which the `never` default makes false: the raw key is rendered instead).
- Test: `src/lib/warnings.test.ts` and `src/components/UninstallDialog.test.tsx`, plus `pnpm typecheck` with a deliberate mutation (Step 2 and Step 4).

**Interfaces:**
- Consumes: `Warning` (`src/lib/types.ts:79-85`, six variants: `"DependentsUnknown" | { WouldBreak: { names } } | "CompilesLocally" | "NonRegistrySource" | { ThirdPartyRegistry: { host } } | { Message: string }`), the `never`-default idiom of `faultKey` (`src/lib/format.ts:77-94`) and `faultArgs` (`:98-105`).
- Produces: the same five exports with the same signatures — `warningKey(warning: Warning): string | null`, `warningArgs(warning: Warning): Record<string, unknown>`, `warningMessage`, `warningText`, `warningTexts` — so no caller changes. What changes is the compile-time contract: a `Warning` variant without a case in `warningKey` (and, for a payload variant, in `warningArgs`) is a `tsc` error. Step C's eight new variants (spec §6.5) rely on this.

Scope note: the spec names `warningKey` (§6.5, §9.1's table). `warningArgs` is made exhaustive in the same pass because it is the same defect one line later — a payload variant with a key but no values renders its sentence with a literal `{{path}}` in it — and because `faultArgs`, the model the spec points at, is already written that way. Bare-string variants stay a single `return {}` in `warningArgs`: a bare string cannot carry a payload, so there is nothing per-variant to forget.

- [ ] **Step 1: Write the failing tests**

Replace the `describe("warningKey", ...)` block (`src/lib/warnings.test.ts:12-33`) with:

```ts
describe("warningKey", () => {
  it("gives each fixed warning its own key", () => {
    expect(warningKey("DependentsUnknown")).toBe("warnings.dependentsUnknown");
    expect(warningKey("CompilesLocally")).toBe("warnings.compilesLocally");
    expect(warningKey("NonRegistrySource")).toBe("warnings.nonRegistrySource");
    expect(warningKey({ WouldBreak: { names: ["python@3.13"] } })).toBe("warnings.wouldBreak");
    expect(warningKey({ ThirdPartyRegistry: { host: "modelscope.cn" } })).toBe(
      "warnings.thirdPartyRegistry",
    );
  });

  it("has no key for a Message -- its text comes from the wire, not i18n", () => {
    expect(warningKey({ Message: "boom" })).toBeNull();
  });

  it("is null for Message and for nothing else", () => {
    // The runtime half of what `tsc` checks at compile time: every
    // variant of `Warning` is one of these six, and the only one without
    // a `warnings.*` key is the raw-text catch-all. A variant this list
    // does not name is a `never` in `warningKey`'s default branches and
    // does not compile, so there is no "unrecognised variant" to test.
    const all: Warning[] = [
      "DependentsUnknown",
      "CompilesLocally",
      "NonRegistrySource",
      { WouldBreak: { names: ["a"] } },
      { ThirdPartyRegistry: { host: "modelscope.cn" } },
      { Message: "boom" },
    ];
    const keyless = all.filter((warning) => warningKey(warning) === null);
    expect(keyless).toEqual([{ Message: "boom" }]);
  });
});
```

Replace the `describe("warningText", ...)` block (`:80-95`) with:

```ts
describe("warningText", () => {
  it("looks a fixed warning up through t(), with its args", () => {
    expect(warningText(fakeT, "DependentsUnknown")).toBe("warnings.dependentsUnknown");
    expect(warningText(fakeT, { WouldBreak: { names: ["a", "b"] } })).toBe(
      'warnings.wouldBreak({"count":2,"names":"a, b"})',
    );
  });

  it("reads a Message's text directly, bypassing t()", () => {
    expect(warningText(fakeT, { Message: "boom" })).toBe("boom");
  });
});
```

Replace the `describe("warningTexts", ...)` block (`:97-110`) with:

```ts
describe("warningTexts", () => {
  it("renders every warning in order, Message included", () => {
    const warnings: Warning[] = [
      "DependentsUnknown",
      { Message: "boom" },
      { WouldBreak: { names: ["a"] } },
    ];
    expect(warningTexts(fakeT, warnings)).toEqual([
      "warnings.dependentsUnknown",
      "boom",
      'warnings.wouldBreak({"count":1,"names":"a"})',
    ]);
  });

  it("is empty for an empty list", () => {
    expect(warningTexts(fakeT, [])).toEqual([]);
  });
});
```

The `import type { Warning }` at `:3` stays (the new tests use it); `warningArgs`, `warningMessage` blocks (`:35-78`) are unchanged.

Replace the test at `src/components/UninstallDialog.test.tsx:140-155` (`it("shows nothing, not an empty heading, for a warning variant this build does not recognise", …)`) with:

```tsx
  it("renders a warning variant the mirror lacks as its raw key rather than dropping it", async () => {
    // `warningKey` is exhaustive over `Warning`, so this value cannot be
    // written without the cast: it stands for a Rust variant the
    // TypeScript mirror has not caught up with (`types.test.ts` pins the
    // spellings; this is what happens if that check is ever wrong). At
    // runtime it reaches `warningKey`'s `never` default and comes back as
    // the key itself, which i18next hands back unchanged. Showing that is
    // the honest failure: a warning is about something the command is
    // about to do, and a silently shorter list would hide it.
    vi.mocked(invoke).mockResolvedValue(
      issuedPlanFor({ warnings: ["SomeFutureVariant" as unknown as Plan["warnings"][number]] }),
    );

    renderWithProviders(
      <UninstallDialog open onOpenChange={() => {}} request={request} displayName="jq" />,
    );

    expect(await screen.findByText("Before you continue:")).toBeInTheDocument();
    expect(screen.getByText("SomeFutureVariant")).toBeInTheDocument();
  });
```

Nothing else in that file changes: `Plan` is already imported at `:6`, `screen` at `:2`, and `renderWithProviders` (`src/test/setup.ts:46`) wraps the dialog in the real `I18nextProvider` (`src/i18n/index.ts`, `fallbackLng: "en"`), so a key with no entry comes back as itself, and the heading is `t("uninstall.warningsTitle")` = "Before you continue:" (`src/i18n/en.json:202`), rendered only when `planWarnings.length > 0` (`UninstallDialog.tsx:217-224`, one `<li>` per text).

- [ ] **Step 2: Run to verify the defect exists today, at runtime and at compile time**

Run: `pnpm exec vitest run src/components/UninstallDialog.test.tsx`
Expected: FAIL — one test, the rewritten one: `Unable to find an element with the text: Before you continue:`. Today `warningKey("SomeFutureVariant")` answers `null` (`src/lib/warnings.ts:33`), `warningTexts` drops it (`:80-84`), and the dialog shows neither the heading nor the value. (The `warnings.test.ts` changes pass against the current code too; they only removed the drift tests.)

The other half of the red state is the *absence* of a type error. Temporarily append a variant to `Warning` in `src/lib/types.ts:79-85`:

```ts
  | { Message: string }
  | "SomeFutureVariant";
```

Run: `pnpm typecheck`
Expected: PASS — this is the defect. A variant with no copy compiles, and at runtime `warningKey("SomeFutureVariant")` returns `null`, which `warningTexts` drops without a word (`src/lib/warnings.ts:80-84`).

Leave the mutation in place for Step 4; do not commit it.

- [ ] **Step 3: Rewrite `warningKey` and `warningArgs`, and the comments**

Replace `src/lib/warnings.ts:10-47` (the doc comment and body of `warningKey`, then `warningArgs`) with:

```ts
/**
 * The `warnings.*` key for a `Warning`'s copy, or `null` for the `Message`
 * catch-all, whose text is read straight off the wire (see
 * `warningMessage` below).
 *
 * Exhaustive, the way `faultKey` in `src/lib/format.ts` is: every variant
 * of `Warning` is named here, and the `never` defaults make `tsc` fail on
 * one that is not. It used to `return null` for anything it did not
 * recognise, and `warningTexts` drops a `null` without a word -- so a
 * variant added to `types.ts` without a case here reached the uninstall
 * dialog as a silently shorter list, which for a warning that names a
 * file about to be removed is the worst possible failure.
 */
export function warningKey(warning: Warning): string | null {
  if (typeof warning === "string") {
    switch (warning) {
      case "DependentsUnknown":
        return "warnings.dependentsUnknown";
      case "CompilesLocally":
        return "warnings.compilesLocally";
      case "NonRegistrySource":
        return "warnings.nonRegistrySource";
      default: {
        const unhandled: never = warning;
        return unhandled;
      }
    }
  }
  if ("WouldBreak" in warning) return "warnings.wouldBreak";
  if ("ThirdPartyRegistry" in warning) return "warnings.thirdPartyRegistry";
  if ("Message" in warning) return null;
  const unhandled: never = warning;
  return unhandled;
}

/**
 * Interpolation values for `t(warningKey(warning), warningArgs(warning))`.
 * Exhaustive over the payload variants for the same reason `warningKey`
 * is (`faultArgs` in `src/lib/format.ts` is the model): a payload variant
 * with a key but no values here would render its sentence with a literal
 * `{{path}}` in it. Bare-string variants carry nothing, so one `{}` covers
 * them all.
 */
export function warningArgs(warning: Warning): Record<string, unknown> {
  if (typeof warning === "string") return {};
  if ("WouldBreak" in warning) {
    const names = warning.WouldBreak.names;
    return { count: names.length, names: names.join(", ") };
  }
  if ("ThirdPartyRegistry" in warning) return { host: warning.ThirdPartyRegistry.host };
  if ("Message" in warning) return {};
  const unhandled: never = warning;
  return unhandled;
}
```

Replace the doc comment on `warningText` (`:67-73`) with:

```ts
/**
 * One `Warning`, rendered: `t(warningKey(warning), warningArgs(warning))`
 * for a fixed warning, `warningMessage(warning)` for a `Message`. The
 * convenience wrapper every call site actually wants; `warningKey`/
 * `warningArgs`/`warningMessage` stay exported and `t()`-free for testing.
 * The return type keeps `null` for `warningTexts`'s filter; with
 * `warningKey` exhaustive, a fixed warning always has a key and a
 * `Message` always has its text, so it is never actually `null`.
 */
```

Replace the doc comment on `warningTexts` (`:79`) with:

```ts
/** `warnings`, rendered in order. */
```

The bodies of `warningMessage`, `warningText` and `warningTexts` are unchanged.

Replace the `Warning` doc comment in `src/lib/types.ts:66-78` with:

```ts
/**
 * A specific warning `Plan` or `UpdateCandidate` carries. Mirrors `Warning`
 * in crates/banager-core/src/model.rs: bare-string unit variants,
 * externally tagged data variants (`WouldBreak`, whose `names` interpolate
 * and pluralise the copy in `src/lib/warnings.ts`, and
 * `ThirdPartyRegistry`, whose `host` interpolates it), and a `Message`
 * catch-all for warnings this phase does not localise (spec §6's
 * `show_technical_details` backlog item) -- rendered as the raw string it
 * carries, same as before this type existed. A variant added here without
 * copy fails `tsc` in `warningKey`'s `never` default
 * (src/lib/warnings.ts), the way a `Fault` does in `faultKey`;
 * `types.test.ts` keeps a shape test over all of them.
 */
```

Replace the comment in `src/lib/types.test.ts:198-201` (inside the `Warning` shape test, above `const dependentsUnknown`) with:

```ts
    // Mirrors `Warning` in crates/banager-core/src/model.rs -- every
    // spelling below has to match it exactly. `warningKey` is exhaustive
    // over this union, so a variant it lacks fails `tsc`; but a spelling
    // here that differs from Rust's compiles fine and lands the real wire
    // value in `warningKey`'s `never` default at runtime, where it is
    // returned as a raw key. This test is what pins the spellings.
```

Replace the comment in `src/components/UninstallDialog.tsx:88-92` (the five lines between `const hasAffected` at `:87` and `const planWarnings` at `:93`) with:

```tsx
  // Rendered here, once, rather than as text per `<li>`, so the heading
  // above the list is decided by the same list it heads: `warningTexts`
  // is the one rule for turning `plan.warnings` into sentences, and
  // `plan.warnings.length > 0` would be a second one.
```

- [ ] **Step 4: Run to verify — the mutation now fails to compile, then revert it**

With the `"SomeFutureVariant"` mutation from Step 2 still in `src/lib/types.ts`:

Run: `pnpm typecheck`
Expected: FAIL — `src/lib/warnings.ts(…): error TS2322: Type '"SomeFutureVariant"' is not assignable to type 'never'.` at the `const unhandled: never = warning;` inside `warningKey`'s `switch` default. (Both `tsc` programs report it; `tsconfig.test.json` includes `src` too.)

Remove the mutation from `src/lib/types.ts` (the union ends with `| { Message: string };` again).

Run: `pnpm typecheck && pnpm exec vitest run src/lib/warnings.test.ts src/lib/types.test.ts src/components/UninstallDialog.test.tsx src/pages/UpdatesPage.test.tsx`
Expected: PASS — including the rewritten dialog test from Step 1, which now finds both the heading and the raw `SomeFutureVariant` in the list. `git diff src/lib/types.ts` shows only the comment change.

- [ ] **Step 5: Run the gates**

Run: `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && pnpm test && pnpm typecheck`
Expected: all clean. (`src/i18n/completeness.test.ts` is unaffected: no key was added or removed; the five `warnings.*` keys — `dependentsUnknown`, `wouldBreak`, `compilesLocally`, `nonRegistrySource`, `thirdPartyRegistry`; `src/i18n/en.json:93-100`, `zh-CN.json:87-93` — are still referenced by their literals in `warningKey`.)

- [ ] **Step 6: Commit**

```bash
git add src/lib/warnings.ts src/lib/warnings.test.ts src/lib/types.ts src/lib/types.test.ts src/components/UninstallDialog.tsx src/components/UninstallDialog.test.tsx
git commit -m "$(cat <<'EOF'
Make warningKey exhaustive so an unhandled Warning variant fails tsc

warningKey answered null for any variant it had no case for, and
warningTexts drops a null without a word: a Warning added to types.ts
without copy reached the uninstall dialog as a silently shorter list.
Phase 4 adds eight variants that each name a file about to be moved, so
that failure mode has to be a compile error first. warningKey and
warningArgs now end in `never` defaults, the shape faultKey and
faultArgs in format.ts already have. The three unit tests built on an
"unrecognised variant" go, since the type no longer admits one; the
dialog test that expected such a value to vanish now expects it rendered
as its raw key, which is what the never default does at runtime.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 4: `docs/what-we-run.md` for seven sources, checked by a test

**Files:**
- Create: `crates/banager-core/tests/what_we_run_test.rs`
- Rewrite: `docs/what-we-run.md` (all 78 lines replaced)
- Test: the new integration test.

**Interfaces:**
- Consumes: `banager_core::session::Session::{new, adapter_ids}` (as `tests/fixtures_layout_test.rs:13-16` does), `banager_core::events::VecSink`, `banager_core::adapters::AdapterMeta::from_toml` and `.name` (`adapters/mod.rs:83-96`), `banager_core::adapters::brew::BrewAdapter::ENV` (`brew/mod.rs:127-132`, `pub const`), `banager_core::adapters::npm::NpmAdapter::ENV` (`npm.rs:87-91`, `pub const`), `banager_core::http::real::ALLOWED_HTTPS_HOSTS` (Task 2).
- Produces: the document, and four tests that hold it to the code: a `## <meta.name>` section per registered adapter and the title line exactly `# What Banager Runs` (not a substring check for "Homebrew only", which the never-list's "passed through to Homebrew only when it was already set" would trip); every host in `ALLOWED_HTTPS_HOSTS` named; every `NAME=value` of `BrewAdapter::ENV` and `NpmAdapter::ENV` shown; the promise that Homebrew is never passed `--zap`, `--force` or `--ignore-dependencies`, on one line that names all three flags, "never" and Homebrew — a plain `doc.contains("--force")` would be satisfied by the Cargo section's `cargo install --force` rows with the Homebrew sentence gone. Production readers of the document: the person spec §12 wrote it for; of the test: `cargo test --workspace`.

**How the document was checked.** Every sentence in Step 3's text was written from a line of code read at `26bc640`. The table in Step 5 maps each claim to its `file:line`, so a reviewer can re-verify without re-reading seven adapters; the document itself cites files and function names rather than line numbers, which drift with every commit.

- [ ] **Step 1: Write the failing test**

Create `crates/banager-core/tests/what_we_run_test.rs`:

```rust
//! `docs/what-we-run.md` is spec §12's trust file: the one place a person
//! who does not read Rust can see every command Banager runs and every
//! host it contacts. Prose cannot be compiled, so these pin the parts of it
//! the code can vouch for: a section per registered source, every host on
//! the https allowlist, every environment variable brew and npm set, and
//! the three Homebrew flags the file promises are never passed. A source,
//! host or variable added without its line in the document fails here.

use banager_core::adapters::brew::BrewAdapter;
use banager_core::adapters::npm::NpmAdapter;
use banager_core::adapters::AdapterMeta;
use banager_core::events::VecSink;
use banager_core::http::real::ALLOWED_HTTPS_HOSTS;
use banager_core::session::Session;
use std::path::Path;
use std::sync::Arc;

/// The document, read the way every other repo path in this crate's tests
/// is: cargo runs tests with cwd = the package manifest directory
/// (crates/banager-core).
fn read_doc() -> String {
    let path = Path::new("../../docs/what-we-run.md");
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// A `## ` heading whose text is `name`, or `name` followed by a space or a
/// colon -- so `## pip` is found by "pip" and not by "pipx", and
/// `## pip (read-only)` still counts.
fn has_section(doc: &str, name: &str) -> bool {
    doc.lines().any(|line| {
        line.strip_prefix("## ").is_some_and(|text| {
            text == name
                || text.starts_with(&format!("{name} "))
                || text.starts_with(&format!("{name}:"))
        })
    })
}

#[test]
fn test_what_we_run_has_a_section_for_every_registered_source() {
    let doc = read_doc();
    let session = Session::new(Arc::new(VecSink::new()), None);
    for id in session.adapter_ids() {
        let meta_path = format!("../../adapters/meta/{id}.toml");
        let meta = AdapterMeta::from_toml(
            &std::fs::read_to_string(&meta_path)
                .unwrap_or_else(|e| panic!("read {meta_path}: {e}")),
        )
        .unwrap_or_else(|e| panic!("parse {meta_path}: {e}"));
        assert!(
            has_section(&doc, &meta.name),
            "docs/what-we-run.md has no `## {}` section for the registered source {id:?}",
            meta.name
        );
    }
    // The title line, exactly: the phase 0-1 file was headed "What Banager
    // Runs (Phase 0–1: Homebrew only)". A substring check for "Homebrew
    // only" would misfire on ordinary prose ("passed through to Homebrew
    // only when it was already set", in the never-list).
    assert_eq!(
        doc.lines().next(),
        Some("# What Banager Runs"),
        "docs/what-we-run.md's title still narrows the file to one source"
    );
}

#[test]
fn test_what_we_run_names_every_allowed_https_host() {
    let doc = read_doc();
    for host in ALLOWED_HTTPS_HOSTS {
        assert!(
            doc.contains(host),
            "docs/what-we-run.md does not name {host:?}, which ALLOWED_HTTPS_HOSTS allows"
        );
    }
}

#[test]
fn test_what_we_run_shows_every_environment_variable_brew_and_npm_set() {
    let doc = read_doc();
    for (name, value) in BrewAdapter::ENV.iter().chain(NpmAdapter::ENV.iter()) {
        assert!(
            doc.contains(&format!("{name}={value}")),
            "docs/what-we-run.md does not show {name}={value}"
        );
    }
}

#[test]
fn test_what_we_run_promises_the_three_brew_flags_are_never_passed() {
    let doc = read_doc();
    // The promise, not the flags' spellings: the Cargo section lists
    // `cargo install --force`, so `doc.contains("--force")` would pass
    // with the Homebrew sentence gone. One line has to name all three
    // flags, the word "never" and Homebrew together -- the never-list's
    // bullet does.
    let promised = doc.lines().any(|line| {
        let lower = line.to_ascii_lowercase();
        lower.contains("never")
            && lower.contains("homebrew")
            && ["--zap", "--force", "--ignore-dependencies"]
                .iter()
                .all(|flag| line.contains(flag))
    });
    assert!(
        promised,
        "docs/what-we-run.md has no line promising Homebrew is never passed --zap, --force and --ignore-dependencies, which test_plan_never_passes_zap_force_or_ignore_dependencies keeps true"
    );
}
```

The `//!` block is first in the file because an inner doc comment must precede every item; `fixtures_layout_test.rs` puts its explanation on the test fn with `///` instead, and either shape is fine.

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p banager-core --test what_we_run_test`
Expected: FAIL — 4 failed, each on the first thing the current 78-line document lacks:
- `test_what_we_run_has_a_section_for_every_registered_source`: `adapter_ids()` is sorted, so `brew` is checked first, and the current file's `## ` headings are "Environment applied to every invocation", "Read-only commands (…)" and "Write commands (…)" — no `## Homebrew`. Message: ``docs/what-we-run.md has no `## Homebrew` section for the registered source "brew"``.
- `test_what_we_run_names_every_allowed_https_host`: fails on `"crates.io"` (the current file names no host at all).
- `test_what_we_run_shows_every_environment_variable_brew_and_npm_set`: the four brew variables are in the current file, so it fails on the first npm one, `npm_config_update_notifier=false`.
- `test_what_we_run_promises_the_three_brew_flags_are_never_passed`: no line of the current file names all three flags beside "never" and "Homebrew" — its line 71 names only `--ignore-dependencies`, beside `brew uninstall`. (After Step 3 the match is the never-list bullet "Never passes `--zap`, `--force` or `--ignore-dependencies` to Homebrew", which is one line; the Homebrew section's prose says the same across a line break and is not what the test finds.)

- [ ] **Step 3: Rewrite the document**

Replace the entire contents of `docs/what-we-run.md` with:

````markdown
# What Banager Runs

Every command Banager runs, every file it reads or writes, every host it
connects to and every environment variable it sets, for the seven sources
it manages today: Homebrew, npm, pipx, uv, pip (read-only), Cargo and
Ollama. Each sentence describes what the code does now and names the
function it describes, so it can be checked against
`crates/banager-core/src/adapters/` rather than believed.
`crates/banager-core/tests/what_we_run_test.rs` checks the parts a test
can: a section per registered source, every host on the https allowlist,
every environment variable Homebrew's and npm's commands are given, and
the three Homebrew flags this file promises are never passed.

Throughout, `<brew>`, `<npm>` and so on stand for the absolute path of the
executable the adapter found; `{name}` is the one user-chosen argument a
command can carry.

## How Banager runs anything

**Never through a shell.** Every package-manager command is a fixed argv
array run directly against an absolute program path by `RealRunner::run`
(`crates/banager-core/src/runner/real.rs`): `Command::new(program)` with
the arguments appended one by one. No string is ever handed to `sh`, and
nothing Banager downloads is ever piped into one.

**One shell run, at launch, that runs no command.** An app opened from
Finder starts with a minimal `PATH`, so at startup (`run()` in
`src-tauri/src/lib.rs`) the `fix-path-env` crate runs the user's login
shell once — `$SHELL -ilc 'echo -n "_SHELL_ENV_DELIMITER_"; env; echo -n
"_SHELL_ENV_DELIMITER_"; exit'`, with `DISABLE_AUTO_UPDATE=true` in its
environment and the home folder as its working directory — reads the
`PATH` that shell exports, and sets it on Banager's own process
(`fix_vars` in `fix-path-env-rs` at the pinned commit `c4c45d5`). That is
the only time a shell is involved, and all it does is print the
environment.

**What a command inherits.** A child gets Banager's own environment — the
`PATH` above and whatever else the login shell exported — plus the
variables listed in each source's section below (`RealRunner::run` adds
them with `envs` and never clears the environment). Its stdin is
`/dev/null`, so a tool that asks a question gets end-of-file rather than a
wait; its stdout and stderr are piped and, for a write command, streamed
line by line into the operation log. Each child runs in its own process
group. Every command has a timeout (listed below; `RealRunner` caps any
timeout at 24 hours); on timeout or cancel the whole group gets `SIGTERM`,
a grace period, and then `SIGKILL` for whatever is left.

**Where the program comes from.** At launch (`run()` in
`src-tauri/src/lib.rs`), at the start of every refresh, and when the Open
Ollama button is pressed, `HostEnv::discover`
(`crates/banager-core/src/runner/path_env.rs`) reads `PATH`, `HOME`,
`CARGO_HOME` and `OLLAMA_HOST` from Banager's environment and the
effective user id from the process. Every source
but Homebrew finds its executable with `resolve_exe`: the first directory
on that `PATH` containing a regular file of that name. Homebrew is looked
for at three fixed paths instead (its section). The path that was found
is the one previewed and the one run.

**What a user-chosen value may look like.** A package name reaches an
argv only after `validate_package_name`
(`crates/banager-core/src/adapters/mod.rs`): `^[A-Za-z0-9@._+/-]+$`, not
starting with `-`, `/` or `.`, no `..` segment, no `.rb` suffix. Two
sources have their own rule for their own shape of input: npm's search
box (`validate_search_query`: once surrounding whitespace is trimmed,
non-empty, not starting with `-`, and at most 200 bytes of UTF-8 — a CJK
character is three of those; npm receives the query untrimmed) and
Ollama's model references, which contain a colon
(`validate_model_reference`). Every other token in every argv below is a
fixed string.

**Root.** Homebrew refuses to run as root, so Banager never runs a `brew`
command when its effective user ID is 0 (`refuse_if_root`); a Homebrew
found under root is listed as refusing, not as missing. No other source
checks.

**Passwords.** Banager never asks for a password and never handles one.
The only thing it does with one is pass `SUDO_ASKPASS` through, unchanged,
to Homebrew cask installs and upgrades when the variable is already set
in Banager's environment (Homebrew's section); it never sets it on its
own behalf.

## When commands run

**A refresh** happens when the window opens (`refreshIntoCache(…,
"initial")` in `src/lib/events.ts`), when the user presses a Retry or
Refresh control (the status bar after a failed refresh, a source notice),
after every operation finishes, when the "include self-updating apps"
setting changes, after Ollama is opened from its notice, and whenever a
`brew update` a refresh left running in the background ends
(`refresh_on_background_change` in `src-tauri/src/ipc.rs`). Within a
refresh (`refresh_round` in `crates/banager-core/src/session/refresh.rs`)
every source's detect runs concurrently; then, for each instance found,
under that instance's lock, its inventory is read and then its update
check runs. Everything a refresh runs is in the read-only tables below:
no refresh runs a write command, launches an application or asks for a
password.

**An operation** is previewed first: `plan` builds the exact argv and the
front end shows it (`plan_operation` in `src-tauri/src/ipc.rs`; the front
end never builds an argv and sends back only the id of a plan Rust
issued). The plan can be confirmed for ten minutes (`PLAN_LIFETIME` in
`crates/banager-core/src/session/plans.rs`), after which it has to be
previewed again. Before a plan is built, `Session::issue_plan` refuses an
operation on a source that is read-only or not answering, and an upgrade
or uninstall the tool itself reports it will refuse (a pinned package) —
the buttons the pages hide are backed by that refusal, not only by the
page. On confirmation `run_operation` (`crates/banager-core/src/ops/mod.rs`)
takes the plan's locks, runs the command, and then re-reads the inventory
to check what actually happened; an upgrade is also preceded by a reading,
so the version before can be compared with the version after. An install
after which the package is not present, an uninstall after which it still
is, and an upgrade that exits 0 with the version unchanged are all
reported as needing attention, never as success. The one case with less
to go on: when the reading before an upgrade was refused — on Homebrew,
while a `brew update` a refresh left running is still going (Homebrew's
section) — there is nothing to compare, and an upgrade that exits 0 is
reported as a success whenever the package is still present afterwards,
whether or not its version moved. A command that was
cancelled or timed out is reported as unconfirmed unless the reading after
settles it (`run_plan` in `crates/banager-core/src/adapters/mod.rs`, then
`run_operation`).

## Homebrew

Adapter: `BrewAdapter` in `crates/banager-core/src/adapters/brew/mod.rs`.
Verified against Homebrew 7.0.3 (`adapters/meta/brew.toml`).

**Detect.** Banager checks whether `/opt/homebrew/bin/brew`,
`/usr/local/bin/brew` and `/home/linuxbrew/.linuxbrew/bin/brew` exist
(`BrewAdapter::CANDIDATE_PATHS`) — never a `brew` resolved through `PATH`
— and runs `<brew> --version` (30 s) for each that does. Each is its own
instance, with the prefix two directories up from the executable.

**Environment applied to every invocation** (`BrewAdapter::ENV`),
including `--version`, `update` and every plan:

    HOMEBREW_NO_AUTO_UPDATE=1
    HOMEBREW_NO_ENV_HINTS=1
    HOMEBREW_NO_INSTALL_CLEANUP=1
    NO_COLOR=1

Install and upgrade plans additionally carry `SUDO_ASKPASS` when it is
already set in Banager's process environment (`askpass_fn`, read per
plan). It only has any effect for casks whose installer scripts invoke
`sudo`.

**Read-only commands** (background checks; never need a password):

| Purpose | Argv | Timeout |
|---|---|---|
| Detect a Homebrew install | `<brew> --version` | 30 s |
| List installed formulae + casks (`inventory`) | `<brew> info --installed --json=v2` | 120 s |
| Refresh Homebrew's local package index (`maybe_update`) | `<brew> update` | see below |
| List outdated formulae + casks (`check_updates`) | `<brew> outdated --json=v2`, plus `--greedy` when the "include self-updating apps" setting is on | 120 s |
| Qualify the names `outdated` reported (once per `check_updates`) | `<brew> info --installed --json=v2` | 120 s |
| Search by name | `<brew> search {query}` | 30 s |
| Search by name + description | `<brew> search --desc {query}` | 30 s |
| List installed formulae depending on a formula (uninstall preview) | `<brew> uses --installed {name}` | 120 s |

`brew update` runs at most once per six hours per prefix (`update_ttl`).
A refresh waits up to two minutes for it (`UPDATE_PATIENCE`) and then
leaves it running rather than killing it — a `brew update` stopped halfway
can leave Homebrew's git checkout locked; only after thirty minutes
(`UPDATE_BACKSTOP`) is it stopped. While one is running, `inventory`,
`check_updates` and the uninstall preview do not read the catalogue at
all (`AdapterError::IndexUpdating`): the pages keep the previous answer
and say the index is updating, and refresh again when it ends. A `brew
update` that failed is reported as a note on the source (the list may be
out of date), not as a failed source. The search query passes
`validate_package_name`.

**Write commands** (only run after the user reviews and confirms a plan
preview):

| Purpose | Argv | Timeout | Needs a password |
|---|---|---|---|
| Install a formula | `<brew> install --formula {name}` | 1800 s | No |
| Install a cask | `<brew> install --cask {name}` | 1800 s | Sometimes — some cask installers invoke `sudo`; `SUDO_ASKPASS` is passed through when set |
| Uninstall a formula | `<brew> uninstall --formula {name}` | 1800 s | No |
| Uninstall a cask | `<brew> uninstall --cask {name}` | 1800 s | Sometimes — some cask uninstalls invoke `sudo` (removing a `pkgutil` receipt, a launch daemon, a kernel extension) |
| Upgrade one formula | `<brew> upgrade --formula {name}` | 1800 s | No |
| Upgrade one cask | `<brew> upgrade --cask {name}` | 1800 s | Sometimes — as for install |

Every one of these argvs is exactly the verb, the kind flag and the name
(`test_plan_never_passes_zap_force_or_ignore_dependencies` in the same
file). Banager never passes `--zap`, `--force` or `--ignore-dependencies`
to Homebrew, and never runs a bare `brew upgrade`: upgrades are one
confirmed artifact per invocation. Before a write command starts,
`execute` waits up to ten minutes (`OP_UPDATE_WAIT`) for a `brew update`
still running in the background; if it is still running after that,
nothing is run and the operation is reported as failed for that reason.

An upgrade started while that `brew update` is still running also gets
no reading before: `inventory` refuses at once with `IndexUpdating`, no
`brew info` runs, and there is nothing to compare the reading after with
— so an upgrade that then exits 0 is reported as a success whenever the
package is still installed afterwards, whether or not its version moved
(the `Unknown` arm of `run_operation`). This is the one way an exit-0
upgrade whose version did not move is not reported as needing attention.

**Files this adapter reads.** Besides checking that the three candidate
paths exist, the uninstall preview looks at Homebrew's own update lock,
`<prefix>/var/homebrew/locks/update`, to make sure no `brew update` —
Banager's or anyone's — overlapped its `brew uses` read
(`probe_homebrew_update_lock`): the directory is `stat`ed, the file is
opened read-only and never created, and `fcntl(F_GETLK)` asks whether the
lock is held without taking it.

## npm

Adapter: `NpmAdapter` in `crates/banager-core/src/adapters/npm.rs`.
Verified against npm 12.0.2 (`adapters/meta/npm.toml`).

**Detect.** `npm` is the first `npm` on `PATH`. Banager runs `<npm>
prefix -g` (30 s) to learn the global prefix, which is the instance's
identity, and `<npm> --version` (30 s), then asks `access(2)` whether the
current user can write `{prefix}/lib/node_modules` — or, when that does
not exist yet, `{prefix}/lib` or `{prefix}` (`real_prefix_is_writable`).
A prefix this user cannot write (a Node installed from nodejs.org's
package leaves a root-owned one) makes the instance read-only. An npm that
will not answer `prefix -g` is still listed, as not responding.

**Environment applied to every invocation** (`NpmAdapter::ENV`):

    NO_COLOR=1
    npm_config_update_notifier=false
    npm_config_fund=false

**Read-only commands:**

| Purpose | Argv | Timeout |
|---|---|---|
| Global prefix | `<npm> prefix -g` | 30 s |
| Version | `<npm> --version` | 30 s |
| List global packages (`inventory`) | `<npm> ls -g --depth=0 --json` | 60 s |
| List outdated global packages (`check_updates`) | `<npm> outdated -g --json` | 60 s |
| Search | `<npm> search --json --searchlimit 20 {query}` | 30 s |

`npm ls` exits 1 for non-fatal reasons (a peer dependency mismatch), so
exit 0 and 1 are both read. `npm outdated` exits 1 whenever it finds
something outdated, so a non-zero exit with findings is a result, and a
non-zero exit with nothing to show is reported as "could not check" for
every package rather than as "everything is up to date" — listing every
package that way takes one more run of `<npm> ls -g --depth=0 --json`, so
a refresh whose `outdated` failed runs the inventory command twice. The
search query passes `validate_search_query`.

**Write commands:**

| Purpose | Argv | Timeout | Needs a password |
|---|---|---|---|
| Install | `<npm> install -g {name}` | 600 s | No |
| Uninstall | `<npm> uninstall -g {name}` | 600 s | No |
| Upgrade | `<npm> install -g {name}@latest` | 600 s | No |

A plan is refused at click time if the prefix has stopped being writable
since the refresh that listed it.

## pipx

Adapter: `PipxAdapter` in `crates/banager-core/src/adapters/pipx.rs`.
Verified against pipx 1.17.3 (`adapters/meta/pipx.toml`).

**Detect.** `pipx` is the first `pipx` on `PATH`; `<pipx> --version`
(30 s). No environment variables are added to any pipx command.

**Read-only commands:**

| Purpose | Argv | Timeout |
|---|---|---|
| Version | `<pipx> --version` | 30 s |
| List installed tools (`inventory`) | `<pipx> list --json` | 60 s |
| List outdated tools (`check_updates`, pipx ≥ 1.16) | `<pipx> list --outdated` | 60 s |

If `pipx list --outdated` exits non-zero, `<pipx> list --json` is run once
more so every installed tool can be listed as "could not check", with the
reason — one more process than the table shows, on that path only.

On a pipx older than 1.16, which has no `list --outdated`, Banager
instead asks PyPI about each installed tool: `GET
https://pypi.org/pypi/{name}/json` (30 s each), the name percent-encoded.
A tool PyPI does not answer for is listed as "could not check", never as
an error for the whole source. pipx has no search command Banager uses.

**Write commands:**

| Purpose | Argv | Timeout | Needs a password |
|---|---|---|---|
| Install | `<pipx> install {name}` | 600 s | No |
| Uninstall | `<pipx> uninstall {name}` | 600 s | No |
| Upgrade | `<pipx> upgrade {name}` | 600 s | No |

## uv

Adapter: `UvAdapter` in `crates/banager-core/src/adapters/uv.rs`.
Verified against uv 0.12.17 (`adapters/meta/uv.toml`).

**Detect.** `uv` is the first `uv` on `PATH`; `<uv> --version` (30 s). No
environment variables are added to any uv command, and Banager makes no
network request of its own for uv: `uv tool list --outdated` reaches PyPI
itself, under uv's own configuration.

**Read-only commands:**

| Purpose | Argv | Timeout |
|---|---|---|
| Version | `<uv> --version` | 30 s |
| List installed tools (`inventory`) | `<uv> tool list --show-paths` | 60 s |
| List outdated tools (`check_updates`) | `<uv> tool list --outdated` | 60 s |

If `uv tool list --outdated` exits non-zero, `<uv> tool list --show-paths`
is run once more so every installed tool can be listed as "could not
check", with the reason — one more process than the table shows, on that
path only. uv has no tool-search command Banager uses.

**Write commands:**

| Purpose | Argv | Timeout | Needs a password |
|---|---|---|---|
| Install | `<uv> tool install {name}` | 600 s | No |
| Uninstall | `<uv> tool uninstall {name}` | 600 s | No |
| Upgrade | `<uv> tool upgrade {name}` | 600 s | No |

## pip (read-only)

Adapter: `PipAdapter` in `crates/banager-core/src/adapters/pip.rs`.
Verified against pip 26.2.1 (`adapters/meta/pip.toml`).

**Detect.** For each of `python3.14`, `python3.13`, `python3.12`,
`python3.11`, `python3.10`, `python3` and `python` found on `PATH`
(`PipAdapter::CANDIDATE_INTERPRETERS`), Banager canonicalises the path so
two names for one interpreter count once, and runs `<python> -m pip
--version` (30 s). Every pip instance is read-only by design. No
environment variables are added, and Banager makes no network request of
its own for pip: `pip list --outdated` reaches PyPI itself.

**Read-only commands:**

| Purpose | Argv | Timeout |
|---|---|---|
| Version | `<python> -m pip --version` | 30 s |
| List packages (`inventory`) | `<python> -m pip list --format=json` | 60 s |
| List packages nothing else depends on (`inventory`, to tell dependencies apart) | `<python> -m pip list --format=json --not-required` | 60 s |
| List outdated packages (`check_updates`) | `<python> -m pip list --outdated --format=json` | 60 s |

**Write commands: none.** `PipAdapter::plan` refuses every install,
uninstall and upgrade before building an argv, so no pip write command
can be previewed, let alone run; the pages show no such button for a pip
package. pip has no search command Banager uses.

## Cargo

Adapter: `CargoAdapter` in `crates/banager-core/src/adapters/cargo.rs`.
Verified against cargo 1.98.1 (`adapters/meta/cargo.toml`).

**Detect.** `cargo` is the first `cargo` on `PATH`; `<cargo> --version`
(30 s). Banager also looks for `cargo-binstall` on the same `PATH` and
remembers the path found for plans. The Cargo home is `CARGO_HOME` from
the environment, else `~/.cargo`. No environment variables are added to
any cargo command.

**Read-only reads.** `inventory` runs no command: it reads
`<CARGO_HOME>/.crates2.json`, the file `cargo install` keeps its records
in (a missing file means nothing is installed). `check_updates` reads the
same file and, for each crate installed from the registry, asks crates.io
once: `GET https://crates.io/api/v1/crates/{name}` (30 s), the name
percent-encoded. Crates installed from a git repository or a local path
are never looked up; they are listed as "could not check" with that
reason. Cargo has no search command Banager uses.

**Write commands:**

| Purpose | Argv | Timeout | Needs a password |
|---|---|---|---|
| Install, cargo-binstall found | `<cargo-binstall> -y {name}` | 1800 s | No |
| Install, otherwise | `<cargo> install {name}` (previewed with a "compiles locally" warning) | 1800 s | No |
| Upgrade, cargo-binstall found | `<cargo-binstall> -y --force {name}` | 1800 s | No |
| Upgrade, otherwise | `<cargo> install --force {name}` (same warning) | 1800 s | No |
| Uninstall | `<cargo> uninstall {name}` | 300 s | No |

`--force` here is cargo's own flag, meaning "reinstall even though a
version of this crate is already installed" — it is how cargo upgrades a
binary. It is the only `--force` Banager passes to any tool, and it never
goes to Homebrew.

## Ollama

Adapter: `OllamaAdapter` in `crates/banager-core/src/adapters/ollama/mod.rs`.
Verified against Ollama 0.34.1 (`adapters/meta/ollama.toml`).

**Detect.** `ollama` is the first `ollama` on `PATH`; `<ollama> --version`
(30 s) — never `ollama list`, which on macOS launches Ollama.app as a side
effect, and a background refresh must never launch an application. The
daemon is asked over HTTP instead: `GET {host}/api/tags` (10 s), where
`{host}` is `OLLAMA_HOST` from the environment, normalised to an absolute
http(s) URL, or Ollama's default `http://127.0.0.1:11434`
(`DEFAULT_HOST`). Banager also checks whether `/Applications/Ollama.app`
or `~/Applications/Ollama.app` is a directory: a daemon on this Mac that
does not answer while the app is there is reported as not running, with
an Open Ollama button; anything else that does not answer is reported as
not responding, with no button. No environment variables are added to any
ollama command.

**Read-only reads:**

| Purpose | Request or argv | Timeout |
|---|---|---|
| Version | `<ollama> --version` | 30 s |
| Is the daemon answering (detect) | `GET {host}/api/tags` | 10 s |
| List pulled models (`inventory`) | `GET {host}/api/tags` | 30 s |
| Is a model current (`check_updates`, per model) | `GET https://registry.ollama.ai/v2/{namespace}/{name}/manifests/{tag}` with `Accept: application/vnd.docker.distribution.manifest.v2+json` | 30 s |

For each pulled model `check_updates` reads the local manifest file
`~/.ollama/models/manifests/registry.ollama.ai/{namespace}/{name}/{tag}`
and compares its layer digests with the registry's. The three name parts
come out of the daemon's `/api/tags` answer, so before any path is built
each must be a plain path segment (`contained_manifest_path`: nothing
absolute, no `..`), and in the URL each is percent-encoded. The registry
manifest is always fetched from `registry.ollama.ai`, whatever registry
the model was pulled from. Ollama has no search command Banager uses.

**Write commands:**

| Purpose | Argv | Timeout | Needs a password |
|---|---|---|---|
| Install (pull) | `<ollama> pull {model}` | 3600 s | No |
| Upgrade (pull again) | `<ollama> pull {model}` | 3600 s | No |
| Uninstall | `<ollama> rm {model}` | 3600 s | No |

Writes go through the CLI, not the daemon's HTTP API, so every guarantee
an operation has — the preview, the log, cancel, the check afterwards — is
the same as for every other source. A model reference whose first segment
names a registry other than `registry.ollama.ai` or `hf.co` is previewed
with a warning naming that host; it is never blocked, since `ollama pull`
is what will contact it, under Ollama's own configuration.

**The Open Ollama button** runs `/usr/bin/open -a Ollama`
(`open_ollama_app_argv` in `src-tauri/src/ipc.rs`), with its stdin,
stdout and stderr pointed at `/dev/null`, only when the user presses it and only when
Ollama.app was found; it waits up to 20 seconds for `open` to report
whether LaunchServices accepted the request. It is the one launch in the
app that is not a package-manager command, and it never happens during a
refresh.

## Files Banager reads

All read-only, none saved anywhere else, none uploaded:

- Homebrew: whether the three candidate `brew` paths exist;
  `<prefix>/var/homebrew/locks` and the `update` lock file in it, during
  the uninstall preview (Homebrew's section).
- npm: whether `{prefix}/lib/node_modules`, `{prefix}/lib` or `{prefix}`
  is writable, via `access(2)`.
- pip: the canonical path of each interpreter found, to count it once.
- Cargo: `<CARGO_HOME>/.crates2.json`; whether `cargo-binstall` is on
  `PATH`.
- Ollama: whether `/Applications/Ollama.app` or `~/Applications/Ollama.app`
  is a directory; `~/.ollama/models/manifests/registry.ollama.ai/{namespace}/{name}/{tag}`
  for each pulled model.
- Banager's own `settings.json` in its application data directory
  (`settings::load`; a missing or unreadable file means default settings).

## Files Banager writes

One: `settings.json` in Banager's application data directory
(`settings::save`, written to a `settings.json.tmp.<n>` beside it and
renamed into place, so a crash mid-write cannot leave it corrupt; the
directory is created if it is missing). Nothing else on the Mac is
written, moved or deleted by Banager itself: every change to what is
installed is made by the tool named in the preview, running the command
shown there.

## Network: Banager only connects to these hosts

Every request goes through `RealHttpClient`
(`crates/banager-core/src/http/real.rs`), and it refuses, before opening a
connection, any `https` request whose host is not on this list
(`ALLOWED_HTTPS_HOSTS`, checked by `host_allowed` at the top of `send`):

| Host | What is fetched | By |
|---|---|---|
| `crates.io` | `GET /api/v1/crates/{name}` — the newest stable version of one crate | Cargo's `check_updates` |
| `pypi.org` | `GET /pypi/{name}/json` — the newest version of one package | pipx's `check_updates`, on pipx < 1.16 only |
| `registry.ollama.ai` | `GET /v2/{namespace}/{name}/manifests/{tag}` — one model's manifest | Ollama's `check_updates` |

Plain `http` is exempt from the list for one caller: the Ollama daemon at
`OLLAMA_HOST` or `http://127.0.0.1:11434` (`GET /api/tags`), which may be
a machine the user named.

Every request: TLS through rustls; the header `User-Agent:
banager/<version>`; no other header of Banager's own, except `Accept` on
the Ollama registry request — the HTTP library adds what the protocol
needs, `Host` and `Accept: */*`, and nothing else; no cookies, no
credentials, nothing about this Mac in the request; a timeout per request
(listed in each source's table: 30 s unless stated, and the daemon check
in Ollama's detect is 10 s); a response body limit of 8 MiB
(`MAX_RESPONSE_BYTES`); and no redirect is ever followed — a 3xx is an
error. Nothing is ever sent by any method but `GET`.

Three things are outside that client and worth saying out loud. The
window itself cannot make a network request: its content security policy
is `connect-src 'self'` (`src-tauri/tauri.conf.json`). The Tauri opener
plugin — the one that opens a URL or a path in another application — is
registered (`run()` in `src-tauri/src/lib.rs`) and the main window is
permitted to call it (`opener:default` in
`src-tauri/capabilities/default.json`), but nothing in the front end
calls it: no homepage link, no "reveal in Finder"; when one ships, this
paragraph changes. And the Tauri updater
plugin is compiled in and configured with the endpoint
`https://github.com/Brulek/Banager/releases/latest/download/latest.json`
(`src-tauri/tauri.conf.json`, `plugins.updater`), but nothing in Banager
calls it yet, so no request to it is made; when app self-update ships,
this paragraph changes.

The tools Banager runs make their own connections — `brew`, `npm`, `pip`,
`pipx`, `uv`, `cargo`, `cargo-binstall` and `ollama pull` each reach
whatever index or registry they are configured to use. Those are the
tools' connections, under the tools' configuration; Banager neither
chooses nor sees them.

## What Banager never does

- Never runs a shell for any command, and never pipes a download into one
  (`curl … | sh`). The one shell run is the `PATH` read at launch, above.
- Never runs an installer script, and never reruns one to update a tool.
- Never passes `--zap`, `--force` or `--ignore-dependencies` to Homebrew
  (the brew plan test), and never runs a bare `brew upgrade`.
- Never runs a `brew` command as root.
- Never runs a write command from a refresh, and never runs one without a
  preview the user confirmed within the last ten minutes.
- Never launches an application from a refresh; `open -a Ollama` runs
  only when the button is pressed.
- Never asks for, stores or types a password; `SUDO_ASKPASS` is passed
  through to Homebrew only when it was already set.
- Never writes, moves or deletes a file on the Mac itself, other than its
  own `settings.json`; never edits a shell startup file.
- Never connects to an `https` host that is not on the list above, and
  never follows a redirect.
- Never reports an operation as succeeded on the tool's exit code alone:
  the inventory is re-read afterwards, and a package still present after
  an uninstall, one missing after an install, or an upgraded version that
  did not move is reported as needing attention — the last whenever a
  version before could be read; when Homebrew's index was updating and
  the reading before was refused, presence afterwards is all there is to
  go on (Homebrew's section).
````

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p banager-core --test what_we_run_test`
Expected: PASS — 4 passed.

- [ ] **Step 5: Verify every claim against the code (reviewer's table)**

Each row is a sentence or table cell in Step 3's text and the line(s) at `26bc640` it was written from. The implementer re-checks each before committing; a reviewer uses it as the gate.

| Document claim | Evidence (file:line at `26bc640`) |
|---|---|
| argv array, absolute program, `Command::new` + args, no shell | `runner/real.rs:664-665`; `adapters/mod.rs:487-497` (`run_plan` builds `CommandSpec` from `plan.program`/`plan.args`) |
| login shell run once at launch, `-ilc`, the exact echo/env string, `DISABLE_AUTO_UPDATE=true`, cwd home, reads PATH only | `src-tauri/src/lib.rs:18`; `src-tauri/Cargo.toml:33` (pinned rev `c4c45d5…`); fix-path-env `src/lib.rs:38-56`, `:91-93` (`fix` = `fix_vars(&["PATH"])`) |
| child inherits Banager's environment plus listed vars; `envs`, no clear | `runner/real.rs:666` (no `env_clear` anywhere in the file) |
| stdin `/dev/null`, stdout/stderr piped, own process group | `runner/real.rs:670-673` |
| write commands streamed line by line; read-only pass `None` | `adapters/mod.rs:478-486`, `:499`; every `run_*` helper passes `None` (`brew/mod.rs:321`, `npm.rs:137`, `pipx.rs:255`, `uv.rs:180`, `pip.rs:192`, `cargo.rs:150`, `ollama/mod.rs:264`) |
| timeout capped at 24 h; SIGTERM, grace, SIGKILL on the group | `runner/real.rs:417`, `:707`; `:3-4` (module doc) |
| `HostEnv::discover` reads PATH, HOME, CARGO_HOME, OLLAMA_HOST and the effective uid; runs at launch, at every refresh, and on Open Ollama | `runner/path_env.rs:64-84` (`libc::geteuid()` at `:71`); `src-tauri/src/lib.rs:21`, `src-tauri/src/ipc.rs:34`, `:544` (`grep -rn 'HostEnv::discover' src-tauri/src crates/banager-core/src` finds no other production call) |
| `resolve_exe`: first `is_file` on PATH | `runner/path_env.rs:87-95` |
| `validate_package_name` rule | `adapters/mod.rs:348-365` (the doc comment at `:338-347` states the regex) |
| npm `validate_search_query`: trimmed, non-empty, not `-`, ≤ 200 bytes; npm receives the untrimmed query | `adapters/npm.rs:27-33` (`trimmed.len() > 200` at `:29` — bytes, not chars), `:338` (`query.to_string()`, untrimmed) |
| Ollama `validate_model_reference` allows `:` | `adapters/ollama/mod.rs:27-43` |
| brew refuses as root; found under root listed as refusing | `brew/mod.rs:273-280`, `:654-656`, `:667`, `:674-675`, `:707-708`; no other adapter reads `euid` |
| `SUDO_ASKPASS` passthrough only when already set, only Install/Upgrade | `brew/mod.rs:192`, `:1204-1207`, `:1293-1296`; Uninstall uses plain `env_vec()` `:1278` |
| refresh triggers: initial, retry/refresh control, post-operation, setting change, Ollama opened, background change | `src/lib/events.ts:129`, `:173`; `src/lib/queries.ts:60`, `:89`, `:152`; `src/components/SnapshotStatus.tsx:15,23`, `src/components/SourceNotices.tsx:25`; `src-tauri/src/ipc.rs:124-129` |
| refresh: detect concurrently, then per instance under lock inventory → check_updates | `session/refresh.rs:154-168`, `:275-277`, `:292`, `:329-333` |
| front end never builds an argv; submits only a plan id | `src-tauri/src/ipc.rs:131-136` |
| `PLAN_LIFETIME` ten minutes | `session/plans.rs:18` |
| gate refuses read-only / unavailable / blocked before `plan` | `session/plans.rs:172-187` |
| `run_operation`: locks, execute, reconcile after; Upgrade reads before; outcome rules | `ops/mod.rs:638-646`, `:659-661`, `:668`, `:677-720`; stopped arm `:787-799` |
| upgrade with no reading before (brew `IndexUpdating` while `brew update` runs, or any `Err`): exit 0 + still present → Succeeded whether or not the version moved | `ops/mod.rs:621-625` (comment), `:645` (`read.ok()` keeps `before` `None`), `:88-100` (`version_change`: no before → `Unknown`), `:704-706` (comment), `:715-717` (`VersionChange::Changed \| VersionChange::Unknown => Outcome::Succeeded`) |
| `run_plan`: cancelled/timed-out → Unconfirmed; exit 0 → Succeeded | `adapters/mod.rs:500-505` |
| Homebrew verified 7.0.3 | `adapters/meta/brew.toml` |
| `CANDIDATE_PATHS`, existence check, never PATH | `brew/mod.rs:173-177`, `:193`, `:669-673` |
| `brew --version` 30 s with ENV | `brew/mod.rs:677-685` |
| prefix two directories up | `brew/mod.rs:289-295` |
| `BrewAdapter::ENV` four variables, on every invocation | `brew/mod.rs:127-132`; applied `:311` (`run_brew`), `:457` (`update`), `:680` (`--version`), `:1204`, `:1278`, `:1293` |
| `info --installed --json=v2` 120 s; refused with `IndexUpdating` while updating | `brew/mod.rs:734-747` |
| `brew update`: 6 h TTL, 120 s patience, 30 min backstop, left running | `brew/mod.rs:188`, `:138`, `:156`, `:435-439`, `:481-487`, `:453-458` |
| `outdated --json=v2` 120 s, `--greedy` from the setting | `brew/mod.rs:789-793`; `src-tauri/src/ipc.rs:38-43` |
| second `info --installed` per check | `brew/mod.rs:820` |
| `search {q}` and `search --desc {q}`, 30 s each; query validated | `brew/mod.rs:834-858` |
| `uses --installed {name}` 120 s; refused when update runs before/during | `brew/mod.rs:1242-1258` |
| failed update → source note | `brew/mod.rs:784-788` |
| write argvs, 1800 s, needs_password for casks | `brew/mod.rs:1199-1219`, `:1221-1286`, `:1287-1309` |
| execute waits ≤ 10 min for a background update, then fails without running | `brew/mod.rs:171`, `:1326-1338` |
| update lock probe: stat dir, open read-only, never create, `F_GETLK` | `brew/mod.rs:998-1037`, `:992-993`; wired via `:113-114`, `:194`, `:579` |
| npm verified 12.0.2 | `adapters/meta/npm.toml` |
| npm detect: `prefix -g` 30 s, `--version` 30 s, `access(2)` on the three candidates, read-only reason, not responding | `adapters/npm.rs:141-255`, `:49-69` |
| `NpmAdapter::ENV` on every invocation | `adapters/npm.rs:87-91`, `:128`, `:148`, `:212`, `:386` |
| npm read-only table | `adapters/npm.rs:264-270`, `:293-298`, `:333-340` |
| `ls` exit 0/1; `outdated` exit 1 semantics; inventory run once more on the failure path | `adapters/npm.rs:273-281`, `:301-321` (`self.inventory(inst)` at `:320`) |
| npm write table, 600 s, click-time refusal | `adapters/npm.rs:366-393` |
| pipx verified 1.17.3 | `adapters/meta/pipx.toml` |
| pipx detect, no env | `adapters/pipx.rs:190-236`, `:247` |
| pipx read-only table; ≥ 1.16 rule; PyPI fallback URL, 30 s, percent-encoded; inventory run once more when `list --outdated` fails | `adapters/pipx.rs:38-43`, `:266-267`, `:338-350`, `:279-299`, `:365-366`; `:355-361` (`self.inventory(inst)` at `:358`) |
| pipx write table 600 s; no search | `adapters/pipx.rs:388-404`, `:370-378` |
| uv verified 0.12.17 | `adapters/meta/uv.toml` |
| uv detect, tables, no env, no search; inventory run once more when `tool list --outdated` fails | `adapters/uv.rs:113-161`, `:172`, `:188-206`, `:213-237` (`self.inventory(inst)` at `:231`), `:239-247`, `:257-277` |
| pip verified 26.2.1 | `adapters/meta/pip.toml` |
| pip interpreters, canonicalise, `-m pip --version` 30 s, read-only by design | `adapters/pip.rs:68-76`, `:92-111`, `:163` |
| pip read-only table | `adapters/pip.rs:174-191`, `:216-217`, `:253-270` |
| pip: no writes, plan refuses, execute unreachable, no search | `adapters/pip.rs:325-334`, `:342-352`, `:309-317` |
| cargo verified 1.98.1 | `adapters/meta/cargo.toml` |
| cargo detect: PATH, cargo-binstall remembered, CARGO_HOME or `~/.cargo`, `--version` 30 s, no env | `adapters/cargo.rs:90-92`, `:129-158` |
| `.crates2.json` read; missing = nothing installed | `adapters/cargo.rs:186-198`, `:204-205`, `:253` |
| crates.io URL, 30 s, percent-encoded; git/path never looked up | `adapters/cargo.rs:218-231`, `:262-273` |
| cargo write table: binstall `-y`/`-y --force`, `install`/`install --force`, warning, 1800 s; uninstall 300 s | `adapters/cargo.rs:315-355` |
| cargo has no search | `adapters/cargo.rs:296-304` |
| Ollama verified 0.34.1 | `adapters/meta/ollama.toml` |
| `ollama --version` 30 s, never `ollama list` | `adapters/ollama/mod.rs:243-267` |
| `GET {host}/api/tags` 10 s in detect; host rule; `DEFAULT_HOST` | `adapters/ollama/mod.rs:272-283`, `:135`, `:141-145`; `runner/path_env.rs:40-57` |
| Ollama.app in `/Applications` or `~/Applications`, `is_dir`; NotRunning vs NotResponding | `adapters/ollama/mod.rs:162-176`, `:207-209`, `:312-318` |
| `GET {host}/api/tags` 30 s in inventory | `adapters/ollama/mod.rs:331-339` |
| local manifest path under `~/.ollama`; containment check | `adapters/ollama/mod.rs:289`, `:475`, `:373-374`, `:76-103` |
| registry URL, `Accept` header, 30 s, percent-encoded, always registry.ollama.ai | `adapters/ollama/mod.rs:389-407` |
| Ollama write table 3600 s; pull for both; third-party warning never blocks; `hf.co` | `adapters/ollama/mod.rs:518-542`, `:108`, `:124-131` |
| Open Ollama: `/usr/bin/open -a Ollama`, streams null, only on the button, only when app found, 20 s | `src-tauri/src/ipc.rs:423-428`, `:440`, `:484-490`, `:499`, `:544-548`; `adapters/ollama/mod.rs:292-296` |
| settings.json read; missing/unreadable → defaults | `crates/banager-core/src/settings.rs:44-49`; path `src-tauri/src/lib.rs:28` |
| settings.json the only write; tmp + rename; dir created | `crates/banager-core/src/settings.rs:60-72`; `grep -rn 'fs::write\|fs::rename\|remove_file\|remove_dir\|create_dir' crates/banager-core/src src-tauri/src` hits only `settings.rs:62,70,71` outside `#[cfg(test)]` |
| allowlist, `host_allowed` first in `send`, http exempt | `http/real.rs` (Task 2) |
| rustls, UA, 8 MiB, no redirects; 30 s is the client-wide default and every request overrides it with its own | `http/real.rs:38-53` (`:51` the default), `:75` (`.timeout(req.timeout)`), `:22`, `:87-97`, `:115-119`; per-request values `adapters/cargo.rs:230`, `pipx.rs:289`, `ollama/mod.rs:279` (10 s, detect), `:338`, `:404` |
| Banager's own headers: none but UA, except Ollama `Accept`; `GET` only | `adapters/cargo.rs:221,229`; `adapters/pipx.rs:283,288`; `adapters/ollama/mod.rs:276,278`, `:335,337`, `:398,400-403`; `http/mod.rs:14-16` |
| the library adds `Host` and `Accept: */*`, nothing else | reqwest 0.13 (`crates/banager-core/Cargo.toml:25`): `ClientBuilder::new` seeds `Accept: */*` (`~/.cargo/registry/src/*/reqwest-0.13.5/src/async_impl/client.rs:285`); `Host` is HTTP/1.1's own; `real.rs:39-52` sets only the UA, the redirect policy and the timeout on top, and `send` (`:72-78`) adds only `req.headers` |
| CSP `connect-src 'self'` | `src-tauri/tauri.conf.json` `app.security.csp` |
| updater plugin registered, endpoint, never called | `src-tauri/src/lib.rs:26`; `src-tauri/tauri.conf.json` `plugins.updater.endpoints`; `grep -rn updater src src-tauri/src` finds only the registration and an unrelated comment in `src/lib/events.ts:40` |
| opener plugin registered and permitted, never called from the front end | `src-tauri/src/lib.rs:25` (`tauri_plugin_opener::init()`); `src-tauri/capabilities/default.json` (`opener:default`); `package.json:22` (`@tauri-apps/plugin-opener`, installed but unimported); `grep -rn 'plugin-opener\|openUrl\|openPath' src` is empty — the `opener` in `src/components/LogDrawer.tsx:70` is a local variable for the element that opened the drawer |
| never launches an app from a refresh | `adapters/ollama/mod.rs:243-248`; `session/refresh.rs` calls only `detect`/`inventory`/`check_updates` |

- [ ] **Step 6: Run the gates**

Run: `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && pnpm test && pnpm typecheck`
Expected: all clean. (`cargo test --workspace` now includes `what_we_run_test`. The long `assert!` messages and the `unwrap_or_else` chains in the new test file are shapes rustfmt rewraps: run `cargo fmt --all` and re-check rather than hand-wrapping.)

- [ ] **Step 7: Commit**

```bash
git add docs/what-we-run.md crates/banager-core/tests/what_we_run_test.rs
git commit -m "$(cat <<'EOF'
Rewrite what-we-run.md for all seven sources, and test the parts a test can

The file was titled "Phase 0-1: Homebrew only" and said nothing about
npm, pipx, uv, pip, cargo or Ollama, three of which contact the network.
It now lists, per source, every command and its timeout, every
environment variable, every file read, and every host; then what
Banager writes (its own settings.json), the https allowlist, the one
shell run at launch, and what it never does. A new integration test
holds the document to the code: a section per registered adapter, every
host in ALLOWED_HTTPS_HOSTS, every variable in BrewAdapter::ENV and
NpmAdapter::ENV, and the three brew flags the plan test promises.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

---

## Self-review against the spec

**§十 row A** — "docs/what-we-run.md 为六个阶段 3 来源补齐；brew 三标志测试；ALLOWED_HTTPS_HOSTS（现有三主机）+ send() 检查 + 改掉 :43-49 注释；warningKey 改 never 穷尽": Task 4, Task 1, Task 2, Task 3 respectively. "无，可最先合": no task here touches `Plan`, `Warning`'s variants, `Recipe`, or any file steps B–F create.

**§4.2** — constant spelled `ALLOWED_HTTPS_HOSTS` in `http/real.rs` ✔ (Task 2); `send()` refuses an https host off the list with `HttpError::Network("host not allowed: …")` ✔; `http://` exempt ✔; the `:43-49` comment rewritten to point at the constant ✔ (and the same list in the test comment at `:294-301`); readers: `send`, `what-we-run.md`'s host list ✔ (Task 4), `tests/what_we_run_test.rs` ✔. The third reader the spec names — "一条单元测试遍历 RECIPES 每个 Latest 的 URL" — belongs to step B, since `RECIPES` does not exist yet; `host_allowed` is `pub` precisely so that test is one call. **Deliberate deviation from the spec's literal list:** the spec's code block lists six hosts; this plan adds the three current ones only, per the step-A brief ("phase-4 hosts are added by the steps that add their producers") and the "every field names its producer in the same task" rule. Steps B, D and E each add one host with its recipe.

**§6.5 "必须一起改"** — `warningKey` is a `switch` + `never` default for bare strings and a `never` fall-through for objects, modelled on `format.ts:77-94` ✔. `warningArgs` is made exhaustive too (justified in Task 3's scope note; `faultArgs`, `format.ts:98-105`, is the same idiom). The spec's other two halves of that sentence — "`types.test.ts` 与 `model.rs` 的形状测试各加新变体" — are per-variant and land with the variants in step C; this task updates the existing shape test's *comment* only.

**§6.7** — `brew/mod.rs` test module gains one test covering cask and formula × Install/Uninstall/Upgrade, asserting `args` contain none of the three flags ✔ and, stronger, equal exactly `[verb, flag, name]`; `docs/what-we-run.md`「Banager 绝不做的事」names the three flags ✔ (Task 4, checked by `test_what_we_run_promises_the_three_brew_flags_are_never_passed`).

**§9.4 (the two lines for this step)** — "`brew/mod.rs` 三种 `plan()` 不含 `--zap`/`--force`/`--ignore-dependencies`" ✔ Task 1; "`http/real.rs` 名单外 https 主机被 `send()` 拒绝、`http://` 放行" ✔ Task 2 (`test_real_http_client_refuses_an_https_host_off_the_list_before_connecting` and `test_host_allowed_exempts_plain_http_whatever_the_host`, plus the seven pre-existing `http://` loopback tests that keep passing).

**§9.5** — "重写成每来源一节（只读表：后台检查、不要密码；写表：先预览后确认），从各适配器的 `plan()`/`detect()` 抄 argv 与超时" ✔ seven sections, each with a read-only table headed "background checks; never need a password" (Homebrew's, then "Read-only commands" for the rest) and a write table headed "only run after the user reviews and confirms a plan preview" (or "none" for pip). Of the spec's five bullets: the standalone-tool sections, the `~/.claude/settings.json` / rc-file reads, the six-host list, the phase-4 lines of the never-list, the step-C FDA result, and the unknown-scan section are each added by the step that produces them (B, C, D, E, F), as the spec says ("本阶段每步各加自己那一节"). What this step's document already carries from those bullets: the "Banager 读的文件" section with `~/.cargo/.crates2.json` ✔ (the one file the spec names that exists today), "Banager 只连接这些主机" with the constant's three entries, "不跟随重定向", "请求里除 UA 外不带本机任何信息" ✔, and the never-list items that are true today (no shell, no `curl | sh`, no installer scripts, the three brew flags, nothing outside the app's own settings written, no rc-file edits, no background writes) ✔. The items "不删 `$HOME` 之外的文件 / 不永久删除任何文件（唯一的进程内文件系统写入就是「移到废纸篓」）" are phrased for the state after step C; today the true sentence is "never writes, moves or deletes a file other than its own settings.json", which is what the document says, and step C rewrites that line when the trasher exists.

**Gaps found and fixed while writing:**
1. The spec's §4.2 says the http caller is "唯一" the Ollama daemon; the code agrees (`grep` finds no other `http://` URL built outside tests), but the `real.rs` loopback tests also depend on the exemption — the plan says so in Task 2's rationale so nobody "tightens" it to loopback-only later and breaks them.
2. The current document's claim "Banager never passes `--ignore-dependencies` to `brew uninstall`" survives; the new document adds that cargo's `--force` is the only `--force` Banager passes to anything (`cargo.rs:327-329`), because a reader who greps the repo for `--force` will find it and the never-list must not look wrong.
3. The Tauri updater plugin (`src-tauri/src/lib.rs:26`) and its GitHub endpoint (`tauri.conf.json`) are outside `RealHttpClient` and so outside the allowlist. Nothing calls the plugin today; the document says exactly that rather than omitting the endpoint. Worth flagging to the author: when self-update ships, either the plugin's endpoint joins a documented exception or the allowlist idea is extended to it. The opener plugin (`lib.rs:25`, `opener:default` in `capabilities/default.json`) is in the same position — registered, permitted, never called from `src/` — and is exactly the kind of capability (open a URL or a path in another app) a trust file exists to disclose, so the document names it too.
4. Spec §8.5 says the Unknown page's "重新扫描" will be "全 app 第一个刷新控件"; the code has a Retry control after a failed refresh (`SnapshotStatus.tsx:23`) and a refresh from a source notice (`SourceNotices.tsx:25`). The document describes those as they are; the spec's sentence is about a *page-level* control and is not contradicted, but step F's author should read it as "first page-level rescan", not "first refresh control".

**Ambiguities in the spec for this step:**
- §4.2's `ALLOWED_HTTPS_HOSTS` block lists six hosts under one `// 阶段 3` / phase-4 split; read with §十's "每一步只带该步有生产者的变体与字段", the step-A list is three. Resolved as three (above).
- §6.5 names only `warningKey`; `warningArgs` has the identical defect for payload variants. Resolved by including it, with the reasoning in Task 3.
- §9.5 mixes "补齐六个阶段 3 来源" (this step) with bullets that only make sense after later steps. Resolved by writing only what is true at `26bc640` and stating which lines later steps rewrite.

## Review log

Adversarial review of this plan, 2026-09-24, each point re-verified against `26bc640` before the plan was changed. 17 points, 17 accepted, 0 rejected (two are duplicates of earlier points and one cites the wrong lines, noted below).

| # | Verdict | Reason (one line) and where the plan changed |
|---|---|---|
| 1 | accepted | `ops/mod.rs:645` `read.ok()` keeps `before` `None` on `IndexUpdating`, `version_change` (`:88-100`) then answers `Unknown`, and `:715-717` reports `Succeeded`; the current doc's `:56-62` exception was dropped by the rewrite. Restored: "When commands run" paragraph, a Homebrew-section paragraph after the `OP_UPDATE_WAIT` one, the never-list's last bullet, and an evidence row citing `:621-625`, `:645`, `:88-100`, `:704-706`, `:715-717`. |
| 2 | accepted | reqwest 0.13.5 `ClientBuilder::new` seeds `Accept: */*` (`async_impl/client.rs:285` in the local registry) and hyper adds `Host`; the adapters' empty `headers` prove only Banager's half. Network paragraph now says "no other header of Banager's own … the HTTP library adds `Host` and `Accept: */*`, and nothing else"; the evidence row is split into Banager's headers and the library's. |
| 3 | accepted | `real.rs:51` is the client default and `:75` overrides it per request; Ollama's detect passes 10 s (`ollama/mod.rs:279`). Network paragraph now says "a timeout per request (30 s unless stated; the daemon check in Ollama's detect is 10 s)"; the `rustls, UA, …` evidence row cites `:51`, `:75` and the five per-request values. |
| 4 | accepted | The planned Cargo rows carry `cargo install --force`, so `doc.contains("--force")` could not tell the Homebrew promise was gone. The test now requires one line naming all three flags, "never" and "Homebrew" (the never-list bullet); Interfaces and Step 2's expected failure were reworded to match. |
| 5 | accepted | `grep -c 'host_allowed('` over the Step 1 block is 10 (3 + 3 + 1 + 1 + 2), one E0425 each. Step 2 now says ten, one per call site. |
| 6 | accepted | `warnings.ts` exports five functions (`:19`, `:38`, `:58`, `:74`, `:80`). "four" → "five". |
| 7 | accepted | `lib.rs:25` registers `tauri_plugin_opener`, `capabilities/default.json` grants `opener:default`, `package.json:22` installs the JS half, and `grep -rn 'plugin-opener\|openUrl\|openPath' src` is empty (LogDrawer's `opener` is a local variable). The "outside that client" paragraph now names three things and describes the opener; an evidence row and self-review gap 3 were extended. |
| 8 | accepted | `ipc.rs:486-488` is `Stdio::null()` three times, a redirect to `/dev/null`, not a close. Reworded to the document's own phrase for package commands. |
| 9 | accepted | `HostEnv::discover` runs at `lib.rs:21` (launch), `ipc.rs:34` (refresh) and `ipc.rs:544` (Open Ollama), and reads `libc::geteuid()` at `path_env.rs:71`. "Where the program comes from" and its evidence row now say so. |
| 10 | accepted | `pipx.rs:358`, `uv.rs:231` and `npm.rs:320` each call `self.inventory(inst)` on the failure path. One clause added under each of the three tables (npm's in its existing prose), and the three evidence rows cite the lines. |
| 11 | accepted | Only Task 1 carried the "run `cargo fmt --all`" parenthetical; Task 2's `assert!` blocks and 100-column format string, and Task 4's long messages, are the shapes rustfmt rewraps. The parenthetical is now in Task 2 Step 5 and Task 4 Step 6. |
| 12 | accepted | `npm.rs:29` compares `trimmed.len()` (bytes) and `:338` passes `query.to_string()` untrimmed. The document now says "at most 200 bytes of UTF-8 … npm receives the query untrimmed"; the evidence row cites `:29` and `:338`. The code is not changed here, as the point allows. |
| 13 | accepted | `UninstallDialog.test.tsx` feeds `"SomeFutureVariant"` through the real `I18nextProvider` (`src/test/setup.ts:46-58`); with the `never` default returning the raw key, `t()` hands it back, `warningTexts` yields one item and the heading renders, so the old assertion fails. The test is at `:140-155`, not `:142-157` as the point cites (the `it(` opens at `:140`). Fixed: the file is in Task 3's Files, File Structure, Step 1 (a replacement test that asserts the heading and the raw key), Step 2 (it is now the runtime red), Step 4 and Step 6's `git add`; "three" such tests → three unit tests plus this one. |
| 14 | accepted | Duplicate of 2 + 3 with the same evidence; the one rewording of the Network paragraph and the split evidence rows cover it. |
| 15 | accepted | `en.json:93-100` and `zh-CN.json:87-93` each hold five `warnings.*` keys (with `wouldBreak` as plural forms), all five referenced in `warningKey`. "three" → "five", keys listed. |
| 16 | accepted | Duplicate of 5; the one fix in Task 2 Step 2 covers it. |
| 17 | accepted | `UninstallDialog.tsx:87` is `const hasAffected`, `:88-92` the comment, `:93` `warningTexts`. `:87-91` → `:88-92` in "What already exists", Task 3's Files list and Step 3. |
| — | found while verifying 4 | Simulating the four `what_we_run_test` assertions over the planned document text (a script over the fenced block, since cargo could not be run) showed `!doc.contains("Homebrew only")` failing on the never-list's own sentence "passed through to Homebrew only when it was already set". The assertion now compares the title line to `# What Banager Runs` exactly; Interfaces says so. The other three assertions pass over the planned text and the flag promise matches exactly one line (the never-list bullet); all four fail over the current file as Step 2 states. |

Remaining risk after this pass: none of the four tasks was run (another process held the tree), so Rust and TypeScript line numbers were read, not compiled, and the `what_we_run_test` assertions were simulated in a script rather than run under cargo. The dialog test's raw-key expectation pins today's runtime behaviour of the `never` default; if step C decides an unmirrored wire value should be rendered differently, that test changes with it.
