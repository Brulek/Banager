//! The login shell's `PATH`: an app opened from Finder starts with
//! macOS's four system folders, and nearly every tool lives elsewhere.
//! With it, the proxy and mirror settings Terminal has (`IMPORTED`, U12 of
//! the decisions round): exported only in `~/.zprofile`, they never reached
//! an app opened from Finder, and a `brew update` that works in Terminal
//! timed out in Banager.
//!
//! One read (`read`) runs the user's login shell with the command
//! fix-path-env ran, bounded by `TIMEOUT`; only `PATH` and `IMPORTED` are
//! taken from it, and only from a complete, successful run. It never blocks the window:
//! the Tauri shell starts the first read as the app starts, in the
//! background (`LoginPath::ensure`), and every refresh waits for that one
//! before it looks for sources -- the window says it is checking meanwhile
//! -- and reads again, once, when the last read failed or ran out of time
//! (Check Again, the next refresh).
//!
//! What a read found is kept here (`accept`), never put in the process
//! environment: setting a variable while other threads may read the
//! environment is unsound, and the window's threads do. Every command
//! Banager runs is handed it as its `PATH`, and the `IMPORTED` settings
//! it found as they were (`RealRunner::run`); `HostEnv::discover` and the
//! diagnostics read the `PATH` in place of the process's own (`path`),
//! and Banager's own requests go through the proxy it names, never for
//! this Mac itself (`command_var`, `http::proxy`). The diagnostics never
//! hold an imported value: a proxy's can hold a password. Nor does what a
//! command prints, as anyone sees it: tools print a setting back whole,
//! and the runner masks a login in it first (`redact`).
use super::{CommandRunner, CommandSpec, OutputUse};
use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

const DELIMITER: &str = "_SHELL_ENV_DELIMITER_";
const COMMAND: &str =
    "echo -n \"_SHELL_ENV_DELIMITER_\"; env; echo -n \"_SHELL_ENV_DELIMITER_\"; exit";

/// How long a login shell gets to start and print its environment.
///
/// fix-path-env, which ran the same command before, waited as long as the
/// shell took: a shell that never finished kept the window from opening at
/// all. The 3 seconds that replaced it cut off shells that do finish:
/// startup files that set up nvm, conda, pyenv or rbenv, or oh-my-zsh with
/// plugins, each run their own programs, commonly taking a second or more
/// apiece once warm, and several times that on the first launch after
/// login, when nothing is in the disk cache yet. Cut off, `PATH` stays
/// Finder's four folders and npm, pipx, uv and Cargo are not found. 15
/// seconds covers such a shell several times over, and is short enough to
/// give up on one that is stuck (waiting on a network mount, say). It
/// costs little now: the read runs in the background, so a slow shell
/// delays only the first check, which says it is checking, and never the
/// window. The runner's stop adds up to 5 seconds of grace to a shell
/// that has to be stopped (`RealRunner`).
pub const TIMEOUT: Duration = Duration::from_secs(15);

