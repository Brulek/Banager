import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, screen, within } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import { WithToolbarSlot } from "../test/toolbarSlot";
import { InstalledPage } from "./InstalledPage";
import type { InstalledArtifact, ManagerInstance, Settings, Snapshot } from "../lib/types";
import { NO_FACTS } from "../lib/types";
import { BUTTON } from "../components/ui/controls";

// Codex installed by its own script, listed only: Banager checks nothing
// for it, so its row never says "Up to date" -- it says Codex updates
// itself, or only that it is not checked -- and it cannot be uninstalled
// here. Claude Code beside it is checked, and is up to date. The harness
// is InstalledPage.homebrew.test.tsx's.

const mockInvoke = vi.mocked(invoke);

function standalone(id: string, exe: string, prefix: string, version: string): ManagerInstance {
  return {
    id,
    adapter_id: id,
    exe_path: exe,
    prefix,
    scope: "User",
    version,
    status: { unavailable: null, notes: [] },
    unverified_version: null,
    read_only_reason: null,
  };
}

const codex = standalone(
  "standalone-codex",
  "/Users/you/.local/bin/codex",
  "/Users/you/.codex/packages/standalone",
  "0.159.3",
);
const claude = standalone(
  "standalone-claude",
  "/Users/you/.local/bin/claude",
  "/Users/you/.local/share/claude",
  "2.1.282",
);

const settings: Settings = {
  language: "System",
  show_technical_details: false,
  ignored_updates: [],
  skipped_versions: [],
  include_self_updating: false,
  auto_check: false,
  notify_updates: false,
};

function row(instance: ManagerInstance, name: string, display: string, over: Partial<InstalledArtifact>): InstalledArtifact {
  return {
    key: { instance_id: instance.id, kind: "Binary", name },
    display_name: display,
    version: instance.version ?? "",
    reason: "Requested",
    description: null,
    homepage: null,
    size_bytes: null,
    installed_at: null,
    path: null,
    auto_updates: false,
    uninstall_blocked: null,
    facts: NO_FACTS,
    ...over,
  };
}

function snapshotWith(codexFollowsLatest: boolean): Snapshot {
  return {
    generation: 1,
    round: 1,
    detect: "Found",
    instances: [claude, codex],
    artifacts: [
      row(claude, "claude", "Claude Code", { auto_updates: true }),
      row(codex, "codex", "Codex", { auto_updates: codexFollowsLatest, uninstall_blocked: "NoSafeMethod" }),
    ],
    updates: [],
    refreshed_at: 1789700000,
    stale: false,
    errors: [],
  };
}

function serve(snapshot: Snapshot): void {
  mockInvoke.mockImplementation((cmd: string) => {
    if (cmd === "get_snapshot") return Promise.resolve(snapshot);
    if (cmd === "get_settings") return Promise.resolve(settings);
    if (cmd === "list_operations") return Promise.resolve([]);
    return Promise.resolve(undefined);
  });
}

beforeEach(() => {
  mockInvoke.mockReset();
  vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockImplementation(function (this: HTMLElement) {
    return this.getAttribute("data-index") === null ? 600 : 56;
  });
  vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(800);
  serve(snapshotWith(true));
});

afterEach(() => {
  vi.restoreAllMocks();
});

function page() {
  return renderWithProviders(
    <WithToolbarSlot>
      <InstalledPage />
    </WithToolbarSlot>,
  );
}

async function rowOf(name: string): Promise<HTMLElement> {
  const label = await screen.findByText(name, { selector: "[data-tool-row] p" });
  return label.closest("[data-tool-row]") as HTMLElement;
}

async function openDetails(name: string): Promise<HTMLElement> {
  const tool = await rowOf(name);
  fireEvent.click(within(tool).getByRole("button", { name: `Details: ${name}` }));
  return screen.findByRole("complementary", { name });
}

