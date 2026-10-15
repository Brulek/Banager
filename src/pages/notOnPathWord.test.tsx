import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import { WithToolbarSlot } from "../test/toolbarSlot";
import { InstalledPage } from "./InstalledPage";
import i18n from "../i18n";
import type { ArtifactKey, CommandFact, InstalledArtifact, Settings, Snapshot } from "../lib/types";
import { NO_FACTS } from "../lib/types";

/**
 * The Installed page's row word for a tool Terminal does not find:
 * 「终端里找不到」, the Show menu's word for the same tools, only where one
 * of the tool's commands has a `NotOnPath` verdict, behind its ⓘ the
 * details' own words; in the details, the 「在终端里输入」 group says it,
 * not 「状态」 again.
 */

const mockInvoke = vi.mocked(invoke);

const BREW = "brew:/opt/homebrew";
const PIPX = "pipx:/opt/homebrew/bin";
const GROK = "standalone-grok";

function instance(adapterId: string, id: string): Snapshot["instances"][number] {
  return {
    id,
    adapter_id: adapterId,
    exe_path: `/opt/homebrew/bin/${adapterId}`,
    prefix: "/opt/homebrew",
    scope: "User",
    version: "1.0.0",
    status: { unavailable: null, notes: [] },
    answered_at: null,
    unverified_version: null,
    read_only_reason: null,
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

const notFound = (dir: string): CommandFact["state"] => ({ NotOnPath: { dir } });

const wget: ArtifactKey = { instance_id: BREW, kind: "Formula", name: "wget" };
const libuv: ArtifactKey = { instance_id: BREW, kind: "Formula", name: "libuv" };
const httpie: ArtifactKey = { instance_id: PIPX, kind: "Tool", name: "httpie" };
const grok: ArtifactKey = { instance_id: GROK, kind: "Binary", name: "grok" };
const black: ArtifactKey = { instance_id: PIPX, kind: "Tool", name: "black" };

const settings: Settings = {
  language: "System",
  show_technical_details: false,
  ignored_updates: [],
  skipped_versions: [],
  include_self_updating: false,
  auto_check: false,
  notify_updates: false,
};

let artifacts: InstalledArtifact[];

beforeEach(() => {
  artifacts = [
    // Runs: no word.
    artifact(wget, [{ name: "wget", state: "Runs" }]),
    // Nothing judged: no word either -- not "not found".
    artifact(libuv, [{ name: "uvx-like", state: null }]),
    // One folder: the details' line and its ⓘ.
    artifact(
      grok,
      [
        { name: "agent", state: notFound("~/.grok/bin") },
        { name: "grok", state: notFound("~/.grok/bin") },
      ],
      "Grok Build",
    ),
    // One command found, one in two folders Terminal does not search.
    artifact(httpie, [
      { name: "http", state: "Runs" },
      { name: "https", state: notFound("~/.local/bin") },
      { name: "httpie", state: notFound("~/bin") },
    ]),
    // One command found, the other in one folder: "it" is not the whole tool.
    artifact(black, [
      { name: "black", state: "Runs" },
      { name: "blackd", state: notFound("~/.local/bin") },
    ]),
  ];
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
        instances: [instance("brew", BREW), instance("pipx", PIPX), instance("standalone-grok", GROK)],
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

function rowOf(name: string): HTMLElement {
  const row = [...document.querySelectorAll<HTMLElement>("[data-tool-row]")].find(
    (candidate) => candidate.querySelector("p")?.textContent === name,
  );
  if (row === undefined) throw new Error(`no row ${name}`);
  return row;
}

function wordOf(name: string): string | null {
  return rowOf(name).querySelector("[data-status]")?.textContent ?? null;
}

describe("the Installed row's word for a tool Terminal can't find", () => {
  it("says Not Found in Terminal only where a command has a NotOnPath verdict", async () => {
    renderInstalled();
    await screen.findByText("wget", { selector: "[data-tool-row] p" });
    expect(wordOf("Grok Build")).toBe("Not Found in Terminal");
    expect(wordOf("httpie")).toBe("Not Found in Terminal");
    // A command that runs, or one Banager said nothing about, is no reason.
    expect(rowOf("wget").querySelector("[data-status-word], [data-status] button")).toBeNull();
    expect(rowOf("libuv").querySelector("[data-status-word], [data-status] button")).toBeNull();
  });

  it("says behind its ⓘ where the command is and what to do, in the details' words", async () => {
    renderInstalled();
    await screen.findByText("wget", { selector: "[data-tool-row] p" });
    const button = within(rowOf("Grok Build")).getByRole("button", { name: "Not Found in Terminal: Grok Build" });
    fireEvent.click(button);
    const panel = await screen.findByText(/^Terminal can't find it: it's in ~\/\.grok\/bin/);
    expect(panel).toBeInTheDocument();
    expect(screen.getByText(/^Terminal looks for commands only in certain folders/)).toBeInTheDocument();
  });

  it("names no folder for commands in more than one", async () => {
    renderInstalled();
    await screen.findByText("wget", { selector: "[data-tool-row] p" });
    fireEvent.click(within(rowOf("httpie")).getByRole("button", { name: "Not Found in Terminal: httpie" }));
    expect(await screen.findByText(/^One of its commands is in a folder Terminal doesn't search/)).toBeInTheDocument();
    expect(screen.queryByText(/^Terminal can't find it/)).not.toBeInTheDocument();
  });

  it("does not say the whole tool is in a folder when only one of its commands is", async () => {
    renderInstalled();
    await screen.findByText("wget", { selector: "[data-tool-row] p" });
    expect(wordOf("black")).toBe("Not Found in Terminal");
    fireEvent.click(within(rowOf("black")).getByRole("button", { name: "Not Found in Terminal: black" }));
    expect(await screen.findByText(/^One of its commands is in a folder Terminal doesn't search/)).toBeInTheDocument();
    expect(screen.queryByText(/^Terminal can't find it/)).not.toBeInTheDocument();
  });

  it("is said by the details' Type in Terminal group, not their Status again", async () => {
    renderInstalled();
    await screen.findByText("wget", { selector: "[data-tool-row] p" });
    fireEvent.click(rowOf("Grok Build").querySelector("[data-row-open]") as HTMLElement);
    const inspector = await waitFor(() => {
      const aside = document.querySelector("aside");
      if (aside === null) throw new Error("no details");
      return aside as HTMLElement;
    });
    await within(inspector).findByText(/^Terminal can't find it/);
    expect(within(inspector).queryByText("Not Found in Terminal")).not.toBeInTheDocument();
  });

  it("says it in Chinese with the Show menu's word", async () => {
    await i18n.changeLanguage("zh-CN");
    renderInstalled();
    await screen.findByText("wget", { selector: "[data-tool-row] p" });
    await waitFor(() => expect(wordOf("Grok Build")).toBe("终端里找不到"));
  });
});
