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
    await headlineIn("zh-CN", "uv这次没检查完，其余都是最新的");
    await headlineIn("zh-Hant", "uv這次沒檢查完，其餘都是最新的");
  });

  it("claims nothing else is up to date where nothing else was checked", async () => {
    served = snapshotWith({ instances: [stoppedOllama, codexOwn], artifacts: [artifact(ownCodex)] });
    const { container } = renderOverview();
    await screen.findByRole("heading", { level: 2, name: "Ollama wasn't checked this time" });
    expect(statusRow(container).getAttribute("data-status")).toBe("quiet");
    await headlineIn("zh-CN", "Ollama这次没检查");
  });

  it("says the hidden and the unchecked under the name, as under the green check", async () => {
    served = snapshotWith({ instances: [brew, pip, stoppedOllama], updates: [candidate(glib)] });
    settings.ignored_updates = [glib];
    renderOverview();
    const headline = await screen.findByRole("heading", {
      level: 2,
      name: "Ollama wasn't checked this time; everything else is up to date",
    });
    await waitFor(() => expect(headline.nextElementSibling?.textContent).toBe("1 hidden"));
  });
});