describe("InstalledPage, Codex's own install", () => {
  it("says what Codex is, and that it is only listed here", async () => {
    page();
    const tool = await rowOf("Codex");
    expect(within(tool).getByText("OpenAI's AI coding assistant")).toBeInTheDocument();
    // The row's one word: what stands in the way of its Uninstall -- not
    // "uninstall by hand", which would promise a way nothing here gives.
    expect(within(tool).getByText("Listed only")).toBeInTheDocument();
    expect(within(tool).queryByText("Up to date")).toBeNull();
  });

  it("says in the inspector that Codex updates itself, never that it is up to date", async () => {
    page();
    const pane = await openDetails("Codex");
    const status = within(pane).getByText("Status").nextElementSibling as HTMLElement;
    expect(within(status).getByText("Updates itself")).toBeInTheDocument();
    expect(within(pane).queryByText("Up to date")).toBeNull();
    fireEvent.click(within(status).getByRole("button", { name: "Details: Updates itself" }));
    expect(
      await screen.findByText(
        "Codex was installed by its own script as the latest release, not a pinned one, so it can install new versions itself. Its updates aren't checked or installed here.",
      ),
    ).toBeInTheDocument();
    fireEvent.click(within(status).getByRole("button", { name: "Details: Listed only" }));
    expect(
      await screen.findByText("This copy of Codex was installed by its own script. It's only listed here and can't be uninstalled here."),
    ).toBeInTheDocument();
  });

  it("says only that it is not checked when the install does not follow the latest release", async () => {
    serve(snapshotWith(false));
    page();
    const pane = await openDetails("Codex");
    const status = within(pane).getByText("Status").nextElementSibling as HTMLElement;
    expect(within(status).getByText("Updates not checked")).toBeInTheDocument();
    expect(within(status).queryByText("Updates itself")).toBeNull();
    expect(within(pane).queryByText("Up to date")).toBeNull();
    fireEvent.click(within(status).getByRole("button", { name: "Details: Updates not checked" }));
    expect(await screen.findByText("Codex's updates aren't checked or installed here.")).toBeInTheDocument();
  });

  it("says 「装了两份」 on the row when npm has a copy too, as the 「装了不止一份」 filter lists it, and which copy runs", async () => {
    const npm: ManagerInstance = { ...standalone("npm:/opt/homebrew", "/opt/homebrew/bin/npm", "/opt/homebrew", "11.0.0"), adapter_id: "npm" };
    const npmCodex: InstalledArtifact = {
      ...row(npm, "@openai/codex", "@openai/codex", {}),
      key: { instance_id: npm.id, kind: "Package", name: "@openai/codex" },
      version: "0.155.1",
      facts: {
        ...NO_FACTS,
        family: "codex",
        commands: [{ name: "codex", state: { ShadowedBy: { by: { instance_id: codex.id, kind: "Binary", name: "codex" } } } }],
      },
    };
    const base = snapshotWith(true);
    const own = {
      ...base.artifacts[1],
      facts: { ...NO_FACTS, family: "codex", commands: [{ name: "codex", state: "Runs" as const }] },
    };
    serve({ ...base, instances: [...base.instances, npm], artifacts: [base.artifacts[0], own, npmCodex] });
    page();
    const tool = await rowOf("Codex");
    // The twin outranks 「只列出」 on the row; the details say both.
    expect(within(tool).getByText("Installed twice")).toBeInTheDocument();
    expect(within(tool).queryByText("Listed only")).toBeNull();
    const pane = await openDetails("Codex");
    expect(pane.querySelector("[data-twin-advice]")?.textContent).toBe(
      "Typing codex in Terminal runs this copy; Terminal doesn't use the one from npm, version 0.155.1.",
    );
    const status = within(pane).getByText("Status").nextElementSibling as HTMLElement;
    expect([...status.querySelectorAll("[data-status-word]")].map((word) => word.textContent)).toEqual([
      "Listed only",
      "Updates itself",
    ]);
    // npm's copy, which Terminal does not run: what it is, and that it may go.
    const npmPane = await openDetails("@openai/codex");
    expect([...npmPane.querySelectorAll("[data-twin-advice]")].map((line) => line.textContent)).toEqual([
      "Typing codex in Terminal runs the copy from Codex's own installer, version 0.159.3, so Terminal doesn't use this one.",
      "If you don't need it, you can uninstall this copy.",
    ]);
  });

  it("greys Update on the copy Terminal does not run, and keeps it blue on one it does", async () => {
    const npm: ManagerInstance = { ...standalone("npm:/opt/homebrew", "/opt/homebrew/bin/npm", "/opt/homebrew", "11.0.0"), adapter_id: "npm" };
    const npmKey = { instance_id: npm.id, kind: "Package" as const, name: "@openai/codex" };
    const npmCodex: InstalledArtifact = {
      ...row(npm, "@openai/codex", "@openai/codex", {}),
      key: npmKey,
      version: "0.155.1",
      facts: {
        ...NO_FACTS,
        family: "codex",
        commands: [{ name: "codex", state: { ShadowedBy: { by: { instance_id: codex.id, kind: "Binary", name: "codex" } } } }],
      },
    };
    const prettier: InstalledArtifact = { ...row(npm, "prettier", "prettier", {}), key: { ...npmKey, name: "prettier" }, version: "3.0.0" };
    const base = snapshotWith(true);
    const own = {
      ...base.artifacts[1],
      facts: { ...NO_FACTS, family: "codex", commands: [{ name: "codex", state: "Runs" as const }] },
    };
    const update = (key: typeof npmKey, current: string, target: string) => ({
      key,
      current,
      target,
      channel: "Native" as const,
      checkable: true,
      warnings: [],
      blocked: null,
    });
    serve({
      ...base,
      instances: [...base.instances, npm],
      artifacts: [base.artifacts[0], own, npmCodex, prettier],
      updates: [update(npmKey, "0.155.1", "0.159.3"), update(prettier.key as typeof npmKey, "3.0.0", "3.1.0")],
    });
    page();
    const npmPane = await openDetails("@openai/codex");
    const greyUpdate = within(npmPane).getByRole("button", { name: "Update" });
    expect(greyUpdate.className).toBe(BUTTON.regular.grey);
    const prettierPane = await openDetails("prettier");
    expect(within(prettierPane).getByRole("button", { name: "Update" }).className).toBe(BUTTON.regular.default);
  });

  it("still calls a checked tool beside it up to date", async () => {
    page();
    const pane = await openDetails("Claude Code");
    expect(within(pane).getByText("Up to date")).toBeInTheDocument();
    expect(within(pane).queryByText("Updates itself")).toBeNull();
  });
});
