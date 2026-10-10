import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import { OverviewPage } from "./OverviewPage";
import { SnapshotStatus } from "../components/SnapshotStatus";
import i18n from "../i18n";
import { useUiStore } from "../store/ui";
import type {
  ArtifactKey,
  CommandState,
  InstalledArtifact,
  ManagerInstance,
  Settings,
  Snapshot,
  UpdateCandidate,
} from "../lib/types";
import { NO_FACTS } from "../lib/types";

/**
 * I22 (decisions round, 2026-10-06): the Overview reaches a plain 「都好了」.
 * Nothing to install and every source checked this time: the green check,
 * and 「能在这里更新的都已是最新」 where the Updates page lists something
 * besides -- hidden, can't be updated here, a copy Terminal does not run --
 * said in a quiet line under it. A source not checked this time is named:
 * 「uv这次没检查，其余都是最新的」.
 */

const mockInvoke = vi.mocked(invoke);

function instance(id: string, adapterId: string, over: Partial<ManagerInstance> = {}): ManagerInstance {
  return {
    id,
    adapter_id: adapterId,
    exe_path: `/opt/${adapterId}/bin/${adapterId}`,
    prefix: `/opt/${adapterId}`,
    scope: "User",
    version: "1.0.0",
    status: { unavailable: null, notes: [] },
    answered_at: null,
    unverified_version: null,
    read_only_reason: null,
    ...over,
  };
}

const brew = instance("brew:/opt/homebrew", "brew");
const pip = instance("pip:/usr/bin/python3", "pip", { read_only_reason: "ByDesign" });
const uv = instance("uv:/Users/you/.local/share/uv", "uv");
const npm = instance("npm:/opt/homebrew", "npm");
const codexOwn = instance("standalone-codex", "standalone-codex");
const stoppedOllama = instance("ollama:http://127.0.0.1:11434", "ollama", {
  status: { unavailable: "NotRunning", notes: [] },
});
const stoppedUv: ManagerInstance = { ...uv, status: { unavailable: "NotResponding", notes: [] } };

function key(source: ManagerInstance, name: string, kind: ArtifactKey["kind"] = "Formula"): ArtifactKey {
  return { instance_id: source.id, kind, name };
}

function artifact(artifactKey: ArtifactKey, family: string | null = null, state: CommandState | null = null): InstalledArtifact {
  return {
    key: artifactKey,
    display_name: artifactKey.name,
    version: "1.0.0",
    reason: "Requested",
    description: null,
    homepage: null,
    size_bytes: null,
    installed_at: null,
    path: null,
    auto_updates: false,
    uninstall_blocked: null,
    facts: family === null ? NO_FACTS : { ...NO_FACTS, family, commands: [{ name: family, state }] },
  };
}

function candidate(artifactKey: ArtifactKey, over: Partial<UpdateCandidate> = {}): UpdateCandidate {
  return {
    key: artifactKey,
    current: "1.0.0",
    target: "1.1.0",
    channel: "Native",
    checkable: true,
    warnings: [],
    blocked: null,
    ...over,
  };
}

const glib = key(brew, "glib");
const jq = key(brew, "jq");
const urllib3 = key(pip, "urllib3", "Package");
const ownCodex = key(codexOwn, "codex", "Binary");
const npmCodex = key(npm, "@openai/codex", "Package");

let served: Snapshot;
let settings: Settings;

function snapshotWith(over: Partial<Snapshot> = {}): Snapshot {
  return {
    generation: 7,
    round: 7,
    detect: "Found",
    instances: [brew, pip],
    artifacts: [artifact(glib), artifact(jq), artifact(urllib3)],
    updates: [],
    refreshed_at: 1790586000,
    stale: false,
    errors: [],
    ...over,
  };
}

