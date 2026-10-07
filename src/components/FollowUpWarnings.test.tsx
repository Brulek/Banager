import { afterEach, describe, expect, it, vi } from "vitest";
import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { renderWithProviders } from "../test/setup";
import i18n from "../i18n";
import { queryKeys } from "../lib/queries";
import type { ArtifactKey, FollowUpWarning, ManagerInstance, Snapshot } from "../lib/types";
import { FollowUpWarnings } from "./FollowUpWarnings";

const warnings: FollowUpWarning[] = [
  { OldVersionsNotCleanedUp: { name: "node@22", exit_code: 1 } },
  { NoLongerLinked: { name: "node@22", commands: ["node", "npm"] } },
];

/** node@22 of the Apple-silicon Homebrew, which the snapshot here does not list: its brew is read off the id. */
const node22: ArtifactKey = { instance_id: "brew:/opt/homebrew", kind: "Formula", name: "node@22" };

/** The saved warnings of an update from an earlier launch: its log is gone, so the dialog is all there is. */
function open(artifactKey: ArtifactKey = node22, saved: FollowUpWarning[] = warnings) {
  const view = renderWithProviders(<FollowUpWarnings warnings={saved} opId={null} name="node@22" artifactKey={artifactKey} />);
  fireEvent.click(screen.getByRole("button", { name: i18n.t("updates.progress.viewLogLabel", { name: "node@22" }) }));
  return Object.assign(screen.getByRole("dialog", { name: "node@22" }), { view });
}

afterEach(async () => {
  Object.defineProperty(navigator, "clipboard", { value: undefined, configurable: true });
  await i18n.changeLanguage("en");
});

