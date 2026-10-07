//! A keg-only formula linked into its prefix, through its update (y1-keg,
//! r6): the preview, the check right before it runs, and the check and
//! `brew link --formula --force` after it. Each test lays a real prefix out
//! on disk as Homebrew 7.0.8 lays out `node@22` (`links::tests`), reads it
//! with the real `links::read_links`, and runs the plan's commands through
//! `FakeHomebrew`, which changes that disk by Homebrew's own rules for
//! `upgrade`, `link` and `cleanup` (`keg.rb:361-391`, `823-861`), written
//! here apart from the code under test.

use super::links;
use super::links::tests::{brew_link, make_keg, node_22_prefix, npm_updates_itself};
use super::*;
use crate::events::VecSink;
use crate::runner::{LineCallback, MockRunner, RunnerError};
use std::os::unix::fs::symlink;
use std::path::Component;

/// A Homebrew instance whose prefix is `prefix`.
fn instance(prefix: &Path) -> ManagerInstance {
    ManagerInstance {
        exe_path: prefix.join("bin/brew"),
        prefix: prefix.to_path_buf(),
        version: Some("7.0.8".to_string()),
        ..crate::testing::manager_instance("brew", &format!("brew:{}", prefix.display()))
    }
}

fn request(
    inst: &ManagerInstance,
    kind: OpKind,
    artifact_kind: ArtifactKind,
    name: &str,
) -> OpRequest {
    OpRequest {
        kind,
        instance_id: inst.id.clone(),
        artifact_kind,
        name: name.to_string(),
    }
}

fn upgrade(inst: &ManagerInstance, name: &str) -> OpRequest {
    request(inst, OpKind::Upgrade, ArtifactKind::Formula, name)
}

/// An adapter that reads `inst`'s prefix for real and knows `node@22` as
/// keg-only, as an inventory of the recorded `brew info` would.
fn adapter(runner: Arc<dyn CommandRunner>, inst: &ManagerInstance) -> BrewAdapter {
    BrewAdapter::new(runner)
        .with_links_fn(links::read_links)
        .with_keg_only(&inst.id, &["node@22"])
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| item.to_string()).collect()
}

/// The follow-ups of an update's plan; empty for a plan of one command.
fn follow_ups(plan: &Plan) -> Vec<Vec<String>> {
    match &plan.action {
        PlanAction::CommandThen { then, .. } => then.clone(),
        _ => Vec::new(),
    }
}

fn relinks(name: &str, commands: &[&str]) -> Warning {
    Warning::HomebrewRelinksAfterUpdate {
        name: name.to_string(),
        commands: strings(commands),
    }
}

/// A Cellar with node@22's 22.23.3 in it, and no pin: what the cleanup
/// after its update deletes (U9).
fn one_old_version(_prefix: &Path, name: &str) -> Option<Kegs> {
    (name == "node@22").then(|| Kegs {
        versions: vec!["22.23.3".to_string()],
        pinned: false,
    })
}

// -- The preview ----------------------------------------------------------

#[tokio::test]
async fn an_update_of_a_keg_only_formula_linked_with_brew_link_is_checked_after() {
    let prefix = node_22_prefix("keg-plan-brew-link");
    brew_link(&prefix, "22.23.3");
    let inst = instance(&prefix);
    let adapter = adapter(Arc::new(MockRunner::new()), &inst);
    let plan = adapter
        .plan(&inst, &upgrade(&inst, "node@22"))
        .await
        .expect("plan");
    assert_eq!(
        crate::testing::command_args(&plan),
        ["upgrade", "--formula", "node@22"]
    );
    assert_eq!(
        follow_ups(&plan),
        [strings(&["link", "--formula", "--force", "node@22"])]
    );
    assert_eq!(
        plan.warnings,
        [relinks("node@22", &["corepack", "node", "npm", "npx"])]
    );
    // With its cleanup after (U9): the link first, then the cleanup; the
    // link's line first.
    let adapter = adapter.with_kegs_fn(one_old_version);
    let plan = adapter
        .plan(&inst, &upgrade(&inst, "node@22"))
        .await
        .expect("plan");
    assert_eq!(
        follow_ups(&plan),
        [
            strings(&["link", "--formula", "--force", "node@22"]),
            strings(&["cleanup", "node@22"]),
        ]
    );
    assert_eq!(
        plan.warnings,
        [
            relinks("node@22", &["corepack", "node", "npm", "npx"]),
            Warning::HomebrewCleansUpOldVersions {
                versions: strings(&["22.23.3"]),
            },
        ]
    );
    std::fs::remove_dir_all(&prefix).unwrap();
}

#[tokio::test]
async fn an_update_of_a_keg_only_formula_linked_without_a_record_is_planned_as_before() {
    // No record: Homebrew's update unlinks and links nothing
    // (`upgrade.rb:268-272`, `640-643`). A link through `opt` follows it to
    // the new version; one straight into the keg keeps leading to the
    // version it replaces.
    let prefix = node_22_prefix("keg-plan-by-hand");
    for by_hand in [
        "../opt/node@22/bin/node",
        "../Cellar/node@22/22.23.3/bin/node",
    ] {
        let _ = std::fs::remove_file(prefix.join("bin/node"));
        symlink(by_hand, prefix.join("bin/node")).unwrap();
        let inst = instance(&prefix);
        let plan = adapter(Arc::new(MockRunner::new()), &inst)
            .plan(&inst, &upgrade(&inst, "node@22"))
            .await
            .expect("plan");
        assert!(
            matches!(plan.action, PlanAction::Command { .. }),
            "{by_hand}"
        );
        assert_eq!(plan.warnings, [], "{by_hand}");
    }
    std::fs::remove_dir_all(&prefix).unwrap();
}

