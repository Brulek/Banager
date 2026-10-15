import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, screen, within } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import { WithToolbarSlot } from "../test/toolbarSlot";
import { InstalledPage } from "./InstalledPage";
import type { InstalledArtifact, ManagerInstance, Settings, Snapshot } from "../lib/types";
import { NO_FACTS } from "../lib/types";

// opencode installed by its own script, listed only, as Codex's own
// install was before it could be uninstalled (InstalledPage.codex.test.tsx,
// whose harness this is): no version is read, Banager checks nothing for
// it, so its row never says "Up to date" -- it says opencode updates
// itself, by default, and why its version is unknown -- and it cannot be
// uninstalled here.

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
    answered_at: null,
    unverified_version: null,
    read_only_reason: null,
  };
}

// No version is read: the instance's and the row's are empty.
const opencode: ManagerInstance = {
  ...standalone("standalone-opencode", "/Users/you/.opencode/bin/opencode", "/Users/you/.opencode", ""),
  version: null,
};
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

function snapshotWith(): Snapshot {
  return {
    generation: 1,
    round: 1,
    detect: "Found",
    instances: [claude, opencode],
    artifacts: [
      row(claude, "claude", "Claude Code", { auto_updates: true }),
      row(opencode, "opencode", "opencode", {
        auto_updates: true,
        uninstall_blocked: "NoSafeMethod",
        path: "/Users/you/.opencode/bin/opencode",
      }),
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
  serve(snapshotWith());
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

describe("InstalledPage, opencode's own install", () => {
  it("says what opencode is, with no version, and that it has to be uninstalled by hand", async () => {
    page();
    const tool = await rowOf("opencode");
    expect(within(tool).getByText("An open-source AI coding assistant")).toBeInTheDocument();
    expect(within(tool).getByText("Manual uninstall")).toBeInTheDocument();
    expect(within(tool).queryByText("Up to date")).toBeNull();
  });

  it("says in the inspector that opencode updates itself by default and why its version is unknown", async () => {
    page();
    const pane = await openDetails("opencode");
    const status = within(pane).getByText("Status").nextElementSibling as HTMLElement;
    // Its default, not a setting Banager read: the word says so.
    expect(within(status).getByText("Updates itself by default")).toBeInTheDocument();
    expect(within(pane).queryByText("Up to date")).toBeNull();
    fireEvent.click(within(status).getByRole("button", { name: "Details: Updates itself by default" }));
    expect(
      await screen.findByText(
        "By default, opencode downloads new versions itself when it starts. Its updates aren't checked or installed here.",
      ),
    ).toBeInTheDocument();
    expect(
      screen.getByText(
        "opencode's install script leaves no file that names its version, and opencode isn't run here to ask, so its version isn't known.",
      ),
    ).toBeInTheDocument();
    fireEvent.click(within(status).getByRole("button", { name: "Details: Manual uninstall" }));
    expect(
      await screen.findByText("opencode is only listed here for now and can't be uninstalled here."),
    ).toBeInTheDocument();
  });
});
