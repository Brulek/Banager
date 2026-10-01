import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import { WithToolbarSlot } from "../test/toolbarSlot";
import { InstalledPage } from "./InstalledPage";
import { useUiStore } from "../store/ui";
import { SEARCH_SETTLE_MS } from "../lib/settled";
import i18n from "../i18n";
import type { ArtifactKey, InstalledArtifact, Settings, Snapshot } from "../lib/types";
import { NO_FACTS } from "../lib/types";

/**
 * A search that hides the selected tool closes its details, as a 「显示」
 * choice that hides it does (backlog, 「搜索把选中的工具藏掉时，详情仍
 * 开着」) -- once the user has stopped typing: a letter that hides it and
 * the Backspace that brings it back close nothing.
 */

const mockInvoke = vi.mocked(invoke);

const BREW = "brew:/opt/homebrew";
const PIPX = "pipx:/opt/homebrew/bin";

function instance(adapterId: string, id: string): Snapshot["instances"][number] {
  return {
    id,
    adapter_id: adapterId,
    exe_path: `/opt/homebrew/bin/${adapterId}`,
    prefix: "/opt/homebrew",
    scope: "User",
    version: "1.0.0",
    status: { unavailable: null, notes: [] },
    unverified_version: null,
    read_only_reason: null,
  };
}

function artifact(key: ArtifactKey): InstalledArtifact {
  return {
    key,
    display_name: key.name,
    version: "1.0.0",
    reason: "Requested",
    description: null,
    homepage: null,
    size_bytes: null,
    installed_at: null,
    path: null,
    auto_updates: false,
    uninstall_blocked: null,
    facts: { ...NO_FACTS, commands: [{ name: key.name, state: "Runs" }] },
  };
}

const wget: ArtifactKey = { instance_id: BREW, kind: "Formula", name: "wget" };
const jq: ArtifactKey = { instance_id: BREW, kind: "Formula", name: "jq" };
const httpie: ArtifactKey = { instance_id: PIPX, kind: "Tool", name: "httpie" };

const settings: Settings = {
  language: "System",
  show_technical_details: false,
  ignored_updates: [],
  skipped_versions: [],
  include_self_updating: false,
  auto_check: false,
  notify_updates: false,
};

beforeEach(() => {
  mockInvoke.mockReset();
  vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockImplementation(function (this: HTMLElement) {
    return this.getAttribute("data-index") === null ? 600 : 56;
  });
  vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(800);
  mockInvoke.mockImplementation((cmd: string) => {
    if (cmd === "get_snapshot")
      return Promise.resolve({
        generation: 1,
        round: 1,
        detect: "Found",
        instances: [instance("brew", BREW), instance("pipx", PIPX)],
        artifacts: [artifact(wget), artifact(jq), artifact(httpie)],
        updates: [],
        refreshed_at: 1789700000,
        stale: false,
        errors: [],
      } satisfies Snapshot);
    if (cmd === "get_settings") return Promise.resolve(settings);
    if (cmd === "list_operations") return Promise.resolve([]);
    return Promise.resolve(undefined);
  });
});

afterEach(() => {
  vi.restoreAllMocks();
  void i18n.changeLanguage("en");
});

function rowNames(): string[] {
  return [...document.querySelectorAll("[data-tool-row]")].map((row) => row.querySelector("p")?.textContent ?? "");
}

function renderInstalled() {
  return renderWithProviders(
    <WithToolbarSlot>
      <InstalledPage />
    </WithToolbarSlot>,
  );
}

async function openDetails(name: string): Promise<HTMLElement> {
  const label = await screen.findByText(name, { selector: "[data-tool-row] p" });
  const row = label.closest("[data-tool-row]") as HTMLElement;
  fireEvent.click(within(row).getByRole("button", { name: i18n.t("common.detailsLabel", { title: name }) }));
  return screen.findByRole("complementary", { name });
}

/** Types `text` into the search field, which keeps the focus, as typing does. */
function type(text: string): HTMLInputElement {
  const field = screen.getByRole("searchbox", { name: i18n.t("installed.filterLabel") }) as HTMLInputElement;
  field.focus();
  fireEvent.change(field, { target: { value: text } });
  return field;
}

/** Waits past the pause after which a search counts as settled. */
async function pause(ms = SEARCH_SETTLE_MS + 300): Promise<void> {
  await act(() => new Promise((resolve) => setTimeout(resolve, ms)));
}

describe("a search that hides the selected tool", () => {
  it("closes its details once the typing stops, the focus staying in the search field", async () => {
    renderInstalled();
    await openDetails("wget");
    const field = type("http");
    expect(rowNames()).toEqual(["httpie"]);
    // Still typing: nothing closes yet.
    expect(screen.getByRole("complementary", { name: "wget" })).toBeInTheDocument();
    await waitFor(() => expect(screen.queryByRole("complementary")).toBeNull(), {
      timeout: SEARCH_SETTLE_MS + 1000,
    });
    expect(field).toHaveFocus();
    // The search cleared: wget is not selected again behind the user's back.
    type("");
    await waitFor(() => expect(rowNames()).toContain("wget"));
    expect(screen.queryByRole("complementary")).toBeNull();
    expect(document.querySelector("[data-tool-row][data-selected]")).toBeNull();
  });

  it("closes nothing for a letter that hides it and the Backspace that brings it back", async () => {
    renderInstalled();
    await openDetails("wget");
    type("wgx");
    expect(rowNames()).toEqual([]);
    await pause(SEARCH_SETTLE_MS / 4);
    type("wg");
    expect(rowNames()).toEqual(["wget"]);
    await pause();
    expect(screen.getByRole("complementary", { name: "wget" })).toBeInTheDocument();
    expect(document.querySelector("[data-tool-row][data-selected]")).not.toBeNull();
  });

  it("keeps the details of a tool the search still shows", async () => {
    renderInstalled();
    await openDetails("httpie");
    type("http");
    await pause();
    expect(rowNames()).toEqual(["httpie"]);
    expect(screen.getByRole("complementary", { name: "httpie" })).toBeInTheDocument();
  });

  it("keeps the details a notice's Show opens while the search it clears had hidden that tool", async () => {
    renderInstalled();
    await openDetails("wget");
    type("wg");
    await pause();
    // The search settled on 「wg」, which hides httpie; Show clears it and
    // selects httpie at once, before 「」 has settled.
    act(() => useUiStore.getState().showInstalledTool(httpie));
    expect(await screen.findByRole("complementary", { name: "httpie" })).toBeInTheDocument();
    await pause();
    expect(screen.getByRole("complementary", { name: "httpie" })).toBeInTheDocument();
  });

  it("puts the focus on the list's first row when it was in the details the search closed", async () => {
    renderInstalled();
    const details = await openDetails("wget");
    type("jq");
    // Into the details before the search settles.
    const heading = within(details).getByRole("heading", { name: "wget" });
    heading.focus();
    await waitFor(() => expect(screen.queryByRole("complementary")).toBeNull(), {
      timeout: SEARCH_SETTLE_MS + 1000,
    });
    await waitFor(() => expect(document.activeElement?.closest("[data-tool-row]")?.textContent).toContain("jq"));
  });
});
