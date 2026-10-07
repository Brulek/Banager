import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import i18n from "../i18n";
import type { Settings, Snapshot, SystemFacts } from "../lib/types";
import { NO_FACTS } from "../lib/types";
import { SettingsPage } from "../pages/SettingsPage";
import { renderWithProviders } from "../test/setup";
import { DiagnosticsRows } from "./DiagnosticsRows";

const mockInvoke = vi.mocked(invoke);

const SNAPSHOT: Snapshot = {
  generation: 2,
  round: 2,
  detect: "Found",
  instances: [
    {
      id: "brew:/opt/homebrew",
      adapter_id: "brew",
      exe_path: "/opt/homebrew/bin/brew",
      prefix: "/opt/homebrew",
      scope: "User",
      version: "7.0.3",
      answered_at: null,
      unverified_version: null,
      read_only_reason: null,
      status: { unavailable: null, notes: [] },
    },
  ],
  artifacts: [
    {
      key: { instance_id: "brew:/opt/homebrew", kind: "Formula", name: "jq" },
      display_name: "jq",
      version: "1.8.2",
      reason: "Requested",
      description: null,
      homepage: null,
      size_bytes: null,
      installed_at: null,
      path: null,
      auto_updates: false,
      uninstall_blocked: null,
      facts: NO_FACTS,
    },
  ],
  updates: [],
  refreshed_at: 1_790_000_000,
  stale: false,
  errors: [],
};

const FACTS: SystemFacts = {
  macos_version: "27.0",
  chip: "Apple M2 Pro",
  arch: "aarch64",
  login_path: true,
  path_dirs: ["/opt/homebrew/bin", "~/.local/bin"],
  sources: [{ instance_id: "brew:/opt/homebrew", exe_path: "/opt/homebrew/bin/brew" }],
};

const SETTINGS: Settings = {
  language: "System",
  show_technical_details: false,
  auto_check: false,
  notify_updates: false,
  include_self_updating: false,
  skipped_versions: [],
  ignored_updates: [],
} as unknown as Settings;

let writeText: ReturnType<typeof vi.fn<(text: string) => Promise<void>>>;

beforeEach(() => {
  mockInvoke.mockReset();
  mockInvoke.mockImplementation(async (cmd: string) => {
    if (cmd === "get_snapshot") return SNAPSHOT;
    if (cmd === "get_system_facts") return FACTS;
    if (cmd === "get_sizes") return null;
    if (cmd === "get_settings") return SETTINGS;
    throw new Error(`unexpected command ${cmd}`);
  });
  writeText = vi.fn(async () => {});
});

afterEach(() => {
  Object.defineProperty(navigator, "clipboard", { value: undefined, configurable: true });
});

/**
 * A user, and this test's clipboard: set after `userEvent.setup()`, which
 * puts a clipboard of its own on `navigator`.
 */
function setup() {
  const user = userEvent.setup();
  Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
  return user;
}

/** Until the snapshot and the facts are in, so the text has them. */
async function loaded(): Promise<void> {
  await waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("get_system_facts"));
  await act(async () => {});
}

describe("DiagnosticsRows", () => {
  it("copies the text without the tools, and says it did", async () => {
    const user = setup();
    const { container } = renderWithProviders(<DiagnosticsRows />);
    await loaded();

    await user.click(screen.getByRole("button", { name: "Copy Diagnostic Info" }));

    expect(writeText).toHaveBeenCalledTimes(1);
    const text = writeText.mock.calls[0][0];
    expect(text.startsWith("Diagnostic info\nTime: ")).toBe(true);
    expect(text).toContain("\nmacOS: 27.0\nChip: Apple M2 Pro\n");
    expect(text).toContain("\nHomebrew\n  Version: 7.0.3\n  Location: /opt/homebrew/bin/brew\n  Status: OK\n  Tools: 1\n");
    expect(text).not.toContain("jq 1.8.2");
    expect(container.querySelector("[data-copy-diagnostics]")?.previousElementSibling).toHaveTextContent("Copied");
  });

  it("adds each source's tools only while its checkbox is ticked, which starts off", async () => {
    const user = setup();
    renderWithProviders(<DiagnosticsRows />);
    await loaded();
    const box = screen.getByRole("checkbox", { name: "Include the list of tools" });
    expect(box).not.toBeChecked();

    await user.click(box);
    await user.click(screen.getByRole("button", { name: "Copy Diagnostic Info" }));
    expect(writeText.mock.calls[0][0]).toContain("\n  Tools: 1\n    jq 1.8.2\n");

    await user.click(box);
    await user.click(screen.getByRole("button", { name: "Copy Diagnostic Info" }));
    expect(writeText.mock.calls[1][0]).not.toContain("jq 1.8.2");
  });

  it("says it could not copy when the clipboard is not there or refuses", async () => {
    const user = setup();
    const { container } = renderWithProviders(<DiagnosticsRows />);
    await loaded();
    const status = () => container.querySelector("[data-copy-diagnostics]")?.previousElementSibling;

    writeText.mockRejectedValueOnce(new Error("NotAllowedError"));
    await user.click(screen.getByRole("button", { name: "Copy Diagnostic Info" }));
    await waitFor(() => expect(status()).toHaveTextContent("Couldn't copy"));

    Object.defineProperty(navigator, "clipboard", { value: undefined, configurable: true });
    await user.click(screen.getByRole("button", { name: "Copy Diagnostic Info" }));
    expect(status()).toHaveTextContent("Couldn't copy");
  });

  it("is in Chinese in a Chinese window, text and all", async () => {
    await i18n.changeLanguage("zh-CN");
    try {
      const user = setup();
      renderWithProviders(<DiagnosticsRows />);
      await loaded();
      expect(screen.getByText("诊断信息")).toBeInTheDocument();
      expect(screen.getByRole("checkbox", { name: "包括工具清单" })).not.toBeChecked();

      await user.click(screen.getByRole("button", { name: "拷贝诊断信息" }));

      const text = writeText.mock.calls[0][0];
      expect(text.startsWith("诊断信息\n时间：")).toBe(true);
      expect(text).toContain("\n界面语言：简体中文\n");
      expect(screen.getByText("已拷贝")).toBeInTheDocument();
    } finally {
      await i18n.changeLanguage("en");
    }
  });
});

