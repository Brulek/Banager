import { afterEach, describe, expect, it, vi } from "vitest";
import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { renderWithProviders } from "../test/setup";
import i18n from "../i18n";
import type { FollowUpWarning } from "../lib/types";
import { FollowUpWarnings } from "./FollowUpWarnings";

const warnings: FollowUpWarning[] = [
  { OldVersionsNotCleanedUp: { name: "node@22", exit_code: 1 } },
  { NoLongerLinked: { name: "node@22", commands: ["node", "npm"] } },
];

/** The saved warnings of an update from an earlier launch: its log is gone, so the dialog is all there is. */
function open() {
  renderWithProviders(<FollowUpWarnings warnings={warnings} opId={null} name="node@22" />);
  fireEvent.click(screen.getByRole("button", { name: i18n.t("updates.progress.viewLogLabel", { name: "node@22" }) }));
  return screen.getByRole("dialog", { name: "node@22" });
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
    expect(code?.textContent).toBe("brew link --formula --force node@22");
    expect(code?.className.split(" ")).toEqual(expect.arrayContaining(["block", "select-all", "font-mono", "bg-group"]));
    // A line breaks between its words, never inside one.
    expect([...(code?.querySelectorAll("span.whitespace-nowrap") ?? [])].map((s) => s.textContent)).toEqual([
      "brew",
      "link",
      "--formula",
      "--force",
      "node@22",
    ]);
    fireEvent.click(within(dialog).getByRole("button", { name: "Copy Command" }));
    await waitFor(() => expect(writeText).toHaveBeenCalledWith("brew link --formula --force node@22"));
    // The sentence before it and the one after it say no command.
    const before = within(dialog).getByText(
      "node@22 isn't linked back into Terminal, so typing node, npm no longer runs it. To link it back, you can run this command in Terminal.",
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
        `${i18n.t("brewVersions.logNotCleanedUp")}\n${i18n.t("kegLinks.logNoLongerLinked", { name: "node@22", commands: "node, npm" })}`,
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
      "node@22没有重新接到终端里，输入node、npm不再运行它。要接回去，可以在终端里运行下面的命令。",
      "如果有文件挡住，命令会说出是哪个。",
      "要在终端里运行的命令",
      "拷贝命令",
    ],
    [
      "zh-Hant",
      "node@22沒有重新接到終端機裡，輸入node、npm不再執行它。要接回去，可以在終端機裡執行下面的指令。",
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
    expect(group.querySelector("code")?.textContent).toBe("brew link --formula --force node@22");
    expect(within(dialog).getByRole("button", { name: copyName })).toBeInTheDocument();
  });
});