#[tokio::test]
async fn a_link_through_opt_with_npms_own_npm_is_neither_refused_nor_linked_after() {
    // The author's Mac once `node` is put back by hand through `opt`,
    // with `bin/npm` and `bin/npx` still npm 12.2.0's own: no record, so
    // the update touches none of them, and `node` runs the new version.
    let prefix = node_22_prefix("keg-run-through-opt");
    symlink("../opt/node@22/bin/node", prefix.join("bin/node")).unwrap();
    npm_updates_itself(&prefix);
    let inst = instance(&prefix);
    let runner = Arc::new(FakeHomebrew::new(&prefix, Upgrade::AsHomebrew));
    let (adapter, plan) = planned(&inst, runner.clone()).await;
    assert!(matches!(plan.action, PlanAction::Command { .. }));
    assert_eq!(plan.warnings, []);
    let sink = Arc::new(VecSink::new());
    let outcome = adapter
        .execute(&plan, sink.clone(), 7, CancellationToken::new())
        .await
        .expect("execute");
    assert_eq!(outcome, Outcome::Succeeded);
    assert_eq!(
        runner.calls(),
        [strings(&["upgrade", "--formula", "node@22"])]
    );
    assert_eq!(notes(&sink), []);
    assert_eq!(
        std::fs::canonicalize(prefix.join("bin/node")).unwrap(),
        std::fs::canonicalize(prefix.join("Cellar/node@22/22.23.3_1/bin/node")).unwrap()
    );
    assert_eq!(
        std::fs::read_link(prefix.join("bin/npm")).unwrap(),
        Path::new("../lib/node_modules/npm/bin/npm-cli.js")
    );
    std::fs::remove_dir_all(&prefix).unwrap();
}

#[tokio::test]
async fn a_recorded_formula_with_a_link_through_opt_at_one_place_is_refused() {
    // Recorded, so the update links it again -- and stops at `bin/node`,
    // which its unlink left (`keg.rb:376-377`) and which is there
    // (`keg.rb:850-851`).
    let prefix = node_22_prefix("keg-plan-recorded-through-opt");
    brew_link(&prefix, "22.23.3");
    std::fs::remove_file(prefix.join("bin/node")).unwrap();
    symlink("../opt/node@22/bin/node", prefix.join("bin/node")).unwrap();
    let inst = instance(&prefix);
    let runner = Arc::new(FakeHomebrew::new(&prefix, Upgrade::AsHomebrew));
    let result = adapter(runner.clone(), &inst)
        .plan(&inst, &upgrade(&inst, "node@22"))
        .await;
    assert!(
        matches!(
            result,
            Err(AdapterError::UpdateBlocked {
                reason: UpdateBlocked::LinkTaken
            })
        ),
        "{result:?}"
    );
    // What Homebrew would do: unlink corepack, npm and npx, stop at
    // `bin/node`, and fail with them out of Terminal.
    assert_eq!(runner.upgrade(), 1);
    assert_eq!(runner.links().in_terminal_names(), ["node"]);
    std::fs::remove_dir_all(&prefix).unwrap();
}

#[tokio::test]
async fn an_update_links_nothing_back_of_a_keg_only_formula_nobody_linked() {
    let prefix = node_22_prefix("keg-plan-unlinked");
    let inst = instance(&prefix);
    let plan = adapter(Arc::new(MockRunner::new()), &inst)
        .plan(&inst, &upgrade(&inst, "node@22"))
        .await
        .expect("plan");
    assert!(matches!(plan.action, PlanAction::Command { .. }));
    assert_eq!(plan.warnings, []);
    std::fs::remove_dir_all(&prefix).unwrap();
}

#[tokio::test]
async fn an_update_of_a_formula_homebrew_links_itself_is_planned_as_before() {
    // `python@3.14` is not keg-only: Homebrew linked it at install and
    // links every new version itself. Linked, and with a record, it gets
    // nothing more -- nor does node@22 where no inventory said it is
    // keg-only, nor a cask, nor an install.
    let prefix = temp_dir_for("keg-plan-python");
    let keg = prefix.join("Cellar/python@3.14/3.14.8");
    std::fs::create_dir_all(keg.join("bin")).unwrap();
    std::fs::write(keg.join("bin/python3.14"), b"").unwrap();
    for folder in ["opt", "bin", "var/homebrew/linked"] {
        std::fs::create_dir_all(prefix.join(folder)).unwrap();
    }
    symlink(
        "../Cellar/python@3.14/3.14.8",
        prefix.join("opt/python@3.14"),
    )
    .unwrap();
    symlink(
        "../Cellar/python@3.14/3.14.8/bin/python3.14",
        prefix.join("bin/python3.14"),
    )
    .unwrap();
    symlink(
        "../../../Cellar/python@3.14/3.14.8",
        prefix.join("var/homebrew/linked/python@3.14"),
    )
    .unwrap();
    make_keg(&prefix, "22.23.3");
    symlink("../Cellar/node@22/22.23.3", prefix.join("opt/node@22")).unwrap();
    brew_link(&prefix, "22.23.3");
    let inst = instance(&prefix);
    let keg_only = adapter(Arc::new(MockRunner::new()), &inst);
    let nothing_known =
        BrewAdapter::new(Arc::new(MockRunner::new())).with_links_fn(links::read_links);
    for (adapter, req) in [
        (&keg_only, upgrade(&inst, "python@3.14")),
        (&nothing_known, upgrade(&inst, "node@22")),
        (
            &keg_only,
            request(&inst, OpKind::Upgrade, ArtifactKind::Cask, "node@22"),
        ),
        (
            &keg_only,
            request(&inst, OpKind::Install, ArtifactKind::Formula, "node@22"),
        ),
    ] {
        let plan = adapter.plan(&inst, &req).await.expect("plan");
        assert!(matches!(plan.action, PlanAction::Command { .. }), "{req:?}");
        assert_eq!(plan.warnings, [], "{req:?}");
    }
    std::fs::remove_dir_all(&prefix).unwrap();
}