describe("Settings' Diagnostics group", () => {
  it("ends with one row: Diagnostic info, its checkbox before Copy Diagnostic Info, and what the text holds under the group", async () => {
    renderWithProviders(<SettingsPage />);
    const diagnostics = await screen.findByRole("region", { name: "Diagnostics" });
    const group = diagnostics.querySelector("h2 + div") as HTMLElement;
    const copyRow = group.lastElementChild as HTMLElement;
    expect(copyRow).toHaveTextContent("Diagnostic info");
    const controls = [...copyRow.querySelectorAll("input, button")];
    expect(controls.map((control) => control.getAttribute("type"))).toEqual(["checkbox", "button"]);
    expect(controls[0]).not.toBeChecked();
    expect(controls[1]).toHaveTextContent("Copy Diagnostic Info");
    const footnote = group.nextElementSibling as HTMLElement;
    expect(footnote).toHaveTextContent(
      "Paste it to whoever is helping you. It lists your tools only when the checkbox is selected, and includes the error details of any source that can't run, ran into an error or isn't responding.",
    );
    // The details behind its ⓘ.
    fireEvent.click(within(footnote).getByRole("button", { name: "Details: Diagnostic info" }));
    // What a source's error details may quote, and what is hidden in them:
    // the logins Banager recognizes, not every one a tool can print.
    expect(
      screen.getByText(
        /error details are the tool's own words and can quote its settings files: user names and passwords that Banager recognizes in web addresses and proxy settings are hidden, and everything else is copied as the tool wrote it\./,
      ),
    ).toBeInTheDocument();
  });

  it("says in each language that only the user names and passwords it recognizes are hidden", () => {
    // r9 k1's skeptic: some forms of a login are left as written
    // (docs/what-we-run.md, "What a tool prints about a login"), so no
    // language says flatly that every one is hidden.
    for (const [language, hedged, flat] of [
      ["en", "user names and passwords that Banager recognizes in web addresses and proxy settings are hidden", "user names and passwords in web addresses and proxy settings are hidden"],
      ["zh-CN", "地址和代理设置里能识别出的用户名和密码会隐藏", "地址和代理设置里的用户名和密码会隐藏"],
      ["zh-Hant", "網址和代理設定裡能識別出的使用者名稱和密碼會隱藏", "網址和代理設定裡的使用者名稱和密碼會隱藏"],
    ]) {
      const detail = i18n.getFixedT(language)("clarity.diagnosticsDetail");
      expect(detail).toContain(hedged);
      expect(detail).not.toContain(flat);
    }
  });

  it("names, in each language, every state of a source whose error details the text includes (r31 E4)", () => {
    // `NoAnswer.diagnostic` is kept for all three ways a source gives no
    // answer (crates/banager-core/src/runner/no_answer.rs), and
    // `diagnosticsText` adds it whatever the kind. The window names two of
    // them in words of their own (`saidNoAnswer`, src/lib/noAnswer.ts) and
    // keeps "isn't responding" for the one that ran out of time: the
    // footnote names all three as the notices' titles do, so a source that
    // "ran into an error" does not read as left out.
    for (const language of ["en", "zh-CN", "zh-Hant"]) {
      const t = i18n.getFixedT(language);
      const footnote = t("diagnostics.footnote");
      for (const key of ["noAnswer.title.CouldNotStart", "noAnswer.title.ExitedWithError", "sourceNotice.unreachable.title"]) {
        const state = t(key, { source: "" }).trim();
        expect(state.length, `${language} ${key}`).toBeGreaterThan(0);
        expect(footnote, `${language} ${key}`).toContain(state);
      }
    }
  });
});