beforeEach(() => {
  served = snapshotWith();
  settings = {
    language: "System",
    show_technical_details: false,
    ignored_updates: [],
    skipped_versions: [],
    include_self_updating: false,
    auto_check: false,
    notify_updates: false,
  };
  mockInvoke.mockReset();
  mockInvoke.mockImplementation((cmd: string) => {
    if (cmd === "get_snapshot") return Promise.resolve(served);
    if (cmd === "get_settings") return Promise.resolve(settings);
    if (cmd === "list_operations") return Promise.resolve([]);
    return Promise.resolve(undefined);
  });
});

afterEach(() => {
  void i18n.changeLanguage("en");
});

function renderOverview() {
  return renderWithProviders(
    <SnapshotStatus showsFirstCheck showsNothingFound>
      <OverviewPage />
    </SnapshotStatus>,
  );
}

function statusRow(container: HTMLElement): HTMLElement {
  return container.querySelector<HTMLElement>("[data-status]")!;
}

/** The status row's one button. */
function buttonOf(container: HTMLElement): HTMLElement {
  const buttons = within(statusRow(container))
    .getAllByRole("button")
    .filter((element) => element.closest("h2, [data-status-line]") === null);
  expect(buttons).toHaveLength(1);
  return buttons[0];
}

async function headlineIn(language: string, title: string) {
  await i18n.changeLanguage(language);
  return screen.findByRole("heading", { level: 2, name: title });
}