#[tokio::test]
async fn an_inventory_tells_the_preview_which_formulae_are_keg_only_and_linkable() {
    // The recorded `brew info` lists node@22 (a versioned formula) and
    // sqlite, readline, icu4c@78 (macOS's own, which `brew link` refuses
    // at Homebrew's default prefix) as keg-only.
    let prefix = node_22_prefix("keg-plan-inventory");
    brew_link(&prefix, "22.23.3");
    let inst = instance(&prefix);
    let runner = Arc::new(MockRunner::new());
    let json = std::fs::read_to_string("../../adapters/fixtures/brew/7.0.3/info-installed.json")
        .expect("read the recorded brew info");
    let brew = prefix.join("bin/brew").display().to_string();
    runner.respond(
        vec![brew.as_str(), "info", "--installed", "--json=v2"],
        CommandOutput {
            stderr_cause: Default::default(),
            exit_code: Some(0),
            stdout: json,
            stderr: String::new(),
            timed_out: false,
            cancelled: false,
        },
    );
    let adapter = BrewAdapter::new(runner).with_links_fn(links::read_links);
    let plan = adapter
        .plan(&inst, &upgrade(&inst, "node@22"))
        .await
        .expect("plan");
    assert_eq!(
        follow_ups(&plan),
        Vec::<Vec<String>>::new(),
        "before an inventory"
    );
    adapter.inventory(&inst).await.expect("inventory");
    assert!(adapter.is_keg_only(&inst.id, "node@22"));
    assert!(adapter.is_keg_only(&inst.id, "someone/tap/node@22"));
    for name in ["sqlite", "readline", "icu4c@78", "python@3.14", "git"] {
        assert!(!adapter.is_keg_only(&inst.id, name), "{name}");
    }
    assert!(!adapter.is_keg_only("brew:/usr/local", "node@22"));
    let plan = adapter
        .plan(&inst, &upgrade(&inst, "node@22"))
        .await
        .expect("plan");
    assert_eq!(
        follow_ups(&plan),
        [strings(&["link", "--formula", "--force", "node@22"])]
    );
    std::fs::remove_dir_all(&prefix).unwrap();
}

#[tokio::test]
async fn an_update_is_refused_while_another_program_holds_a_commands_place() {
    // The author's Mac on 2026-10-07 once npm had updated itself: the
    // update would unlink node@22, and `bin/npm` stops any link back.
    let prefix = node_22_prefix("keg-plan-taken");
    brew_link(&prefix, "22.23.3");
    npm_updates_itself(&prefix);
    let inst = instance(&prefix);
    let result = adapter(Arc::new(MockRunner::new()), &inst)
        .plan(&inst, &upgrade(&inst, "node@22"))
        .await;
    assert!(
        matches!(
            result,
            Err(AdapterError::UpdateBlocked {
                reason: UpdateBlocked::LinkTaken
            })
        ),
        "{result:?}"
    );
    std::fs::remove_dir_all(&prefix).unwrap();
}

/// A fresh temporary folder (`read_file::tests::temp_dir`).
fn temp_dir_for(tag: &str) -> PathBuf {
    crate::adapters::read_file::tests::temp_dir(tag)
}

// -- The check -----------------------------------------------------------