describe("a saved follow-up warning's log (p1 polish)", () => {
  it("sets the relink command apart as code, with Copy Command, the sentence around it plain", async () => {
    await i18n.changeLanguage("en");
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
    const dialog = open();
    // The command on its own, as the link-fix sheet and the password steps set theirs.
    const group = within(dialog).getByRole("group", { name: "Command to run in Terminal" });
    const code = group.querySelector("code");
    expect(code?.textContent).toBe("/opt/homebrew/bin/brew link --formula --force node@22");
    expect(code?.className.split(" ")).toEqual(expect.arrayContaining(["block", "select-all", "font-mono", "bg-group"]));
    // A line breaks between its words, never inside one: each a box of
    // its own that goes to the next line whole (`unbrokenTokens`), and
    // only the spaces between them loose.
    const tokens = [...(code?.querySelectorAll("[data-command-token]") ?? [])];
    expect(tokens.map((s) => s.textContent)).toEqual(["/opt/homebrew/bin/brew", "link", "--formula", "--force", "node@22"]);
    for (const token of tokens) expect(token.className.split(" ")).toEqual(["inline-block", "max-w-full", "break-words"]);
    const line = tokens[0]?.parentElement;
    expect(line?.textContent).toBe(code?.textContent);
    const loose = [...(line?.childNodes ?? [])].filter((node) => node.nodeType === Node.TEXT_NODE);
    expect(loose.map((node) => node.textContent)).toEqual(Array(tokens.length - 1).fill(" "));
    fireEvent.click(within(dialog).getByRole("button", { name: "Copy Command" }));
    await waitFor(() => expect(writeText).toHaveBeenCalledWith("/opt/homebrew/bin/brew link --formula --force node@22"));
    // The sentence before it and the one after it say no command.
    const before = within(dialog).getByText(
      "node@22 isn't linked back into Terminal, so typing node or npm no longer runs it. To link it back, you can run this command in Terminal.",
    );
    expect(before.tagName).toBe("P");
    expect(before.className).not.toContain("font-mono");
    expect(within(dialog).getByText("If a file is in the way, the command says which.")).toBeInTheDocument();
    // The other warning, a plain sentence too: nothing in the body is a <pre>.
    expect(within(dialog).getByText(i18n.t("brewVersions.logNotCleanedUp")).tagName).toBe("P");
    expect(dialog.querySelector("pre")).toBeNull();
  });

  it("still copies the whole log as the sentences the log says, command included", async () => {
    await i18n.changeLanguage("en");
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
    const dialog = open();
    fireEvent.click(within(dialog).getByRole("button", { name: "Copy Log" }));
    await waitFor(() =>
      expect(writeText).toHaveBeenCalledWith(
        `${i18n.t("brewVersions.logNotCleanedUp")}\nnode@22 isn't linked back into Terminal, so typing node or npm no longer runs it. To link it back, run /opt/homebrew/bin/brew link --formula --force node@22 in Terminal; if a file is in the way, it says which.`,
      ),
    );
  });

  it("is headed as the recorded password stop's dialog is: the tool, and how its update ended with the warning sign", async () => {
    await i18n.changeLanguage("en");
    const dialog = open();
    // Two warnings saved here, and the words count them (r21 C5).
    const subtitle = within(dialog).getByText("Updated with warnings");
    // The attention sign beside the words, as the log of this launch's update shows it.
    const sign = subtitle.parentElement?.querySelector("svg");
    expect(sign).not.toBeNull();
    expect(sign?.getAttribute("class")).toContain("text-warning");
    // Said to a screen reader as the dialog opens, after its name.
    const describedBy = dialog.getAttribute("aria-describedby")?.split(" ") ?? [];
    expect(describedBy.some((id) => document.getElementById(id)?.textContent?.includes("Updated with warnings"))).toBe(true);
    // The sentence over them calls them what the subtitle does: warnings.
    expect(within(dialog).getByText("Only its warnings were saved; the full log is no longer available.")).toBeInTheDocument();
  });

  it.each([
    [
      "zh-CN",
      "node@22没有重新链接到终端，输入node或npm不再运行它。要重新链接，可以在终端里运行下面的命令。",
      "如果有文件挡住，命令会说出是哪个。",
      "要在终端里运行的命令",
      "拷贝命令",
    ],
    [
      "zh-Hant",
      "node@22沒有重新連結到終端機，輸入node或npm不再執行它。要重新連結，可以在終端機裡執行下面的指令。",
      "如果有檔案擋住，指令會說出是哪一個。",
      "要在終端機執行的指令",
      "拷貝指令",
    ],
  ])("says it in %s too, the command apart", async (language, before, after, groupName, copyName) => {
    await i18n.changeLanguage(language);
    const dialog = open();
    expect(within(dialog).getByText(before)).toBeInTheDocument();
    expect(within(dialog).getByText(after)).toBeInTheDocument();
    expect(within(dialog).getByText("已更新，有警告")).toBeInTheDocument();
    expect(within(dialog).getByText(language === "zh-CN" ? "这里只保留了警告，完整日志已不再保留。" : "這裡只保留了警告，完整記錄已不再保留。")).toBeInTheDocument();
    const group = within(dialog).getByRole("group", { name: groupName });
    expect(group.querySelector("code")?.textContent).toBe("/opt/homebrew/bin/brew link --formula --force node@22");
    expect(within(dialog).getByRole("button", { name: copyName })).toBeInTheDocument();
  });

  it.each([
    ["en", "Updated with a warning", "Only its warning was saved; the full log is no longer available.", "Copy Command"],
    ["zh-CN", "已更新，有警告", "这里只保留了警告，完整日志已不再保留。", "拷贝命令"],
  ])(
    "takes the focus itself as it opens, not Copy Command past the warning, and gives it back on Escape, in %s (r27 A2)",
    async (language, subtitle, saved, copyName) => {
      await i18n.changeLanguage(language);
      const user = userEvent.setup();
      // node@22 left unlinked: Copy Command is the dialog's first control.
      renderWithProviders(
        <FollowUpWarnings
          warnings={[{ NoLongerLinked: { name: "node@22", commands: ["node", "npm"] } }]}
          opId={null}
          name="node@22"
          artifactKey={node22}
        />,
      );
      const viewLog = screen.getByRole("button", { name: i18n.t("updates.progress.viewLogLabel", { name: "node@22" }) });
      viewLog.focus();
      await user.keyboard(" ");
      const dialog = await screen.findByRole("dialog", { name: "node@22" });
      // A screen reader reads its name and what describes it, then the warning.
      await waitFor(() => expect(dialog).toHaveFocus());
      expect(within(dialog).getByRole("button", { name: copyName })).not.toHaveFocus();
      expect(dialog).toHaveAccessibleDescription(`${subtitle} ${saved}`);
      await user.keyboard("{Escape}");
      await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
      await waitFor(() => expect(viewLog).toHaveFocus());
    },
  );
});