/// The settings besides `PATH` taken from the login shell (U12 of the
/// decisions round): which proxy a download goes through, and which
/// mirror it comes from -- nothing else. Not what decides where things are
/// installed (`CARGO_HOME`, `RUSTUP_HOME`, `UV_TOOL_DIR`, `PIPX_HOME`, an
/// npm or Homebrew prefix): that would change which folders an uninstall
/// and its preview act on (the round 4 review of D1), so those stay as
/// Banager was started with. No token, no Ollama address.
///
/// - The proxies, in both spellings: curl (and so Homebrew), git, pip and
///   uv read the lowercase ones first, Go programs such as `ollama` the
///   uppercase ones, and people set either.
/// - Homebrew's mirrors, as the Tsinghua and USTC mirror pages set them.
/// - pip's (which pipx runs) and npm's registry; npm reads its
///   `npm_config_` settings in either case, so both spellings.
/// - uv's and rustup's: not named in the decision, which says Homebrew,
///   pip and npm, but the same kind of setting for the two other sources
///   Banager updates over the network that read their mirror from a
///   variable (`uv tool upgrade`, `rustup update`); the mirror pages give
///   these names. Cargo reads its mirror from `~/.cargo/config.toml`,
///   which needs nothing imported.
///
/// Only settings that replace where a tool downloads from: not pip's or
/// uv's extra index (`PIP_EXTRA_INDEX_URL`, `UV_EXTRA_INDEX_URL`,
/// `UV_INDEX`), which is no mirror but a second index searched beside
/// PyPI, from which an update may take a higher version of a package of
/// the same name than the one Banager found on pypi.org (the U12 review).
///
/// `docs/what-we-run.md` names each (`tests/what_we_run_test.rs`).
pub const IMPORTED: &[&str] = &[
    "http_proxy",
    "HTTP_PROXY",
    "https_proxy",
    "HTTPS_PROXY",
    "all_proxy",
    "ALL_PROXY",
    "no_proxy",
    "NO_PROXY",
    "HOMEBREW_API_DOMAIN",
    "HOMEBREW_BOTTLE_DOMAIN",
    "HOMEBREW_BREW_GIT_REMOTE",
    "HOMEBREW_CORE_GIT_REMOTE",
    "HOMEBREW_PIP_INDEX_URL",
    "PIP_INDEX_URL",
    "npm_config_registry",
    "NPM_CONFIG_REGISTRY",
    "UV_INDEX_URL",
    "UV_DEFAULT_INDEX",
    "RUSTUP_DIST_SERVER",
    "RUSTUP_UPDATE_ROOT",
];

/// What one read of the login shell found (`read`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LoginEnv {
    /// Its `PATH`.
    pub path: String,
    /// Those of `IMPORTED` it set, as `(name, value)`, in `IMPORTED`'s
    /// order.
    pub imported: Vec<(String, String)>,
}

impl LoginEnv {
    /// The names of the settings imported, for a log line: never their
    /// values, since a proxy's can hold a password.
    pub fn imported_names(&self) -> Vec<&str> {
        self.imported
            .iter()
            .map(|(name, _)| name.as_str())
            .collect()
    }
}

/// What a read of the login shell found (`accept`), for the rest of this
/// run of Banager; `None` until one has.
static ACCEPTED: RwLock<Option<LoginEnv>> = RwLock::new(None);

/// Keeps `found`, what a read of the login shell found, for every command
/// Banager runs from now on (`accepted_env`). The process environment is
/// left as it is.
pub fn accept(found: &LoginEnv) {
    *ACCEPTED.write().unwrap_or_else(|e| e.into_inner()) = Some(found.clone());
}

/// What a read of the login shell found, once one has (`accept`).
pub fn accepted_env() -> Option<LoginEnv> {
    ACCEPTED.read().unwrap_or_else(|e| e.into_inner()).clone()
}

/// The login shell's `PATH`, once a read found it (`accept`).
pub fn accepted() -> Option<OsString> {
    ACCEPTED
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .map(|found| OsString::from(&found.path))
}

/// The value `name`, one of `IMPORTED`, has for a command Banager runs:
/// the login shell's, once a read found it set (`accept`), else the
/// process's own, which every command inherits. What Banager's own
/// requests take their proxy from (`http::proxy`), so they go the way its
/// commands do. `None` for a name that is not one of `IMPORTED`.
pub fn command_var(name: &str) -> Option<String> {
    if !IMPORTED.contains(&name) {
        return None;
    }
    let found = ACCEPTED
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .and_then(|found| {
            found
                .imported
                .iter()
                .find(|(imported, _)| imported == name)
                .map(|(_, value)| value.clone())
        });
    found.or_else(|| std::env::var(name).ok())
}

/// The `PATH` Banager looks for programs along and hands every command it
/// runs: the login shell's, once read, else the process's own.
pub fn path() -> Option<OsString> {
    accepted().or_else(|| std::env::var_os("PATH"))
}

/// What a refresh round looks along, and whether its `PATH` is the login
/// shell's, from one look at what was accepted (`accepted`), so the two can
/// never disagree: a read that works while an older round runs changes
/// neither for that round (`Session::refresh_recording_on`).
pub fn round_env() -> (super::HostEnv, bool) {
    let accepted = accepted();
    let known = accepted.is_some();
    (
        super::HostEnv::discover_along(accepted.or_else(|| std::env::var_os("PATH"))),
        known,
    )
}