#[tokio::test]
async fn a_check_marks_the_update_of_a_keg_only_formula_whose_place_is_taken() {
    let prefix = node_22_prefix("keg-check-taken");
    brew_link(&prefix, "22.23.3");
    let inst = instance(&prefix);
    let brew = prefix.join("bin/brew").display().to_string();
    let runner = Arc::new(MockRunner::new());
    let output = |stdout: &str| CommandOutput {
        stderr_cause: Default::default(),
        exit_code: Some(0),
        stdout: stdout.to_string(),
        stderr: String::new(),
        timed_out: false,
        cancelled: false,
    };
    runner.respond(vec![brew.as_str(), "update"], output(""));
    runner.respond(
        vec![brew.as_str(), "outdated", "--json=v2"],
        output(
            r#"{"formulae":[
                {"name":"node@22","installed_versions":["22.23.3"],"current_version":"22.23.4","pinned":false,"pinned_version":null},
                {"name":"git","installed_versions":["2.54.0"],"current_version":"2.55.0","pinned":false,"pinned_version":null}
            ],"casks":[]}"#,
        ),
    );
    runner.respond(
        vec![brew.as_str(), "info", "--installed", "--json=v2"],
        output(
            r#"{"formulae":[
                {"name":"node@22","keg_only":true,"keg_only_reason":{"reason":":versioned_formula"},"linked_keg":"22.23.3","installed":[{"version":"22.23.3","installed_on_request":true}]},
                {"name":"git","keg_only":false,"linked_keg":"2.54.0","installed":[{"version":"2.54.0","installed_on_request":true}]}
            ],"casks":[]}"#,
        ),
    );
    let adapter = BrewAdapter::new(runner)
        .with_links_fn(links::read_links)
        .with_update_ttl(Duration::from_secs(u64::MAX / 4));
    let blocked = |outcome: &CheckOutcome| -> Vec<(String, Option<UpdateBlocked>)> {
        outcome
            .candidates
            .iter()
            .map(|c| (c.key.name.clone(), c.blocked))
            .collect()
    };
    let options = CheckOptions::default();
    let outcome = adapter.check_updates(&inst, &options).await.expect("check");
    assert_eq!(
        blocked(&outcome),
        [("node@22".to_string(), None), ("git".to_string(), None)]
    );
    npm_updates_itself(&prefix);
    let outcome = adapter.check_updates(&inst, &options).await.expect("check");
    assert_eq!(
        blocked(&outcome),
        [
            ("node@22".to_string(), Some(UpdateBlocked::LinkTaken)),
            ("git".to_string(), None)
        ]
    );
    // What is in the way, for the row to name (y1-keg review).
    assert_eq!(
        outcome.candidates[0].warnings,
        [Warning::LinkPlacesHeld {
            name: "node@22".to_string(),
            paths: vec![
                prefix.join("bin/npm").display().to_string(),
                prefix.join("bin/npx").display().to_string(),
            ],
        }]
    );
    assert_eq!(outcome.candidates[1].warnings, []);
    // With no record and `node` linked through `opt`, `bin/npm` is no
    // concern of the update's: it links nothing.
    for command in ["corepack", "node"] {
        std::fs::remove_file(prefix.join("bin").join(command)).unwrap();
    }
    std::fs::remove_file(prefix.join("var/homebrew/linked/node@22")).unwrap();
    symlink("../opt/node@22/bin/node", prefix.join("bin/node")).unwrap();
    let outcome = adapter.check_updates(&inst, &options).await.expect("check");
    assert_eq!(
        blocked(&outcome),
        [("node@22".to_string(), None), ("git".to_string(), None)]
    );
    std::fs::remove_dir_all(&prefix).unwrap();
}

// -- The update ------------------------------------------------------------

/// How the `upgrade` of node@22 ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Upgrade {
    /// As Homebrew 7.0.8's: installs 22.23.3_1 and points `opt` at it;
    /// where the link is recorded (`Keg#linked?`), first unlinks the
    /// version it replaces (`install.rb:632-641`) and then links the new
    /// one (`upgrade.rb:640-643`), exiting 1 where that link stops at a
    /// place (`FormulaInstaller#link`, `formula_installer.rb:1304-1314`).
    /// Without the record it unlinks and links nothing.
    AsHomebrew,
    /// The same, but fails after unlinking, linking nothing: a conflict
    /// in a folder Banager does not read (`lib`, `share`).
    FailsAfterUnlinking,
    /// The same, but exits 0 without linking the new version back. No
    /// Homebrew 7.0.8 ends so -- its 0 says its link step succeeded -- and
    /// one that did is what Banager's check after the update is for.
    EndsUnlinked,
}

/// A `CommandRunner` that is Homebrew for node@22 under `prefix`: each
/// command changes the disk as Homebrew's would, and is recorded.
struct FakeHomebrew {
    prefix: PathBuf,
    upgrade: Upgrade,
    /// `brew link` exits 1 having linked nothing, as a conflict in a
    /// folder Banager does not read makes it.
    link_fails: bool,
    calls: Mutex<Vec<Vec<String>>>,
    /// Cancelled once the command named here has run: a Cancel landing
    /// between two commands of one operation.
    cancel_after: Option<(&'static str, CancellationToken)>,
}

/// `dst`'s one-level target: its link's text joined to its folder, with
/// `..` taking off the name before it and nothing followed
/// (`Utils::Path.resolved_path`, `utils/path.rb:84-85`). `None` for
/// anything but a link.
fn resolved_path(dst: &Path) -> Option<PathBuf> {
    let text = std::fs::read_link(dst).ok()?;
    let mut out = PathBuf::new();
    for component in dst.parent()?.join(text).components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    Some(out)
}

impl FakeHomebrew {
    fn new(prefix: &Path, upgrade: Upgrade) -> FakeHomebrew {
        FakeHomebrew {
            prefix: prefix.to_path_buf(),
            upgrade,
            link_fails: false,
            calls: Mutex::new(Vec::new()),
            cancel_after: None,
        }
    }

    fn calls(&self) -> Vec<Vec<String>> {
        self.calls.lock().unwrap().clone()
    }

    fn links(&self) -> links::KegLinks {
        links::read_links(&self.prefix, "node@22").expect("read")
    }

    fn record(&self) -> PathBuf {
        self.prefix.join("var/homebrew/linked/node@22")
    }