describe("the relink command names the Homebrew the update was in, not Terminal's brew (r33 T1)", () => {
  /** A saved node@22 warning of the Intel Homebrew, as an Apple-silicon Mac that came through Rosetta keeps one. */
  const intel: ArtifactKey = { instance_id: "brew:/usr/local", kind: "Formula", name: "node@22" };
  const unlinked: FollowUpWarning[] = [{ NoLongerLinked: { name: "node@22", commands: ["node", "npm"] } }];

  function homebrew(id: string, exePath: string): ManagerInstance {
    return {
      id,
      adapter_id: "brew",
      exe_path: exePath,
      prefix: id.slice("brew:".length),
      scope: "System",
      version: "7.0.3",
      answered_at: null,
      unverified_version: null,
      read_only_reason: null,
      status: { unavailable: null, notes: [] },
    };
  }
  function snapshotOf(instances: ManagerInstance[]): Snapshot {
    return {
      generation: 1,
      round: 1,
      detect: "Found",
      instances,
      artifacts: [],
      updates: [],
      refreshed_at: 1789700000,
      stale: false,
      errors: [],
    };
  }

  it("gives /usr/local/bin/brew for a record of brew:/usr/local, in Copy Command and Copy Log alike", async () => {
    await i18n.changeLanguage("en");
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
    const dialog = open(intel, unlinked);
    const group = within(dialog).getByRole("group", { name: "Command to run in Terminal" });
    expect(group.querySelector("code")?.textContent).toBe("/usr/local/bin/brew link --formula --force node@22");
    fireEvent.click(within(dialog).getByRole("button", { name: "Copy Command" }));
    await waitFor(() => expect(writeText).toHaveBeenCalledWith("/usr/local/bin/brew link --formula --force node@22"));
    fireEvent.click(within(dialog).getByRole("button", { name: "Copy Log" }));
    await waitFor(() =>
      expect(writeText).toHaveBeenLastCalledWith(
        "node@22 isn't linked back into Terminal, so typing node or npm no longer runs it. To link it back, run /usr/local/bin/brew link --formula --force node@22 in Terminal; if a file is in the way, it says which.",
      ),
    );
    // Nowhere a bare brew, which in Terminal is the Apple-silicon one.
    expect(dialog.textContent).not.toMatch(/(^|[^/])brew link/);
  });

  it("takes the program from the snapshot's instance where it lists it, each token quoted as the preview quotes it", async () => {
    await i18n.changeLanguage("zh-CN");
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
    const custom: ArtifactKey = { instance_id: "brew:/Users/Alice Smith/homebrew", kind: "Formula", name: "node@22" };
    const dialog = open(custom, unlinked);
    act(() => {
      dialog.view.queryClient.setQueryData(
        queryKeys.snapshot,
        snapshotOf([
          homebrew("brew:/opt/homebrew", "/opt/homebrew/bin/brew"),
          homebrew("brew:/Users/Alice Smith/homebrew", "/Users/Alice Smith/homebrew/bin/brew"),
        ]),
      );
    });
    const group = within(dialog).getByRole("group", { name: "要在终端里运行的命令" });
    await waitFor(() =>
      expect(group.querySelector("code")?.textContent).toBe("'/Users/Alice Smith/homebrew/bin/brew' link --formula --force node@22"),
    );
    fireEvent.click(within(dialog).getByRole("button", { name: "拷贝日志" }));
    await waitFor(() =>
      expect(writeText).toHaveBeenCalledWith(
        "node@22没有重新链接到终端，输入node或npm不再运行它。要重新链接，可以在终端里运行'/Users/Alice Smith/homebrew/bin/brew' link --formula --force node@22；如果有文件挡住，它会说出是哪个。",
      ),
    );
  });
});
