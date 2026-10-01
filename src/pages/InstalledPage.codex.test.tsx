import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, screen, within } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import { WithToolbarSlot } from "../test/toolbarSlot";
import { InstalledPage } from "./InstalledPage";
import type { InstalledArtifact, ManagerInstance, Settings, Snapshot } from "../lib/types";
import { NO_FACTS } from "../lib/types";

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
  it("says what Codex is, and that it has to be uninstalled by hand", async () => {
    page();
    const tool = await rowOf("Codex");
    expect(within(tool).getByText("OpenAI's AI coding assistant")).toBeInTheDocument();
    // The row's one word: what stands in the way of its Uninstall.
    expect(within(tool).getByText("Manual uninstall")).toBeInTheDocument();
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
      await screen.findByText("Codex installs new versions itself. Its updates aren't checked or installed here."),
    ).toBeInTheDocument();
    fireEvent.click(within(status).getByRole("button", { name: "Details: Manual uninstall" }));
    expect(
      await screen.findByText(
        "Codex is only listed here for now and can't be uninstalled here. Follow Codex's official instructions to uninstall it.",
      ),
    ).toBeInTheDocument();
  });

  it("says only that it is not checked when the install does not follow the latest release", async () => {
    serve(snapshotWith(false));
    page();
    const pane = await openDetails("Codex");
    const status = within(pane).getByText("Status").nextElementSibling as HTMLElement;
    expect(within(status).getByText("Not checked")).toBeInTheDocument();
    expect(within(status).queryByText("Updates itself")).toBeNull();
    expect(within(pane).queryByText("Up to date")).toBeNull();
    fireEvent.click(within(status).getByRole("button", { name: "Details: Not checked" }));
    expect(await screen.findByText("Codex's updates aren't checked or installed here.")).toBeInTheDocument();
  });

  it("still calls a checked tool beside it up to date", async () => {
    page();
    const pane = await openDetails("Claude Code");
    expect(within(pane).getByText("Up to date")).toBeInTheDocument();
    expect(within(pane).queryByText("Updates itself")).toBeNull();
  });
});