/// What became of the reads so far.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Answer {
    NotYet,
    Read,
    Failed,
}

/// The login shell's `PATH`, read in the background once, and again
/// whenever it is asked for after a read that did not work (`ensure`).
pub struct LoginPath {
    runner: Arc<dyn CommandRunner>,
    shell: PathBuf,
    home: PathBuf,
    timeout: Duration,
    /// Where what a read found is put: `accept`, outside a test.
    publish: Box<dyn Fn(&LoginEnv) + Send + Sync>,
    /// Held for the length of a read, so two never run at once.
    answer: tokio::sync::Mutex<Answer>,
    /// How many reads have ended, however.
    ended: AtomicU64,
    /// Whether a read has worked: once it has, for good.
    read: AtomicBool,
    /// Held from a look at `read` to its being told (`tell`).
    told: std::sync::Mutex<()>,
}

impl LoginPath {
    /// Reads `shell`'s `PATH` and `IMPORTED` from `home`, within
    /// `timeout`, through `runner`; each read that works goes to `publish`.
    pub fn new(
        runner: Arc<dyn CommandRunner>,
        shell: PathBuf,
        home: PathBuf,
        timeout: Duration,
        publish: impl Fn(&LoginEnv) + Send + Sync + 'static,
    ) -> LoginPath {
        LoginPath {
            runner,
            shell,
            home,
            timeout,
            publish: Box::new(publish),
            answer: tokio::sync::Mutex::new(Answer::NotYet),
            ended: AtomicU64::new(0),
            read: AtomicBool::new(false),
            told: std::sync::Mutex::new(()),
        }
    }

    /// Whether a read has worked, without waiting for one under way: what
    /// the session is told (`Session::note_login_path`, through `tell`),
    /// which then never goes back to false because a caller that waited on
    /// a failed read reports after a later read worked.
    pub fn is_read(&self) -> bool {
        self.read.load(Ordering::SeqCst)
    }

    /// Hands `tell` whether a read has worked (`is_read`), looked at and
    /// told under one lock, so that callers look and tell one at a time.
    /// A read that has worked stays worked, so whoever tells last also
    /// looked last, and saw it if any read had: a caller that looked before
    /// another's read worked can no longer tell the session false after
    /// that one told it true (`AppState::read_login_path`; Astra's final
    /// review, F4). `tell` must not wait: it is a store
    /// (`Session::note_login_path`).
    pub fn tell(&self, tell: impl FnOnce(bool)) {
        let _one_at_a_time = self.told.lock().unwrap_or_else(|e| e.into_inner());
        tell(self.is_read());
    }

    /// Whether `PATH` is the login shell's: true at once when a read has
    /// worked. Otherwise reads it -- unless a read ended while this one
    /// waited for its turn, whose answer it gives: the read the app
    /// started as it opened answers the first refresh, which waits for it,
    /// and is not run twice. A read that failed or ran out of time is
    /// tried again by the next call that did not wait on it: Check Again,
    /// or the next refresh.
    pub async fn ensure(&self) -> bool {
        let before = self.ended.load(Ordering::SeqCst);
        let mut answer = self.answer.lock().await;
        match *answer {
            Answer::Read => return true,
            Answer::Failed if self.ended.load(Ordering::SeqCst) != before => return false,
            Answer::NotYet | Answer::Failed => {}
        }
        let found = read(
            self.runner.as_ref(),
            self.shell.clone(),
            self.home.clone(),
            self.timeout,
        )
        .await;
        *answer = match found {
            Some(found) => {
                (self.publish)(&found);
                self.read.store(true, Ordering::SeqCst);
                Answer::Read
            }
            None => Answer::Failed,
        };
        self.ended.fetch_add(1, Ordering::SeqCst);
        *answer == Answer::Read
    }
}