describe("the Overview's all good", () => {
  it("says what can be updated here is up to date, with the green check, over what is listed besides", async () => {
    served = snapshotWith({
      updates: [candidate(jq, { blocked: "Pinned" }), candidate(urllib3), candidate(glib)],
    });
    settings.ignored_updates = [glib];
    const { container } = renderOverview();

    const headline = await screen.findByRole("heading", {
      level: 2,
      name: "Everything you can update here is up to date",
    });
    expect(statusRow(container).getAttribute("data-status")).toBe("upToDate");
    expect(headline.nextElementSibling?.textContent).toBe("1 hidden, 2 can't be updated here");
    // The Updates page lists rows, each under "Can't update here": a page to open, nothing to select.
    const button = buttonOf(container);
    expect(button).toHaveAccessibleName("Review Updates");
    expect(button.className).toContain("bg-fill");
    fireEvent.click(button);
    expect(useUiStore.getState().page).toBe("updates");
    expect(useUiStore.getState().selectedUpdates).toEqual([]);

    const zh = await headlineIn("zh-CN", "能在这里更新的都已是最新");
    expect(zh.nextElementSibling?.textContent).toBe("1个已隐藏，2个无法在这里更新");
    await headlineIn("zh-Hant", "能在這裡更新的都已是最新");
  });

  it("is all good beside Codex's own install, whose updates Banager never checks", async () => {
    served = snapshotWith({
      instances: [brew, pip, codexOwn],
      artifacts: [artifact(glib), artifact(ownCodex)],
    });
    const { container } = renderOverview();
    const headline = await screen.findByRole("heading", {
      level: 2,
      name: "Everything you can update here is up to date",
    });
    expect(statusRow(container).getAttribute("data-status")).toBe("upToDate");
    // Nothing else to say: when the sources were last checked.
    expect(headline.nextElementSibling?.textContent).toMatch(/^Checked /);
    expect(buttonOf(container)).toHaveAccessibleName("Check Again");
  });

  it("says the update of a copy Terminal does not run under the green check, and opens the Updates page to it", async () => {
    served = snapshotWith({
      instances: [brew, pip, npm, codexOwn],
      artifacts: [
        artifact(glib),
        artifact(ownCodex, "codex", "Runs"),
        artifact(npmCodex, "codex", { ShadowedBy: { by: ownCodex } }),
      ],
      updates: [candidate(npmCodex)],
    });
    const { container } = renderOverview();
    const headline = await screen.findByRole("heading", {
      level: 2,
      name: "Everything you can update here is up to date",
    });
    expect(statusRow(container).getAttribute("data-status")).toBe("upToDate");
    expect(headline.nextElementSibling?.textContent).toBe("1 not used in Terminal");
    fireEvent.click(buttonOf(container));
    expect(useUiStore.getState().page).toBe("updates");
    // Unticked: Update All leaves it out (U4).
    expect(useUiStore.getState().selectedUpdates).toEqual([]);

    const zh = await headlineIn("zh-CN", "能在这里更新的都已是最新");
    expect(zh.nextElementSibling?.textContent).toBe("1个终端用不到");
  });

  it("is all good beside a Python with no pip, which no check would find otherwise, and says so under it", async () => {
    const noPip = instance("pip:/opt/local/bin/python3.13", "pip", {
      exe_path: "/opt/local/bin/python3.13",
      read_only_reason: "ByDesign",
      status: { unavailable: "NoPip", notes: [] },
    });
    served = snapshotWith({ instances: [brew, pip, noPip] });
    const { container } = renderOverview();
    await screen.findByRole("heading", { level: 2, name: "Everything you can update here is up to date" });
    expect(statusRow(container).getAttribute("data-status")).toBe("upToDate");
    // Its notice is still there, as news.
    expect(screen.getByRole("list", { name: "Needs attention" })).toBeInTheDocument();
    await headlineIn("zh-CN", "能在这里更新的都已是最新");
  });

  it("names a source that was not checked this time, in every language", async () => {
    served = snapshotWith({ instances: [brew, pip, stoppedOllama] });
    const { container } = renderOverview();
    await screen.findByRole("heading", {
      level: 2,
      name: "Ollama wasn't checked this time; everything else is up to date",
    });
    // Not the green check.
    expect(statusRow(container).getAttribute("data-status")).toBe("quiet");
    // Its problem row says why, with its button.
    expect(screen.getByRole("list", { name: "Needs attention" })).toBeInTheDocument();
    await headlineIn("zh-CN", "Ollama这次没检查，其余都是最新的");
    await headlineIn("zh-Hant", "Ollama這次沒檢查，其餘都是最新的");
  });

  it("names every source not checked, once each, in the sidebar's order", async () => {
    served = snapshotWith({ instances: [brew, stoppedOllama, stoppedUv] });
    renderOverview();
    await screen.findByRole("heading", {
      level: 2,
      name: "Ollama and uv weren't checked this time; everything else is up to date",
    });
    await headlineIn("zh-CN", "Ollama和uv这次没检查，其余都是最新的");
  });

  it("says a source whose check did not finish was not checked in full", async () => {
    served = snapshotWith({
      instances: [brew, pip, uv],
      stale: true,
      errors: [{ instance_id: uv.id, message: "uv tool list exited with code 2" }],
    });
    renderOverview();
    await screen.findByRole("heading", {
      level: 2,
      name: "uv wasn't fully checked this time; everything else is up to date",
    });
    // In the words of the problems row under it, 「部分检查未完成」.
    const zh = await headlineIn("zh-CN", "uv这次未检查完，其余都是最新的");
    expect(within(screen.getByRole("list", { name: "需要查看" })).getByText("uv这次未检查完，更新可能还没全部列出。")).toBeInTheDocument();
    expect(zh).toBeInTheDocument();
    await headlineIn("zh-Hant", "uv這次未檢查完，其餘都是最新的");
  });

  it("claims nothing else is up to date where nothing else was checked", async () => {
    served = snapshotWith({ instances: [stoppedOllama, codexOwn], artifacts: [artifact(ownCodex)] });
    const { container } = renderOverview();
    await screen.findByRole("heading", { level: 2, name: "Ollama wasn't checked this time" });
    expect(statusRow(container).getAttribute("data-status")).toBe("quiet");
    await headlineIn("zh-CN", "Ollama这次没检查");
  });

  it("says the hidden under the name, and of the rest only what can be updated here, as the green check does", async () => {
    served = snapshotWith({ instances: [brew, pip, stoppedOllama], updates: [candidate(glib)] });
    settings.ignored_updates = [glib];
    renderOverview();
    const headline = await screen.findByRole("heading", {
      level: 2,
      name: "Ollama wasn't checked this time; everything else you can update here is up to date",
    });
    await waitFor(() => expect(headline.nextElementSibling?.textContent).toBe("1 hidden"));
  });

  it("does not call the rest up to date over an update of a source that answered, which can't be updated here", async () => {
    // pip answered: its urllib3 has an update, under "Can't update here".
    served = snapshotWith({ instances: [brew, pip, stoppedOllama], updates: [candidate(urllib3)] });
    renderOverview();
    const headline = await screen.findByRole("heading", {
      level: 2,
      name: "Ollama wasn't checked this time; everything else you can update here is up to date",
    });
    expect(headline.nextElementSibling?.textContent).toBe("1 can't be updated here");
    const zh = await headlineIn("zh-CN", "Ollama这次没检查，其余能在这里更新的都已是最新");
    expect(zh.nextElementSibling?.textContent).toBe("1个无法在这里更新");
    await headlineIn("zh-Hant", "Ollama這次沒檢查，其餘能在這裡更新的都已是最新");
  });

  it("does not call the rest up to date beside Codex's own install, whose updates Banager never checks", async () => {
    served = snapshotWith({
      instances: [brew, pip, stoppedOllama, codexOwn],
      artifacts: [artifact(glib), artifact(ownCodex)],
    });
    renderOverview();
    await screen.findByRole("heading", {
      level: 2,
      name: "Ollama wasn't checked this time; everything else you can update here is up to date",
    });
  });

  it("calls the rest up to date over the rows of the source it names, kept from its last answer", async () => {
    const ruff = key(stoppedUv, "ruff", "Package");
    served = snapshotWith({
      instances: [brew, pip, stoppedUv],
      artifacts: [artifact(glib), artifact(ruff)],
      updates: [candidate(ruff)],
    });
    renderOverview();
    const headline = await screen.findByRole("heading", {
      level: 2,
      name: "uv wasn't checked this time; everything else is up to date",
    });
    expect(headline.nextElementSibling?.textContent).toBe("1 can't be updated here");
  });

  // Walk-2 review 1.2, whatever the reason each could not be: a lookup a
  // later check will not fix either (a proxy whose certificate Banager does
  // not trust, a registry that answered 404) is no green check.
  it("claims nothing was checked, with no green check, where every installed tool could not be looked up", async () => {
    const untrusted: UpdateCandidate["warnings"] = [{ SecureConnectionFailed: { host: "formulae.brew.sh" } }];
    const uncheckable = (artifactKey: ArtifactKey, warnings: UpdateCandidate["warnings"]) =>
      candidate(artifactKey, { checkable: false, target: "1.0.0", warnings });
    served = snapshotWith({
      updates: [
        uncheckable(glib, untrusted),
        uncheckable(jq, untrusted),
        uncheckable(urllib3, [{ Message: "PyPI returned status 404" }]),
      ],
    });
    const { container, unmount } = renderOverview();
    await screen.findByRole("heading", { level: 2, name: "No tools could be checked" });
    expect(statusRow(container).getAttribute("data-status")).toBe("quiet");
    await headlineIn("zh-CN", "所有工具都没有检查成功");
    await headlineIn("zh-Hant", "所有工具都沒有檢查成功");
    unmount();

    // And none named as the one not checked: no tool was.
    await i18n.changeLanguage("en");
    served = { ...served, instances: [brew, pip, stoppedOllama] };
    const again = renderOverview();
    await screen.findByRole("heading", { level: 2, name: "No tools could be checked" });
    expect(statusRow(again.container).getAttribute("data-status")).toBe("quiet");
  });
});

