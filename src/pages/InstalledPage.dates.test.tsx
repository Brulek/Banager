import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, screen, waitFor } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import { WithToolbarSlot } from "../test/toolbarSlot";
import { InstalledPage } from "./InstalledPage";
import i18n from "../i18n";
import type { InstalledArtifact, ManagerInstance, Settings, Snapshot } from "../lib/types";
import { NO_FACTS, NO_SIZES } from "../lib/types";

// The Installed page's 「按安装日期」 / "By Date Installed" (src/lib/installedDates.ts).

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

const npm: ManagerInstance = {
  ...brew,
  id: "npm:/opt/homebrew/lib/node_modules",
  adapter_id: "npm",
  exe_path: "/opt/homebrew/bin/npm",
  prefix: "/opt/homebrew/lib/node_modules",
  version: "10.9.8",
};

/** Seconds since the epoch of a local calendar day at noon, as `installed_at` is. */
const day = (year: number, month: number, date: number) =>
  Math.floor(new Date(year, month - 1, date, 12).getTime() / 1000);
// This year's, so the column leaves the year out; any day of it is in the past
// or today, as an install is.
const THIS_YEAR = new Date().getFullYear();

function artifact(instance: ManagerInstance, name: string, installed_at: number | null): InstalledArtifact {
  return {
    key: { instance_id: instance.id, kind: instance === brew ? "Formula" : "Package", name },
    display_name: name,
    version: "1.0.0",
    reason: "Requested",
    description: `${name} blurb`,
    homepage: null,
    size_bytes: null,
    installed_at,
    path: null,
    auto_updates: false,
    uninstall_blocked: null,
    facts: NO_FACTS,
  };
}

// jq and wget installed in one `brew install` (the same second): by name.
// node@22 last year. npm reports no dates: after them, by name.
const jq = artifact(brew, "jq", day(THIS_YEAR, 1, 2));
const wget = artifact(brew, "wget", day(THIS_YEAR, 1, 2));
const node = artifact(brew, "node@22", day(2025, 4, 19));
const git = artifact(brew, "git", day(THIS_YEAR, 1, 1));
const yarn = artifact(npm, "yarn", null);
const pnpm = artifact(npm, "pnpm", null);
// A component another formula brought in: folded, after the rows, under any sort.
const oniguruma: InstalledArtifact = { ...artifact(brew, "oniguruma", day(THIS_YEAR, 1, 3)), reason: "Dependency" };

const snapshot: Snapshot = {
  generation: 1,
  round: 4,
  detect: "Found",
  instances: [brew, npm],
  artifacts: [git, jq, node, oniguruma, wget, pnpm, yarn],
  updates: [],
  refreshed_at: 1789700000,
  stale: false,
  errors: [],
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

beforeEach(() => {
  mockInvoke.mockReset();
  vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockImplementation(function (this: HTMLElement) {
    return this.getAttribute("data-index") === null ? 600 : 56;
  });
  vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(800);
  mockInvoke.mockImplementation((cmd: string) => {
    if (cmd === "get_snapshot") return Promise.resolve(snapshot);
    if (cmd === "get_settings") return Promise.resolve(settings);
    if (cmd === "list_operations") return Promise.resolve([]);
    if (cmd === "get_sizes") return Promise.resolve(NO_SIZES);
    return Promise.resolve(undefined);
  });
});

afterEach(async () => {
  vi.restoreAllMocks();
  await i18n.changeLanguage("en");
});

function render() {
  return renderWithProviders(
    <WithToolbarSlot>
      <InstalledPage />
    </WithToolbarSlot>,
  );
}

const rowNames = () =>
  [...document.querySelectorAll("[data-tool-row]")].map((row) => row.querySelector("p")?.textContent ?? "");
const cells = () =>
  [...document.querySelectorAll("[data-tool-row]")].map((row) => row.querySelector("[data-date-cell]")?.textContent);

describe("the Installed page, By Date Installed", () => {
  it("orders the newest first, one second's tools by name, a tool with no date last, the components still folded", async () => {
    render();
    await screen.findByText("yarn", { selector: "[data-tool-row] p" });
    expect(rowNames()).toEqual(["git", "jq", "node@22", "pnpm", "wget", "yarn"]);
    const sortBy = screen.getByRole("combobox", { name: "Sort Order" });
    fireEvent.change(sortBy, { target: { value: "date" } });
    await waitFor(() => expect(rowNames()).toEqual(["jq", "wget", "git", "node@22", "pnpm", "yarn"]));
    expect(sortBy.parentElement?.firstElementChild).toHaveTextContent(/^By Date Installed$/);
    // The fold line stays after every row: the component is not sorted in.
    const list = document.querySelector("[data-list]") as HTMLElement;
    expect(list.textContent).toMatch(/yarn.*1 more/s);
  });

  it("shows the day in the version's place, without the year this year, 「—」 muted and unread for none", async () => {
    render();
    await screen.findByText("yarn", { selector: "[data-tool-row] p" });
    fireEvent.change(screen.getByRole("combobox", { name: "Sort Order" }), { target: { value: "date" } });
    await waitFor(() => expect(document.querySelector("[data-date-cell]")).not.toBeNull());
    expect(cells()).toEqual(["Jan 2", "Jan 2", "Jan 1", "Apr 19, 2025", "—", "—"]);
    expect(document.querySelectorAll("[data-date-cell].text-muted")).toHaveLength(2);
    expect(document.querySelector("[data-size-cell]")).toBeNull();
    const dashes = [...document.querySelectorAll("[data-date-cell]")].filter((cell) => cell.textContent === "—");
    for (const dash of dashes) expect(dash).toHaveAttribute("aria-hidden", "true");
    // The row's name says the day with its term, as By Size says the size.
    const names = [...document.querySelectorAll("[data-tool-row]")].map((row) => row.getAttribute("aria-label"));
    expect(names).toEqual([
      "jq, Date installed: Jan 2",
      "wget, Date installed: Jan 2",
      "git, Date installed: Jan 1",
      "node@22, Date installed: Apr 19, 2025",
      "pnpm",
      "yarn",
    ]);
    // By name again: the versions, and the name alone.
    fireEvent.change(screen.getByRole("combobox", { name: "Sort Order" }), { target: { value: "name" } });
    await waitFor(() => expect(document.querySelector("[data-date-cell]")).toBeNull());
    expect(document.querySelector("[data-tool-row]")).toHaveAttribute("aria-label", "git");
  });

  it("says it in Chinese", async () => {
    await i18n.changeLanguage("zh-CN");
    render();
    await screen.findByText("yarn", { selector: "[data-tool-row] p" });
    const sortBy = screen.getByRole("combobox", { name: "排序方式" });
    fireEvent.change(sortBy, { target: { value: "date" } });
    await waitFor(() => expect(document.querySelector("[data-date-cell]")).not.toBeNull());
    expect(sortBy.parentElement?.firstElementChild).toHaveTextContent(/^按安装日期$/);
    expect(cells()).toEqual(["1月2日", "1月2日", "1月1日", "2025年4月19日", "—", "—"]);
    const names = [...document.querySelectorAll("[data-tool-row]")].map((row) => row.getAttribute("aria-label"));
    expect(names.slice(0, 4)).toEqual([
      "jq, 安装于1月2日",
      "wget, 安装于1月2日",
      "git, 安装于1月1日",
      "node@22, 安装于2025年4月19日",
    ]);
  });
});