    /// The keg `opt` leads to, as Homebrew names it.
    fn opt_keg(&self) -> PathBuf {
        resolved_path(&self.prefix.join("opt/node@22")).unwrap()
    }

    /// `Keg#linked?` for that keg (`keg.rb:274-278`).
    fn recorded(&self) -> bool {
        let record = self.record();
        record.is_dir() && resolved_path(&record) == Some(self.opt_keg())
    }

    /// The names in `keg`'s `bin`, in order.
    fn commands(keg: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(keg.join("bin"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        names
    }

    /// `Keg#unlink` of `keg` (`keg.rb:361-391`, its `bin`): each link
    /// whose one-level target is that keg's file of the same name, and
    /// nothing else; then the record.
    fn unlink(&self, keg: &Path) {
        for name in Self::commands(keg) {
            let dst = self.prefix.join("bin").join(&name);
            if resolved_path(&dst) == Some(keg.join("bin").join(&name)) {
                std::fs::remove_file(&dst).unwrap();
            }
        }
        let _ = std::fs::remove_file(self.record());
    }

    /// `Keg#link` of the keg `opt` leads to (`keg.rb:498-590`, its `bin`),
    /// each command by `Keg#make_relative_symlink` (`keg.rb:823-861`):
    /// nothing to do where its own link is; a link made where nothing is,
    /// and where a link leads nowhere; anything else there a conflict,
    /// after which what it linked is unlinked again and nothing recorded.
    /// Already recorded: nothing (`cmd/link.rb:73-84`). Whether it linked.
    fn link(&self) -> bool {
        if self.recorded() {
            return true;
        }
        let keg = self.opt_keg();
        let version = keg.file_name().unwrap().to_str().unwrap().to_string();
        let mut made = Vec::new();
        for name in Self::commands(&keg) {
            let src = keg.join("bin").join(&name);
            let dst = self.prefix.join("bin").join(&name);
            if resolved_path(&dst) == Some(src) {
                continue;
            }
            if std::fs::symlink_metadata(&dst).is_ok() {
                if dst.exists() {
                    for dst in made {
                        std::fs::remove_file(dst).unwrap();
                    }
                    return false;
                }
                std::fs::remove_file(&dst).unwrap();
            }
            symlink(format!("../Cellar/node@22/{version}/bin/{name}"), &dst).unwrap();
            made.push(dst);
        }
        symlink(format!("../../../Cellar/node@22/{version}"), self.record()).unwrap();
        true
    }

    fn upgrade(&self) -> i32 {
        let recorded = self.recorded();
        let old = self.opt_keg();
        make_keg(&self.prefix, "22.23.3_1");
        if recorded {
            self.unlink(&old);
        }
        std::fs::remove_file(self.prefix.join("opt/node@22")).unwrap();
        symlink(
            "../Cellar/node@22/22.23.3_1",
            self.prefix.join("opt/node@22"),
        )
        .unwrap();
        match self.upgrade {
            Upgrade::FailsAfterUnlinking => 1,
            Upgrade::EndsUnlinked => 0,
            Upgrade::AsHomebrew if recorded && !self.link() => 1,
            Upgrade::AsHomebrew => 0,
        }
    }

    /// `brew cleanup node@22`: every version but the one `opt` leads to,
    /// and but a recorded one (`Formula#eligible_kegs_for_cleanup`,
    /// `formula.rb:3767-3793`).
    fn cleanup(&self) {
        let keep = std::fs::canonicalize(self.opt_keg()).unwrap();
        let recorded = resolved_path(&self.record())
            .filter(|keg| keg.is_dir())
            .map(|keg| std::fs::canonicalize(keg).unwrap());
        for entry in std::fs::read_dir(self.prefix.join("Cellar/node@22")).unwrap() {
            let keg = std::fs::canonicalize(entry.unwrap().path()).unwrap();
            if keg != keep && Some(&keg) != recorded.as_ref() {
                std::fs::remove_dir_all(keg).unwrap();
            }
        }
    }
}

#[async_trait]
impl CommandRunner for FakeHomebrew {
    async fn run(
        &self,
        spec: CommandSpec,
        _on_line: Option<LineCallback>,
        _cancel: CancellationToken,
    ) -> Result<CommandOutput, RunnerError> {
        self.calls.lock().unwrap().push(spec.args.clone());
        let exit_code = match spec.args.first().map(String::as_str) {
            Some("upgrade") => self.upgrade(),
            Some("link") if self.link_fails => 1,
            Some("link") => {
                if self.link() {
                    0
                } else {
                    1
                }
            }
            Some("cleanup") => {
                self.cleanup();
                0
            }
            _ => 1,
        };
        if let Some((after, token)) = &self.cancel_after {
            if spec.args.first().map(String::as_str) == Some(after) {
                token.cancel();
            }
        }
        Ok(CommandOutput {
            stderr_cause: Default::default(),
            exit_code: Some(exit_code),
            stdout: String::new(),
            stderr: if exit_code == 0 {
                String::new()
            } else {
                "Error: Could not symlink bin/npm".to_string()
            },
            timed_out: false,
            cancelled: false,
        })
    }
}

/// The notes `sink` got, in order.
fn notes(sink: &VecSink) -> Vec<LogNote> {
    sink.snapshot()
        .into_iter()
        .filter_map(|event| match event {
            OperationEvent::Note { note, .. } => Some(note),
            _ => None,
        })
        .collect()
}

/// `inst`'s update of node@22, planned with `runner`.
async fn planned(inst: &ManagerInstance, runner: Arc<FakeHomebrew>) -> (BrewAdapter, Plan) {
    let adapter = adapter(runner, inst);
    let plan = adapter
        .plan(inst, &upgrade(inst, "node@22"))
        .await
        .expect("plan");
    (adapter, plan)
}

const ALL: [&str; 4] = ["corepack", "node", "npm", "npx"];

#[tokio::test]
async fn homebrew_links_back_what_brew_link_linked_so_no_link_runs_after() {
    let prefix = node_22_prefix("keg-run-brew-link");
    brew_link(&prefix, "22.23.3");
    let inst = instance(&prefix);
    let runner = Arc::new(FakeHomebrew::new(&prefix, Upgrade::AsHomebrew));
    let (adapter, plan) = planned(&inst, runner.clone()).await;
    let sink = Arc::new(VecSink::new());
    let outcome = adapter
        .execute(&plan, sink.clone(), 7, CancellationToken::new())
        .await
        .expect("execute");
    assert_eq!(outcome, Outcome::Succeeded);
    assert_eq!(
        runner.calls(),
        [strings(&["upgrade", "--formula", "node@22"])]
    );
    assert_eq!(
        notes(&sink),
        [LogNote::StillLinkedAfterUpdate {
            name: "node@22".to_string()
        }]
    );
    assert!(runner.links().fully_linked());
    assert_eq!(
        std::fs::canonicalize(prefix.join("bin/node")).unwrap(),
        std::fs::canonicalize(prefix.join("Cellar/node@22/22.23.3_1/bin/node")).unwrap()
    );
    std::fs::remove_dir_all(&prefix).unwrap();
}

#[tokio::test]
async fn a_formula_its_update_did_not_link_back_is_linked_after_it() {
    let prefix = node_22_prefix("keg-run-not-back");
    brew_link(&prefix, "22.23.3");
    let inst = instance(&prefix);
    let runner = Arc::new(FakeHomebrew::new(&prefix, Upgrade::EndsUnlinked));
    let (adapter, plan) = planned(&inst, runner.clone()).await;
    let sink = Arc::new(VecSink::new());
    let outcome = adapter
        .execute(&plan, sink.clone(), 7, CancellationToken::new())
        .await
        .expect("execute");
    assert_eq!(outcome, Outcome::Succeeded);
    assert_eq!(
        runner.calls(),
        [
            strings(&["upgrade", "--formula", "node@22"]),
            strings(&["link", "--formula", "--force", "node@22"]),
        ]
    );
    assert_eq!(
        notes(&sink),
        [LogNote::RelinkingAfterUpdate {
            name: "node@22".to_string()
        }]
    );
    // Back in Terminal, leading into the new version.
    assert!(runner.links().fully_linked());
    assert_eq!(
        std::fs::canonicalize(prefix.join("bin/node")).unwrap(),
        std::fs::canonicalize(prefix.join("Cellar/node@22/22.23.3_1/bin/node")).unwrap()
    );
    std::fs::remove_dir_all(&prefix).unwrap();
}

#[tokio::test]
async fn a_link_that_fails_after_the_update_leaves_it_done_and_says_what_is_gone() {
    let prefix = node_22_prefix("keg-run-link-fails");
    brew_link(&prefix, "22.23.3");
    let inst = instance(&prefix);
    let runner = Arc::new(FakeHomebrew {
        link_fails: true,
        ..FakeHomebrew::new(&prefix, Upgrade::EndsUnlinked)
    });
    let (adapter, plan) = planned(&inst, runner.clone()).await;
    let sink = Arc::new(VecSink::new());
    let outcome = adapter
        .execute(&plan, sink.clone(), 7, CancellationToken::new())
        .await
        .expect("execute");
    // The update is done.
    assert_eq!(outcome, Outcome::Succeeded);
    assert_eq!(
        notes(&sink),
        [
            LogNote::RelinkingAfterUpdate {
                name: "node@22".to_string()
            },
            LogNote::NoLongerLinked {
                name: "node@22".to_string(),
                commands: strings(&ALL),
            },
        ]
    );
    std::fs::remove_dir_all(&prefix).unwrap();
}

#[tokio::test]
async fn an_update_runs_nothing_once_another_program_took_a_commands_place_since_the_preview() {
    // 2026-10-07: the preview of node@22's update came first; npm's own
    // update ran before node@22's turn and took `bin/npm` and `bin/npx`.
    let prefix = node_22_prefix("keg-run-taken");
    brew_link(&prefix, "22.23.3");
    let inst = instance(&prefix);
    let runner = Arc::new(FakeHomebrew::new(&prefix, Upgrade::AsHomebrew));
    let (adapter, plan) = planned(&inst, runner.clone()).await;
    npm_updates_itself(&prefix);
    let sink = Arc::new(VecSink::new());
    let outcome = adapter
        .execute(&plan, sink.clone(), 7, CancellationToken::new())
        .await
        .expect("execute");
    assert_eq!(
        outcome,
        Outcome::BanagerFailed(Fault::LinkTaken {
            name: "node@22".to_string(),
            paths: vec![
                prefix.join("bin/npm").display().to_string(),
                prefix.join("bin/npx").display().to_string(),
            ],
        })
    );
    assert!(runner.calls().is_empty(), "nothing ran");
    // `node` is where it was: in Terminal.
    assert_eq!(runner.links().linked_names(), ["corepack", "node"]);
    // What would have happened without the check: Homebrew unlinks
    // corepack and node, stops at `bin/npm` to link the new version,
    // unlinks what it linked, and fails -- `node` gone from Terminal.
    assert_eq!(runner.upgrade(), 1);
    assert_eq!(runner.links().in_terminal_names(), Vec::<String>::new());
    std::fs::remove_dir_all(&prefix).unwrap();
}

#[tokio::test]
async fn a_cancel_after_the_update_runs_no_link_and_says_what_is_gone() {
    let prefix = node_22_prefix("keg-run-cancel");
    brew_link(&prefix, "22.23.3");
    let inst = instance(&prefix);
    let token = CancellationToken::new();
    let runner = Arc::new(FakeHomebrew {
        cancel_after: Some(("upgrade", token.clone())),
        ..FakeHomebrew::new(&prefix, Upgrade::EndsUnlinked)
    });
    let (adapter, plan) = planned(&inst, runner.clone()).await;
    let sink = Arc::new(VecSink::new());
    let outcome = adapter
        .execute(&plan, sink.clone(), 7, token)
        .await
        .expect("execute");
    assert_eq!(outcome, Outcome::Succeeded);
    assert_eq!(
        runner.calls(),
        [strings(&["upgrade", "--formula", "node@22"])]
    );
    assert_eq!(
        notes(&sink),
        [LogNote::NoLongerLinked {
            name: "node@22".to_string(),
            commands: strings(&ALL),
        }]
    );
    std::fs::remove_dir_all(&prefix).unwrap();
}

#[tokio::test]
async fn an_update_that_fails_after_unlinking_says_what_is_gone() {
    let prefix = node_22_prefix("keg-run-fails");
    brew_link(&prefix, "22.23.3");
    let inst = instance(&prefix);
    let runner = Arc::new(FakeHomebrew::new(&prefix, Upgrade::FailsAfterUnlinking));
    let (adapter, plan) = planned(&inst, runner.clone()).await;
    let sink = Arc::new(VecSink::new());
    let outcome = adapter
        .execute(&plan, sink.clone(), 7, CancellationToken::new())
        .await
        .expect("execute");
    assert!(matches!(outcome, Outcome::Failed { .. }), "{outcome:?}");
    assert_eq!(
        runner.calls(),
        [strings(&["upgrade", "--formula", "node@22"])]
    );
    assert_eq!(
        notes(&sink),
        [LogNote::NoLongerLinked {
            name: "node@22".to_string(),
            commands: strings(&ALL),
        }]
    );
    std::fs::remove_dir_all(&prefix).unwrap();
}

#[tokio::test]
async fn the_link_runs_before_the_cleanup() {
    let prefix = node_22_prefix("keg-run-cleanup");
    brew_link(&prefix, "22.23.3");
    let inst = instance(&prefix);
    let runner = Arc::new(FakeHomebrew::new(&prefix, Upgrade::EndsUnlinked));
    let adapter = adapter(runner.clone(), &inst).with_kegs_fn(one_old_version);
    let plan = adapter
        .plan(&inst, &upgrade(&inst, "node@22"))
        .await
        .expect("plan");
    let sink = Arc::new(VecSink::new());
    let outcome = adapter
        .execute(&plan, sink.clone(), 7, CancellationToken::new())
        .await
        .expect("execute");
    assert_eq!(outcome, Outcome::Succeeded);
    assert_eq!(
        runner.calls(),
        [
            strings(&["upgrade", "--formula", "node@22"]),
            strings(&["link", "--formula", "--force", "node@22"]),
            strings(&["cleanup", "node@22"]),
        ]
    );
    assert_eq!(
        notes(&sink)[..2],
        [
            LogNote::RelinkingAfterUpdate {
                name: "node@22".to_string()
            },
            LogNote::CleaningUpOldVersions {
                name: "node@22".to_string()
            },
        ]
    );
    // The old version is gone, and every command leads into the new one.
    assert!(!prefix.join("Cellar/node@22/22.23.3").exists());
    assert!(runner.links().fully_linked());
    std::fs::remove_dir_all(&prefix).unwrap();
}

// -- The link a source's notice offers (y2-npmwhy) ------------------------
//
// The same `brew link --formula --force`, read the same way: what the
// preview says Homebrew would stop at is what `FakeHomebrew`, by
// `keg.rb:823-861`, stops at; and "linked" after it is what it is after
// an update's link (`KegLinks::fully_linked`).

fn link(inst: &ManagerInstance, name: &str) -> OpRequest {
    request(inst, OpKind::Link, ArtifactKind::Formula, name)
}

fn formula_key(inst: &ManagerInstance, name: &str) -> ArtifactKey {
    ArtifactKey {
        instance_id: inst.id.clone(),
        kind: ArtifactKind::Formula,
        name: name.to_string(),
    }
}

#[tokio::test]
async fn the_link_a_notice_offers_is_the_link_after_an_update_and_reads_linked_after_it() {
    // node@22 installed and not linked: nothing of it in `bin`.
    let prefix = node_22_prefix("keg-fix-free");
    let inst = instance(&prefix);
    let runner = Arc::new(FakeHomebrew::new(&prefix, Upgrade::AsHomebrew));
    let adapter = adapter(runner.clone(), &inst);
    let plan = adapter
        .plan(&inst, &link(&inst, "node@22"))
        .await
        .expect("plan");
    assert_eq!(
        crate::testing::command_args(&plan),
        ["link", "--formula", "--force", "node@22"]
    );
    assert_eq!(plan.timeout_secs, BrewAdapter::LINK_TIMEOUT_SECS);
    assert_eq!(
        plan.warnings,
        [Warning::LinkPutsCommands {
            names: strings(&ALL),
        }]
    );
    assert_eq!(
        adapter
            .reconcile_link(&inst, &formula_key(&inst, "node@22"))
            .await
            .unwrap(),
        Some(false),
        "not linked before it"
    );
    let sink = Arc::new(VecSink::new());
    let outcome = adapter
        .execute(&plan, sink.clone(), 7, CancellationToken::new())
        .await
        .expect("execute");
    assert_eq!(outcome, Outcome::Succeeded);
    assert_eq!(
        runner.calls(),
        [strings(&["link", "--formula", "--force", "node@22"])]
    );
    assert_eq!(
        adapter
            .reconcile_link(&inst, &formula_key(&inst, "node@22"))
            .await
            .unwrap(),
        Some(true)
    );
    assert!(runner.links().fully_linked());
    std::fs::remove_dir_all(&prefix).unwrap();
}

#[tokio::test]
async fn the_link_names_every_place_homebrew_would_stop_at_and_homebrew_stops_there() {
    // The author's Mac after 2026-10-07, with a link of their own back:
    // npm's own `npm` and `npx` in `bin`, and `bin/node` through `opt`.
    // Each is in the way of `brew link`, as of an update's link back
    // (`KegLinks::held_paths`, y1-keg's `LinkTaken`).
    let prefix = node_22_prefix("keg-fix-held");
    npm_updates_itself(&prefix);
    symlink("../opt/node@22/bin/node", prefix.join("bin/node")).unwrap();
    let inst = instance(&prefix);
    let runner = Arc::new(FakeHomebrew::new(&prefix, Upgrade::AsHomebrew));
    let adapter = adapter(runner.clone(), &inst);
    let plan = adapter
        .plan(&inst, &link(&inst, "node@22"))
        .await
        .expect("still planned, with what is in the way");
    let held =
        ["bin/node", "bin/npm", "bin/npx"].map(|path| prefix.join(path).display().to_string());
    assert_eq!(
        plan.warnings,
        [
            Warning::LinkPutsCommands {
                names: strings(&ALL),
            },
            Warning::LinkConflicts {
                paths: held.to_vec(),
            },
        ]
    );
    // `Session::submit` refuses it (`SubmitError::LinkBlocked`). Run all
    // the same, Homebrew links nothing: it took back `corepack`, and left
    // no record.
    let outcome = adapter
        .execute(&plan, Arc::new(VecSink::new()), 7, CancellationToken::new())
        .await
        .expect("execute");
    assert!(matches!(outcome, Outcome::Failed { .. }), "{outcome:?}");
    assert!(std::fs::symlink_metadata(prefix.join("bin/corepack")).is_err());
    assert_eq!(
        adapter
            .reconcile_link(&inst, &formula_key(&inst, "node@22"))
            .await
            .unwrap(),
        Some(false)
    );
    std::fs::remove_dir_all(&prefix).unwrap();
}

#[tokio::test]
async fn a_link_is_planned_only_for_a_keg_only_formula_brew_link_links() {
    // y1-keg's rule, for the link too: the recorded `brew info` lists
    // sqlite, readline and icu4c@78 as keg-only because of macOS, which
    // `brew link` refuses at Homebrew's default prefix, exiting 0 having
    // linked nothing; python@3.14 and git are not keg-only. Before an
    // inventory nothing is known, and nothing is linked.
    let prefix = node_22_prefix("keg-fix-inventory");
    let inst = instance(&prefix);
    let runner = Arc::new(MockRunner::new());
    let json = std::fs::read_to_string("../../adapters/fixtures/brew/7.0.3/info-installed.json")
        .expect("read the recorded brew info");
    let brew = prefix.join("bin/brew").display().to_string();
    runner.respond(
        vec![brew.as_str(), "info", "--installed", "--json=v2"],
        CommandOutput {
            stderr_cause: Default::default(),
            exit_code: Some(0),
            stdout: json,
            stderr: String::new(),
            timed_out: false,
            cancelled: false,
        },
    );
    let adapter = BrewAdapter::new(runner).with_links_fn(links::read_links);
    assert!(matches!(
        adapter.plan(&inst, &link(&inst, "node@22")).await,
        Err(AdapterError::Unsupported(_))
    ));
    adapter.inventory(&inst).await.expect("inventory");
    adapter
        .plan(&inst, &link(&inst, "node@22"))
        .await
        .expect("node@22 is linked");
    for name in ["sqlite", "readline", "icu4c@78", "python@3.14", "git"] {
        assert!(
            matches!(
                adapter.plan(&inst, &link(&inst, name)).await,
                Err(AdapterError::Unsupported(_))
            ),
            "{name}"
        );
    }
    std::fs::remove_dir_all(&prefix).unwrap();
}
