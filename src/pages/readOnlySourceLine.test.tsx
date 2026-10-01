import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { screen, waitFor } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import { WithToolbarSlot } from "../test/toolbarSlot";
import { useUiStore } from "../store/ui";
import { InstalledPage } from "./InstalledPage";
import i18n from "../i18n";
import type { ArtifactKey, CommandFact, InstalledArtifact, Settings, Snapshot } from "../lib/types";
import { NO_FACTS } from "../lib/types";

/**
 * The Installed page on one read-only source (pip): a line over the list
 * saying why it is view only, in the words of each row's 「仅供查看」 ⓘ, in
 * place of a 全选 box that could tick nothing.
 */

const mockInvoke = vi.mocked(invoke);

const BREW = "brew:/opt/homebrew";
const PIP = "pip:/usr/bin/python3";
const NPM = "npm:/usr/local";

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
    read_only_reason: id === PIP ? "ByDesign" : id === NPM ? "PrefixNotWritable" : null,
  };
}

function artifact(key: ArtifactKey, commands: CommandFact[], displayName = key.name): InstalledArtifact {
  return {
    key,
    display_name: displayName,
    version: "1.0.0",
    reason: "Requested",
    description: null,
    homepage: null,
    size_bytes: null,
    installed_at: null,
    path: null,
    auto_updates: false,
    uninstall_blocked: null,
    facts: { ...NO_FACTS, commands },
  };
}

const wget: ArtifactKey = { instance_id: BREW, kind: "Formula", name: "wget" };
const requests: ArtifactKey = { instance_id: PIP, kind: "Package", name: "requests" };
const typescript: ArtifactKey = { instance_id: NPM, kind: "Package", name: "typescript" };

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
  const artifacts = [artifact(wget, []), artifact(requests, []), artifact(typescript, [])];
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
        instances: [instance("brew", BREW), instance("pip", PIP), instance("npm", NPM)],
        artifacts,
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

function renderInstalled() {
  return renderWithProviders(
    <WithToolbarSlot>
      <InstalledPage />
    </WithToolbarSlot>,
  );
}

const PIP_WHY = "You can only view pip installs here. Install command-line tools with pipx or uv instead to update and uninstall them here.";
const selectAll = () => screen.queryByRole("checkbox", { name: "Select all items that can be uninstalled here" });

describe("the Installed page on a read-only source", () => {
  it("says over the list why pip's page is view only, with no Select All", async () => {
    useUiStore.getState().openInstalled(PIP);
    renderInstalled();
    await screen.findByText("requests", { selector: "[data-tool-row] p" });
    const line = document.querySelector("[data-read-only-line]");
    expect(line).toHaveTextContent(PIP_WHY);
    expect(selectAll()).toBeNull();
    expect(document.querySelector("[data-selection-header]")).toBeNull();
    // Said once, over the list: no row repeats View only.
    expect(screen.queryByText("View only")).toBeNull();
  });

  it("still marks a read-only source's rows View only on All Tools", async () => {
    useUiStore.getState().openInstalled(null);
    renderInstalled();
    await screen.findByText("requests", { selector: "[data-tool-row] p" });
    expect(screen.getAllByText("View only").length).toBeGreaterThan(0);
  });

  it("gives npm's own reason where its folder can't be changed", async () => {
    useUiStore.getState().openInstalled(NPM);
    renderInstalled();
    await screen.findByText("typescript", { selector: "[data-tool-row] p" });
    expect(document.querySelector("[data-read-only-line]")).toHaveTextContent(
      /^npm keeps these in a folder your account can't change, so you can only view them\./,
    );
    expect(selectAll()).toBeNull();
  });

  it("keeps Select All on a source that can be changed, and on All Tools", async () => {
    useUiStore.getState().openInstalled(BREW);
    const { unmount } = renderInstalled();
    await screen.findByText("wget", { selector: "[data-tool-row] p" });
    expect(selectAll()).not.toBeNull();
    expect(document.querySelector("[data-read-only-line]")).toBeNull();
    unmount();

    useUiStore.getState().openInstalled(null);
    renderInstalled();
    await screen.findByText("requests", { selector: "[data-tool-row] p" });
    expect(selectAll()).not.toBeNull();
    expect(document.querySelector("[data-read-only-line]")).toBeNull();
  });

  it("says it in Chinese", async () => {
    await i18n.changeLanguage("zh-CN");
    useUiStore.getState().openInstalled(PIP);
    renderInstalled();
    await waitFor(() =>
      expect(document.querySelector("[data-read-only-line]")).toHaveTextContent(
        "pip安装的内容只能在这里查看。其中的命令行工具改用pipx或uv安装，就能在这里更新和卸载。",
      ),
    );
  });
});