/**
 * Independent review r6, F5: the all good is a claim that every tool
 * Banager looks up was looked up. A lookup that did not succeed in a way
 * checking again will not mend -- a certificate rustls would not accept
 * (behind a proxy that reads https traffic), an answer that would not
 * parse, a redirect the client will not follow -- keeps it away as one
 * that got no answer does, though another tool checked fine; only the
 * Check Again stays with those checking again can mend.
 */
describe("the Overview's all good, over a lookup that did not succeed", () => {
  const cargo = instance("cargo:/Users/you/.cargo", "cargo");
  const ripgrep = key(cargo, "ripgrep", "Binary");
  const notLookedUp = (warnings: UpdateCandidate["warnings"]) =>
    candidate(ripgrep, { checkable: false, target: "1.0.0", channel: "Registry", warnings });
  const untrusted: UpdateCandidate["warnings"] = [
    {
      Message:
        "crates.io request failed: secure connection to crates.io failed: invalid peer certificate: UnknownIssuer",
    },
    { SecureConnectionFailed: { host: "crates.io" } },
  ];

  it("says no update was found in what was checked, with no green check, where crates.io's certificate was not trusted", async () => {
    // The reviewer's case: Homebrew checked jq, up to date; ripgrep's
    // lookup met a certificate Banager does not trust.
    served = snapshotWith({
      instances: [brew, cargo],
      artifacts: [artifact(jq), artifact(ripgrep)],
      updates: [notLookedUp(untrusted)],
    });
    const { container } = renderOverview();

    const headline = await screen.findByRole("heading", { level: 2, name: "No updates in the sources checked" });
    expect(statusRow(container).getAttribute("data-status")).toBe("quiet");
    // How many could not be checked, plainly.
    expect(headline.nextElementSibling?.textContent).toBe("1 can't be updated here, including 1 that couldn't be checked");
    // No Check Again for it: the next check meets the same certificate.
    expect(screen.queryByRole("list", { name: "Needs attention" })).toBeNull();
    expect(buttonOf(container)).toHaveAccessibleName("Review Updates");

    const zh = await headlineIn("zh-CN", "已检查的来源中没有可更新的工具");
    expect(zh.nextElementSibling?.textContent).toBe("1个无法在这里更新，其中1个没有检查成功");
    const hant = await headlineIn("zh-Hant", "已檢查的來源中沒有可更新的工具");
    expect(hant.nextElementSibling?.textContent).toBe("1個無法在這裡更新，其中1個沒有檢查成功");
  });

  it.each([
    ["an answer that would not parse", [{ Message: "could not parse registry manifest" }]],
    ["a redirect the client will not follow", [{ Message: "crates.io request failed: refused: refusing to follow a redirect" }]],
    ["a crate the registry does not have", [{ Message: "crates.io returned status 404" }]],
  ] as [string, UpdateCandidate["warnings"]][])("gives no green check over %s", async (_what, warnings) => {
    served = snapshotWith({
      instances: [brew, cargo],
      artifacts: [artifact(jq), artifact(ripgrep)],
      updates: [notLookedUp(warnings)],
    });
    const { container } = renderOverview();
    const headline = await screen.findByRole("heading", { level: 2, name: "No updates in the sources checked" });
    expect(statusRow(container).getAttribute("data-status")).toBe("quiet");
    expect(headline.nextElementSibling?.textContent).toBe("1 can't be updated here, including 1 that couldn't be checked");
  });

  it("keeps the green check beside a crate installed from git, which Banager never looks up", async () => {
    served = snapshotWith({
      instances: [brew, cargo],
      artifacts: [artifact(jq), artifact(ripgrep)],
      updates: [notLookedUp(["NonRegistrySource"])],
    });
    const { container } = renderOverview();
    const headline = await screen.findByRole("heading", {
      level: 2,
      name: "Everything you can update here is up to date",
    });
    expect(statusRow(container).getAttribute("data-status")).toBe("upToDate");
    expect(headline.nextElementSibling?.textContent).toBe("1 can't be updated here");
  });

  it("keeps the green check beside an Ollama whose models are on another Mac, which Banager does not look up from here", async () => {
    const remote = instance("ollama:http://server:11434", "ollama");
    const model = key(remote, "qwen3:8b", "Model");
    served = snapshotWith({
      instances: [brew, remote],
      artifacts: [artifact(jq), artifact(model)],
      updates: [
        candidate(model, {
          checkable: false,
          current: "abc",
          target: "abc",
          channel: "Digest",
          warnings: [{ Message: "remote daemon manifests cannot be checked from this Mac" }, "NotLookedUpHere"],
        }),
      ],
    });
    const { container } = renderOverview();
    const headline = await screen.findByRole("heading", {
      level: 2,
      name: "Everything you can update here is up to date",
    });
    expect(statusRow(container).getAttribute("data-status")).toBe("upToDate");
    expect(headline.nextElementSibling?.textContent).toBe("1 can't be updated here");
  });

  it.each([
    [
      "a model from hf.co",
      "hf.co/bartowski/Llama-3.2-3B-Instruct-GGUF:Q4_K_M",
      "models from hf.co are not looked up; only those from registry.ollama.ai are",
    ],
    [
      "a model whose manifest is not where Banager reads (OLLAMA_MODELS elsewhere)",
      "qwen3:8b",
      "could not read local manifest /Users/you/.ollama/models/manifests/registry.ollama.ai/library/qwen3/8b: no such file or folder",
    ],
  ])("keeps the green check beside %s, which Banager does not look up from here", async (_what, name, message) => {
    // F5 review: no request made, at this check or the next.
    const local = instance("ollama:http://127.0.0.1:11434", "ollama");
    const model = key(local, name, "Model");
    served = snapshotWith({
      instances: [brew, local],
      artifacts: [artifact(jq), artifact(model)],
      updates: [
        candidate(model, {
          checkable: false,
          current: "abc",
          target: "abc",
          channel: "Digest",
          warnings: [{ Message: message }, "NotLookedUpHere"],
        }),
      ],
    });
    const { container } = renderOverview();
    const headline = await screen.findByRole("heading", {
      level: 2,
      name: "Everything you can update here is up to date",
    });
    expect(statusRow(container).getAttribute("data-status")).toBe("upToDate");
    expect(headline.nextElementSibling?.textContent).toBe("1 can't be updated here");
  });

  it("does not call the rest up to date where a source that answered had a lookup fail", async () => {
    served = snapshotWith({
      instances: [brew, cargo, stoppedOllama],
      artifacts: [artifact(jq), artifact(ripgrep)],
      updates: [notLookedUp(untrusted)],
    });
    const { container } = renderOverview();
    const headline = await screen.findByRole("heading", { level: 2, name: "No updates in the sources checked" });
    expect(statusRow(container).getAttribute("data-status")).toBe("quiet");
    expect(headline.nextElementSibling?.textContent).toBe("1 can't be updated here, including 1 that couldn't be checked");
    await headlineIn("zh-CN", "已检查的来源中没有可更新的工具");
  });

  it("still names the source not checked where the only lookup that did not succeed is that source's own", async () => {
    // F5 review: uv did not answer; its last answer, kept, had ruff's
    // lookup meet a certificate Banager does not trust. That row is uv's,
    // said under the headline and not held against the rest, which all
    // checked fine (`updatesSummary`'s `everythingElse`).
    const ruff = key(stoppedUv, "ruff", "Package");
    served = snapshotWith({
      instances: [brew, pip, stoppedUv],
      artifacts: [artifact(glib), artifact(ruff)],
      updates: [
        candidate(ruff, {
          checkable: false,
          target: "1.0.0",
          channel: "Registry",
          warnings: [
            { Message: "PyPI request failed: secure connection to pypi.org failed: invalid peer certificate: UnknownIssuer" },
            { SecureConnectionFailed: { host: "pypi.org" } },
          ],
        }),
      ],
    });
    const { container } = renderOverview();
    const headline = await screen.findByRole("heading", {
      level: 2,
      name: "uv wasn't checked this time; everything else is up to date",
    });
    expect(statusRow(container).getAttribute("data-status")).toBe("quiet");
    // The line still counts it: of what can't be updated here, it could not be checked.
    expect(headline.nextElementSibling?.textContent).toBe("1 can't be updated here, including 1 that couldn't be checked");
    await headlineIn("zh-CN", "uv这次没检查，其余都是最新的");
  });
});