/// One read: `shell -ilc <COMMAND>` from `home`, with
/// `DISABLE_AUTO_UPDATE=true` for oh-my-zsh, within `timeout`. The `PATH`
/// it printed, and those of `IMPORTED` it set, only from a complete,
/// successful, framed run; `None` for a timeout, a failed spawn or exit,
/// output that is not whole, or no `PATH`.
///
/// `env` prints a value with a line break in it over several lines, so a
/// line can look like a setting when it is the rest of another one's
/// value. A setting of `IMPORTED` whose name starts more than one line is
/// therefore not taken: which one is real cannot be told. Nor is one with
/// no value, or with a control character in its value.
pub async fn read(
    runner: &dyn CommandRunner,
    shell: PathBuf,
    home: PathBuf,
    timeout: Duration,
) -> Option<LoginEnv> {
    let output = runner
        .run(
            CommandSpec {
                program: shell,
                args: vec!["-ilc".into(), COMMAND.into()],
                env: vec![("DISABLE_AUTO_UPDATE".into(), "true".into())],
                cwd: Some(home),
                timeout,
                output_use: OutputUse::Parsed,
            },
            None,
            CancellationToken::new(),
        )
        .await
        .ok()?;
    if output.exit_code != Some(0) || output.timed_out || output.cancelled {
        return None;
    }
    let (_, rest) = output.stdout.split_once(DELIMITER)?;
    let (environment, _) = rest.split_once(DELIMITER)?;
    let path = environment
        .lines()
        .find_map(|line| line.strip_prefix("PATH="))
        .filter(|path| !path.is_empty() && !path.chars().any(char::is_control))?;
    let imported = IMPORTED
        .iter()
        .filter_map(|name| {
            let mut values = environment
                .lines()
                .filter_map(|line| line.strip_prefix(name)?.strip_prefix('='));
            let value = values.next()?;
            if values.next().is_some() || value.is_empty() || value.chars().any(char::is_control) {
                return None;
            }
            Some((name.to_string(), value.to_string()))
        })
        .collect();
    Some(LoginEnv {
        path: path.to_owned(),
        imported,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runner::{CommandOutput, MockRunner, RealRunner};

    #[tokio::test]
    async fn test_only_complete_successful_shell_output_supplies_a_path() {
        for (stdout, timed_out, expected) in [
            (
                "noise_SHELL_ENV_DELIMITER_PATH=/a:/b\nOTHER=x\n_SHELL_ENV_DELIMITER_tail",
                false,
                Some("/a:/b"),
            ),
            (
                "_SHELL_ENV_DELIMITER_PATH=/late\n_SHELL_ENV_DELIMITER_",
                true,
                None,
            ),
            ("PATH=/unframed", false, None),
            ("_SHELL_ENV_DELIMITER_PATH=/partial", false, None),
            (
                "_SHELL_ENV_DELIMITER_PATH=\n_SHELL_ENV_DELIMITER_",
                false,
                None,
            ),
        ] {
            let runner = MockRunner::new();
            runner.respond(
                vec!["/test-shell", "-ilc", COMMAND],
                CommandOutput {
                    stderr_cause: Default::default(),
                    exit_code: Some(0),
                    stdout: stdout.into(),
                    stderr: String::new(),
                    timed_out,
                    cancelled: false,
                },
            );
            assert_eq!(
                read(
                    &runner,
                    "/test-shell".into(),
                    "/tmp".into(),
                    Duration::from_millis(20)
                )
                .await
                .map(|found| found.path)
                .as_deref(),
                expected
            );
        }
    }

    /// What `read` makes of `environment`, printed by a shell that
    /// finished.
    async fn read_of(environment: &str) -> Option<LoginEnv> {
        let runner = MockRunner::new();
        runner.respond(
            vec!["/test-shell", "-ilc", COMMAND],
            CommandOutput {
                stderr_cause: Default::default(),
                exit_code: Some(0),
                stdout: format!("{DELIMITER}{environment}{DELIMITER}"),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        read(
            &runner,
            "/test-shell".into(),
            "/tmp".into(),
            Duration::from_millis(20),
        )
        .await
    }

    fn pairs(of: &[(&str, &str)]) -> Vec<(String, String)> {
        of.iter()
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect()
    }

    /// U12 of the decisions round: a proxy and a mirror set in
    /// `~/.zprofile` reach what Banager runs; what decides which folders an
    /// uninstall acts on (`CARGO_HOME`, `RUSTUP_HOME`, `UV_TOOL_DIR` -- the
    /// round 4 review), a token, and everything else do not.
    #[tokio::test]
    async fn test_a_read_takes_the_proxy_and_mirror_settings_and_nothing_else() {
        let found = read_of(
            "PATH=/opt/homebrew/bin:/usr/bin\n\
             HOME=/Users/someone\n\
             CARGO_HOME=/Users/someone/.cargo-elsewhere\n\
             RUSTUP_HOME=/Users/someone/.rustup-elsewhere\n\
             UV_TOOL_DIR=/Users/someone/uv-tools\n\
             UV_TOOL_BIN_DIR=/Users/someone/uv-bin\n\
             PIPX_HOME=/Users/someone/pipx\n\
             NPM_CONFIG_PREFIX=/Users/someone/npm\n\
             HOMEBREW_PREFIX=/opt/elsewhere\n\
             HOMEBREW_GITHUB_API_TOKEN=ghp_secret\n\
             GITHUB_TOKEN=ghp_other\n\
             OLLAMA_HOST=0.0.0.0\n\
             RUSTUP_DIST_SERVER=https://mirrors.ustc.edu.cn/rust-static\n\
             https_proxy=http://127.0.0.1:7890\n\
             HTTPS_PROXY=http://127.0.0.1:7890\n\
             http_proxy_backup=http://elsewhere:1\n\
             all_proxy=socks5://127.0.0.1:7890\n\
             NO_PROXY=localhost,127.0.0.1,.internal\n\
             HOMEBREW_API_DOMAIN=https://mirrors.tuna.tsinghua.edu.cn/homebrew-bottles/api\n\
             HOMEBREW_BOTTLE_DOMAIN=https://mirrors.tuna.tsinghua.edu.cn/homebrew-bottles\n\
             HOMEBREW_BREW_GIT_REMOTE=https://mirrors.tuna.tsinghua.edu.cn/git/homebrew/brew.git\n\
             PIP_INDEX_URL=https://pypi.tuna.tsinghua.edu.cn/simple\n\
             PIP_EXTRA_INDEX_URL=https://pkgs.example.com/simple\n\
             npm_config_registry=https://registry.npmmirror.com\n\
             UV_DEFAULT_INDEX=https://mirrors.aliyun.com/pypi/simple\n\
             UV_EXTRA_INDEX_URL=https://pkgs.example.com/simple\n\
             UV_INDEX=extra=https://pkgs.example.com/simple\n",
        )
        .await
        .expect("a complete read");
        assert_eq!(found.path, "/opt/homebrew/bin:/usr/bin");
        assert_eq!(
            found.imported,
            pairs(&[
                ("https_proxy", "http://127.0.0.1:7890"),
                ("HTTPS_PROXY", "http://127.0.0.1:7890"),
                ("all_proxy", "socks5://127.0.0.1:7890"),
                ("NO_PROXY", "localhost,127.0.0.1,.internal"),
                (
                    "HOMEBREW_API_DOMAIN",
                    "https://mirrors.tuna.tsinghua.edu.cn/homebrew-bottles/api"
                ),
                (
                    "HOMEBREW_BOTTLE_DOMAIN",
                    "https://mirrors.tuna.tsinghua.edu.cn/homebrew-bottles"
                ),
                (
                    "HOMEBREW_BREW_GIT_REMOTE",
                    "https://mirrors.tuna.tsinghua.edu.cn/git/homebrew/brew.git"
                ),
                ("PIP_INDEX_URL", "https://pypi.tuna.tsinghua.edu.cn/simple"),
                ("npm_config_registry", "https://registry.npmmirror.com"),
                ("UV_DEFAULT_INDEX", "https://mirrors.aliyun.com/pypi/simple"),
                (
                    "RUSTUP_DIST_SERVER",
                    "https://mirrors.ustc.edu.cn/rust-static"
                ),
            ]),
            "in IMPORTED's order, and only those"
        );
        assert_eq!(
            found.imported_names(),
            [
                "https_proxy",
                "HTTPS_PROXY",
                "all_proxy",
                "NO_PROXY",
                "HOMEBREW_API_DOMAIN",
                "HOMEBREW_BOTTLE_DOMAIN",
                "HOMEBREW_BREW_GIT_REMOTE",
                "PIP_INDEX_URL",
                "npm_config_registry",
                "UV_DEFAULT_INDEX",
                "RUSTUP_DIST_SERVER",
            ]
        );
    }

    /// `env` prints a value with a line break in it over two lines, so a
    /// name printed twice may be the second half of another variable's
    /// value: neither is taken. Nor is a setting with no value, or one
    /// with a control character in it.
    #[tokio::test]
    async fn test_a_setting_printed_twice_empty_or_with_a_control_character_is_not_taken() {
        let found = read_of(
            "PATH=/usr/bin:/bin\n\
             NOTE=first line\n\
             https_proxy=http://from-inside-a-value:1\n\
             https_proxy=http://127.0.0.1:7890\n\
             http_proxy=\n\
             all_proxy=http://tab\there:1\n\
             PIP_INDEX_URL=https://pypi.tuna.tsinghua.edu.cn/simple\n",
        )
        .await
        .expect("a complete read");
        assert_eq!(
            found.imported,
            pairs(&[("PIP_INDEX_URL", "https://pypi.tuna.tsinghua.edu.cn/simple")])
        );
    }

    #[test]
    fn test_only_proxy_and_mirror_settings_are_imported() {
        assert_eq!(
            IMPORTED,
            [
                "http_proxy",
                "HTTP_PROXY",
                "https_proxy",
                "HTTPS_PROXY",
                "all_proxy",
                "ALL_PROXY",
                "no_proxy",
                "NO_PROXY",
                "HOMEBREW_API_DOMAIN",
                "HOMEBREW_BOTTLE_DOMAIN",
                "HOMEBREW_BREW_GIT_REMOTE",
                "HOMEBREW_CORE_GIT_REMOTE",
                "HOMEBREW_PIP_INDEX_URL",
                "PIP_INDEX_URL",
                "npm_config_registry",
                "NPM_CONFIG_REGISTRY",
                "UV_INDEX_URL",
                "UV_DEFAULT_INDEX",
                "RUSTUP_DIST_SERVER",
                "RUSTUP_UPDATE_ROOT",
            ]
        );
        // What decides where things are installed, and so which folders an
        // uninstall or its preview acts on, stays as Banager was started
        // with (the round 4 review of D1); so do `PATH`, which is read on
        // its own, and Ollama's address and models.
        for kept_out in [
            "PATH",
            "HOME",
            "CARGO_HOME",
            "RUSTUP_HOME",
            "RUSTUP_TOOLCHAIN",
            "UV_TOOL_DIR",
            "UV_TOOL_BIN_DIR",
            "UV_PYTHON_INSTALL_DIR",
            "PIPX_HOME",
            "PIPX_BIN_DIR",
            "NPM_CONFIG_PREFIX",
            "npm_config_prefix",
            "HOMEBREW_PREFIX",
            "HOMEBREW_CELLAR",
            "HOMEBREW_GITHUB_API_TOKEN",
            "OLLAMA_HOST",
            "OLLAMA_MODELS",
            // Not a mirror: a second index searched beside PyPI, from
            // which pip or uv may take a higher version of a package of
            // the same name than the one Banager found on pypi.org (the
            // U12 review).
            "PIP_EXTRA_INDEX_URL",
            "UV_EXTRA_INDEX_URL",
            "UV_INDEX",
        ] {
            assert!(!IMPORTED.contains(&kept_out), "{kept_out} is imported");
        }
    }

    /// A runner that answers the login shell's command with each output
    /// in turn, after `delay`, and records the timeout it was given.
    struct Turns {
        outputs: std::sync::Mutex<Vec<CommandOutput>>,
        delay: Duration,
        timeouts: std::sync::Mutex<Vec<Duration>>,
    }

    impl Turns {
        fn new(outputs: Vec<CommandOutput>, delay: Duration) -> Arc<Turns> {
            Arc::new(Turns {
                outputs: std::sync::Mutex::new(outputs),
                delay,
                timeouts: std::sync::Mutex::new(Vec::new()),
            })
        }

        fn reads(&self) -> usize {
            self.timeouts.lock().unwrap().len()
        }
    }

    #[async_trait::async_trait]
    impl CommandRunner for Turns {
        async fn run(
            &self,
            spec: CommandSpec,
            _on_line: Option<crate::runner::LineCallback>,
            _cancel: CancellationToken,
        ) -> Result<CommandOutput, crate::runner::RunnerError> {
            self.timeouts.lock().unwrap().push(spec.timeout);
            tokio::time::sleep(self.delay).await;
            let mut outputs = self.outputs.lock().unwrap();
            Ok(if outputs.len() > 1 {
                outputs.remove(0)
            } else {
                outputs[0].clone()
            })
        }
    }

    fn timed_out() -> CommandOutput {
        CommandOutput {
            stderr_cause: Default::default(),
            exit_code: None,
            stdout: String::new(),
            stderr: String::new(),
            timed_out: true,
            cancelled: false,
        }
    }

    fn printed(path: &str) -> CommandOutput {
        CommandOutput {
            stderr_cause: Default::default(),
            exit_code: Some(0),
            stdout: format!("{DELIMITER}PATH={path}\nHOME=/Users/someone\n{DELIMITER}"),
            stderr: String::new(),
            timed_out: false,
            cancelled: false,
        }
    }

    fn probe(runner: Arc<Turns>) -> (LoginPath, Arc<std::sync::Mutex<Vec<String>>>) {
        let published = Arc::new(std::sync::Mutex::new(Vec::new()));
        let to = published.clone();
        let probe = LoginPath::new(
            runner,
            "/test-shell".into(),
            "/tmp".into(),
            TIMEOUT,
            move |found: &LoginEnv| to.lock().unwrap().push(found.path.clone()),
        );
        (probe, published)
    }

    /// Astra's final review, F4: caller A looked before any read had
    /// worked; caller B's read then worked and B told the session so; A,
    /// resuming, told it what it had seen, and the diagnostics said
    /// Terminal's settings could not be read after a refresh that read
    /// them (`AppState::read_login_path`, `get_system_facts`). Whoever
    /// tells last must also have looked last.
    #[test]
    fn regression_a_caller_that_looked_before_a_read_worked_never_takes_the_success_back() {
        use std::sync::mpsc;
        let runner = Turns::new(vec![printed("/opt/homebrew/bin:/usr/bin")], Duration::ZERO);
        let (probe, _) = probe(runner);
        let probe = Arc::new(probe);
        // The session's flag, as `Session::note_login_path` keeps it.
        let session = Arc::new(AtomicBool::new(false));
        let (looked, a_looked) = mpsc::channel();
        let (told, b_told) = mpsc::channel();
        let a = std::thread::spawn({
            let (probe, session) = (probe.clone(), session.clone());
            move || {
                probe.tell(|read| {
                    looked.send(read).unwrap();
                    // B's read works now, and B tells the session -- if it
                    // can before A has told it what A saw.
                    let _ = b_told.recv_timeout(Duration::from_millis(500));
                    session.store(read, Ordering::SeqCst);
                })
            }
        });
        assert!(!a_looked.recv().unwrap(), "A looked before any read worked");
        let b = std::thread::spawn({
            let (probe, session) = (probe.clone(), session.clone());
            move || {
                let worked = tokio::runtime::Builder::new_current_thread()
                    .enable_time()
                    .build()
                    .unwrap()
                    .block_on(probe.ensure());
                assert!(worked);
                probe.tell(|read| session.store(read, Ordering::SeqCst));
                let _ = told.send(());
            }
        });
        a.join().unwrap();
        b.join().unwrap();
        assert!(probe.is_read());
        assert!(
            session.load(Ordering::SeqCst),
            "a read worked, and the session must be left saying so"
        );
    }

    #[test]
    fn test_the_login_shell_gets_long_enough_for_nvm_and_conda() {
        // Opus review finding 2: 3 seconds cut off shells that finish.
        assert!(TIMEOUT >= Duration::from_secs(10), "{TIMEOUT:?}");
        assert!(TIMEOUT <= Duration::from_secs(30), "{TIMEOUT:?}");
    }

    #[tokio::test]
    async fn test_a_read_that_timed_out_is_tried_again_and_one_that_worked_is_kept() {
        let runner = Turns::new(
            vec![timed_out(), printed("/opt/homebrew/bin:/usr/bin:/bin")],
            Duration::ZERO,
        );
        let (probe, published) = probe(runner.clone());
        // The first read runs out of time: no PATH, nothing published.
        assert!(!probe.ensure().await);
        assert!(!probe.is_read());
        assert!(published.lock().unwrap().is_empty());
        // Check Again: read again, and this time it works.
        assert!(probe.ensure().await);
        assert!(probe.is_read());
        assert_eq!(
            *published.lock().unwrap(),
            ["/opt/homebrew/bin:/usr/bin:/bin"]
        );
        // Every refresh after that: no shell run again.
        assert!(probe.ensure().await);
        assert!(probe.ensure().await);
        assert_eq!(runner.reads(), 2);
        assert_eq!(
            *runner.timeouts.lock().unwrap(),
            [TIMEOUT, TIMEOUT],
            "each read gets the whole timeout"
        );
    }

    #[tokio::test]
    async fn test_a_refresh_waiting_on_the_launch_read_takes_its_answer_and_runs_no_second_shell() {
        // The read the app starts as it opens has not ended when the first
        // refresh asks: that refresh waits for it, and takes its answer --
        // even a failure -- rather than starting the shell a second time.
        let runner = Turns::new(vec![timed_out()], Duration::from_millis(100));
        let (probe, _) = probe(runner.clone());
        let probe = Arc::new(probe);
        let launch = tokio::spawn({
            let probe = probe.clone();
            async move { probe.ensure().await }
        });
        tokio::time::sleep(Duration::from_millis(20)).await;
        let first_refresh = probe.ensure().await;
        assert!(!launch.await.unwrap());
        assert!(!first_refresh);
        assert_eq!(runner.reads(), 1);
        // A later refresh, not waiting on it: tries again.
        assert!(!probe.ensure().await);
        assert_eq!(runner.reads(), 2);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_a_hung_shell_is_killed_and_returns_without_a_path() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let shell = dir.path().join("shell");
        // An isolated synthetic shell, never the user's startup files.
        std::fs::write(&shell, "#!/bin/sh\nwhile :; do :; done\n").unwrap();
        std::fs::set_permissions(&shell, std::fs::Permissions::from_mode(0o700)).unwrap();
        let result = tokio::time::timeout(
            Duration::from_secs(5),
            read(
                &RealRunner::new(),
                shell,
                dir.path().into(),
                Duration::from_millis(30),
            ),
        )
        .await
        .expect("bounded startup subprocess");
        assert_eq!(result, None);
    }

    #[tokio::test]
    #[ignore = "bug: G04: two PATH lines pick the first one instead of keeping the inherited PATH"]
    async fn f08_g04_duplicate_path_lines_are_ambiguous_in_both_orders() {
        for (first, second) in [("/embedded", "/terminal"), ("/terminal", "/embedded")] {
            let found = read_of(&format!("MULTILINE=value\nPATH={first}\nPATH={second}\n")).await;
            assert!(
                found.is_none(),
                "ambiguous PATH must leave inherited discovery in place: {found:?}"
            );
            let runner = Arc::new(MockRunner::new());
            runner.respond(
                vec!["/test-shell", "-ilc", COMMAND],
                CommandOutput {
                    stderr_cause: Default::default(),
                    exit_code: Some(0),
                    stdout: format!("{DELIMITER}PATH={first}\nPATH={second}\n{DELIMITER}"),
                    stderr: String::new(),
                    timed_out: false,
                    cancelled: false,
                },
            );
            // An injected publisher models discovery's inherited fallback
            // without mutating the process-global PATH in parallel tests.
            let inherited = Arc::new(std::sync::Mutex::new("/inherited".to_string()));
            let published = inherited.clone();
            let discovery = LoginPath::new(
                runner,
                "/test-shell".into(),
                "/tmp".into(),
                Duration::from_millis(20),
                move |env| {
                    *published.lock().unwrap() = env.path.clone();
                },
            );
            assert!(!discovery.ensure().await);
            assert!(!discovery.is_read());
            assert_eq!(*inherited.lock().unwrap(), "/inherited");
        }
    }

    #[tokio::test]
    async fn f08_g04_single_path_is_still_accepted() {
        assert_eq!(
            read_of("OTHER=value\nPATH=/terminal:/usr/bin\n")
                .await
                .unwrap()
                .path,
            "/terminal:/usr/bin"
        );
    }
}
