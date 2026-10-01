import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, screen, within } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import { WithToolbarSlot } from "../test/toolbarSlot";
import { InstalledPage } from "./InstalledPage";
import type { InstalledArtifact, ManagerInstance, Settings, Snapshot } from "../lib/types";
import { NO_FACTS } from "../lib/types";

// Homebrew's state on the Installed page: a row's quiet word, and the
// inspector's sentences. The harness is InstalledPage.test.tsx's, cut to
// what these need.

const mockInvoke = vi.mocked(invoke);

const brew: ManagerInstance = {
  id: "brew:/opt/homebrew",
  adapter_id: "brew",
  exe_path: "/opt/homebrew/bin/brew",
  prefix: "/opt/homebrew",
  scope: "User",
  version: "7.0.3",
  status: { unavailable: null, notes: [] },
  unverified_version: null,
  read_only_reason: null,
};

const settings: Settings = {
  language: "System",
  show_technical_details: false,
  ignored_updates: [],
  skipped_versions: [],
  include_self_updating: false,
  auto_check: false,
  notify_updates: false,
};

function artifact(over: Partial<InstalledArtifact> & { name: string; kind: "Formula" | "Cask" }): InstalledArtifact {
  const { name, kind, ...rest } = over;
  return {
    key: { instance_id: brew.id, kind, name },
    display_name: name,
    version: "1.0",
    reason: "Requested",
    description: `${name} blurb`,
    homepage: null,
    size_bytes: null,
    installed_at: null,
    path: null,
    auto_updates: false,
    uninstall_blocked: null,
    facts: NO_FACTS,
    ...rest,
  };
}

// 2026-09-12 in UTC and in Asia/Shanghai alike.
const INSTALLED = 1789185600;

const snapshot: Snapshot = {
  generation: 1,
  round: 1,
  detect: "Found",
  instances: [brew],
  artifacts: [
    artifact({
      name: "oldapp",
      kind: "Cask",
      installed_at: INSTALLED,
      homepage: "https://oldapp.example/",
      facts: {
        ...NO_FACTS,
        homebrew: {
          deprecated: null,
          disabled: { date: "2026-09-01", reason: "fails_gatekeeper_check", replacement: "newapp" },
          caveats: null,
          other_versions: [],
        },
      },
    }),
    artifact({
      name: "openssl@3",
      kind: "Formula",
      version: "3.6.4",
      facts: {
        ...NO_FACTS,
        homebrew: { deprecated: null, disabled: null, caveats: "Certificates live in\n  $HOMEBREW_PREFIX/etc", other_versions: ["3.6.3"] },
      },
    }),
  ],
  updates: [],
  refreshed_at: 1789700000,
  stale: false,
  errors: [],
};

beforeEach(() => {
  mockInvoke.mockReset();
  // The virtualizer measures through offsetHeight / offsetWidth, which
  // jsdom leaves at 0 (InstalledPage.test.tsx has the story).
  vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockImplementation(function (this: HTMLElement) {
    return this.getAttribute("data-index") === null ? 600 : 56;
  });
  vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(800);
  mockInvoke.mockImplementation((cmd: string) => {
    if (cmd === "get_snapshot") return Promise.resolve(snapshot);
    if (cmd === "get_settings") return Promise.resolve(settings);
    if (cmd === "list_operations") return Promise.resolve([]);
    return Promise.resolve(undefined);
  });
});

afterEach(() => {
  vi.restoreAllMocks();
});

async function rowOf(name: string): Promise<HTMLElement> {
  const label = await screen.findByText(name, { selector: "[data-tool-row] p" });
  return label.closest("[data-tool-row]") as HTMLElement;
}

async function openDetails(name: string): Promise<HTMLElement> {
  const row = await rowOf(name);
  fireEvent.click(within(row).getByRole("button", { name: `Details: ${name}` }));
  return screen.findByRole("complementary", { name });
}

describe("InstalledPage, Homebrew's state", () => {
  it("puts Disabled on a disabled cask's row, and nothing on a formula Homebrew has not marked", async () => {
    renderWithProviders(
      <WithToolbarSlot>
        <InstalledPage />
      </WithToolbarSlot>,
    );
    const oldapp = await rowOf("oldapp");
    expect(within(oldapp).getByRole("button", { name: "Disabled: oldapp" })).toHaveTextContent("Disabled");
    const openssl = await rowOf("openssl@3");
    expect(within(openssl).queryByText("Disabled")).toBeNull();
    expect(within(openssl).queryByText("Deprecated")).toBeNull();
  });

  it("says in the inspector when the cask was installed, why no update will come, and what Homebrew suggests", async () => {
    renderWithProviders(
      <WithToolbarSlot>
        <InstalledPage />
      </WithToolbarSlot>,
    );
    const pane = await openDetails("oldapp");
    const date = new Intl.DateTimeFormat("en", { dateStyle: "medium" }).format(new Date(INSTALLED * 1000));
    expect(within(pane).getByText("Date installed").nextElementSibling).toHaveTextContent(date);
    expect(pane.querySelector("[data-homebrew-mark]")?.textContent).toBe(
      "It doesn't pass the macOS security check. Homebrew disabled it on 2026-09-01 and won't provide more updates. The installed copy isn't removed; uninstall it when you no longer need it.",
    );
    // Right under the facts, whose last row, 状态, says the mark's word;
    // the folded notes come last, after the commands.
    expect(pane.querySelector("[data-facts] + [data-homebrew-notes='mark']")).not.toBeNull();
    expect(pane.querySelector("[data-homebrew-replacement]")?.textContent).toBe("Homebrew suggests “newapp” instead.");
    expect(within(pane).queryByRole("button", { name: /Install/ })).toBeNull();
    expect(within(pane).getByText("Homepage").nextElementSibling).toHaveTextContent("oldapp.example");
    expect(within(pane).getByRole("button", { name: "Copy Link" })).toBeInTheDocument();
    // No update will come, so it is not called up to date either.
    expect(within(pane).queryByText("Up to date")).toBeNull();
    // The status row says the word; the sentence is said once, under the
    // facts, not again behind an ⓘ on the word.
    const status = within(pane).getByText("Status").nextElementSibling as HTMLElement;
    expect(within(status).getByText("Disabled")).toBeInTheDocument();
    expect(within(status).queryByRole("button", { name: "Details: Disabled" })).toBeNull();
  });

  it("says a formula's other installed version as a fact and keeps its caveats folded", async () => {
    renderWithProviders(
      <WithToolbarSlot>
        <InstalledPage />
      </WithToolbarSlot>,
    );
    const pane = await openDetails("openssl@3");
    expect(within(pane).getByText("Other versions").nextElementSibling).toHaveTextContent("3.6.3");
    expect(within(pane).getByRole("button", { name: "Homebrew's notes" })).toHaveAttribute(
      "aria-expanded",
      "false",
    );
    expect(within(pane).queryByText(/Certificates live in/)).toBeNull();
  });
});
