import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import App from "../App";
import i18n from "../i18n";
import en from "../i18n/en.json";
import zhCN from "../i18n/zh-CN.json";
import zhHant from "../i18n/zh-Hant.json";
import { FAQ_ITEMS, openFaqSheet, useFaqSheet, type FaqId } from "../lib/faq";
import { RECENT_DAYS } from "../lib/history";
import { useUiStore } from "../store/ui";
import { fakeMenuBar } from "../test/menuBar";
import { renderWithProviders } from "../test/setup";
import { FaqSheet } from "./FaqSheet";

const mockInvoke = vi.mocked(invoke);

const BREW = "brew:/opt/homebrew";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");

beforeEach(() => {
  useFaqSheet.setState({ open: false });
  // Somewhere else, with a source, a 「显示」 choice on each page, a search and a sort
  // picked, so each 查看 is seen to set what it promises.
  useUiStore.setState({
    page: "overview",
    installedFilter: BREW,
    installedShow: "ai",
    installedSort: "name",
    query: "jq",
    updatesShow: "ai",
  });
});

/** The questions in the sheet's order, with their words in `locale`. */
function questionsIn(locale: typeof en | typeof zhCN): { id: FaqId; question: string; answer: string }[] {
  return FAQ_ITEMS.map(({ id }) => ({ id, ...locale.faq.questions[id] }));
}

/** Each question as the open sheet shows it: its heading and its answer. */
function shown(dialog: HTMLElement): { id: string | null; question: string; answer: string }[] {
  return [...dialog.querySelectorAll("section[data-faq]")].map((section) => ({
    id: section.getAttribute("data-faq"),
    question: section.querySelector("h3")?.textContent ?? "",
    answer: section.querySelector("p")?.textContent ?? "",
  }));
}

describe("FaqSheet", () => {
  it.each([
    ["en", en, ["For an update or uninstall you choose here, the confirmation", "migrations or renames", "install, move or uninstall packages without a preview or confirmation"]],
    ["zh-CN", zhCN, ["在这里选择更新或卸载时", "迁移或重命名", "未经预览或确认就安装、移动或卸载软件包"]],
    ["zh-Hant", zhHant, ["在這裡選擇更新或解除安裝時", "移轉或重新命名", "未經預覽或確認就安裝、移動或解除安裝套件"]],
  ] as const)("discloses Homebrew's unconfirmed refresh changes in %s", async (language, locale, disclosures) => {
    await i18n.changeLanguage(language);
    try {
      renderWithProviders(<FaqSheet />);
      act(() => openFaqSheet());
      const dialog = await screen.findByRole("dialog");
      const answer = shown(dialog).find(({ id }) => id === "changesMac")?.answer;
      expect(answer).toBe(locale.faq.questions.changesMac.answer);
      for (const disclosure of disclosures) expect(answer).toContain(disclosure);
    } finally {
      await i18n.changeLanguage("en");
    }
  });

  it("answers all ten questions in English, each under its question, Done focused", async () => {
    renderWithProviders(<FaqSheet />);
    expect(screen.queryByRole("dialog")).toBeNull();

    act(() => openFaqSheet());

    const dialog = await screen.findByRole("dialog", { name: "Common Questions" });
    expect(FAQ_ITEMS).toHaveLength(10);
    expect(shown(dialog)).toEqual(questionsIn(en));
    expect(within(dialog).getAllByRole("heading", { level: 3 }).map((heading) => heading.textContent)).toEqual([
      "Terminal says a command isn't found. What can I do?",
      "Why can't some tools be updated here?",
      "What does “Installed twice” mean, and what should I do?",
      "Why is my Mac password needed? Can it be typed for me here?",
      "What stays behind after uninstalling, and where?",
      "Does this app change my Mac?",
      "What's in Other Programs?",
      "How accurate are the sizes?",
      "Why are some updates marked “Major update”?",
      "When does the automatic check stop?",
    ]);
    expect(within(dialog).getByRole("button", { name: "Done" })).toHaveFocus();
  });

  it("answers all ten in Chinese", async () => {
    await i18n.changeLanguage("zh-CN");
    try {
      renderWithProviders(<FaqSheet />);
      act(() => openFaqSheet());
      const dialog = await screen.findByRole("dialog", { name: "常见问题" });
      expect(shown(dialog)).toEqual(questionsIn(zhCN));
      expect(within(dialog).getAllByRole("heading", { level: 3 }).map((heading) => heading.textContent)).toEqual([
        "终端里说找不到命令，怎么办？",
        "为什么有的工具不能在这里更新？",
        "“装了两份”是什么意思，该怎么办？",
        "为什么要输入Mac的密码？这里能代我输入吗？",
        "卸载后哪些东西会留下，在哪里？",
        "此App会改动我的Mac吗？",
        "“其他程序”里的是什么？",
        "占用空间的数字准吗？",
        "为什么有些更新标着“大版本更新”？",
        "自动检查什么时候会停？",
      ]);
      expect(within(dialog).getAllByRole("button", { name: /^查看：/ })).toHaveLength(8);
      expect(within(dialog).getByRole("button", { name: "完成" })).toHaveFocus();
    } finally {
      await i18n.changeLanguage("en");
    }
  });

  it("keeps every answer to two to four sentences, in both languages", () => {
    for (const { id, answer } of questionsIn(zhCN)) {
      const count = answer.split("。").filter((part) => part !== "").length;
      expect(count, `zh ${id}`).toBeGreaterThanOrEqual(2);
      expect(count, `zh ${id}`).toBeLessThanOrEqual(4);
    }
    for (const { id, answer } of questionsIn(en)) {
      // A sentence ends at ".", "?" or "!" before a space or the end; "3.x" and "4.0" do not end one.
      const count = answer.split(/[.?!](?:\s|$)/).filter((part) => part.trim() !== "").length;
      expect(count, `en ${id}`).toBeGreaterThanOrEqual(2);
      expect(count, `en ${id}`).toBeLessThanOrEqual(4);
    }
  });

  it.each([
    ["notFound", { page: "installed", installedFilter: null, installedShow: "notOnPath", query: "" }],
    ["cantUpdate", { page: "updates", updatesShow: "all" }],
    ["twins", { page: "installed", installedFilter: null, installedShow: "twins", query: "" }],
    ["leftBehind", { page: "installed", installedFilter: null, installedShow: "all", query: "" }],
    ["otherPrograms", { page: "unknown" }],
    ["sizes", { page: "installed", installedFilter: null, installedShow: "all", installedSort: "size", query: "" }],
    ["majorUpdate", { page: "updates", updatesShow: "all" }],
    ["autoCheck", { page: "settings" }],
  ] as const)("closes on 查看 of “%s” and opens where its answer points", async (id, state) => {
    renderWithProviders(<FaqSheet />);
    act(() => openFaqSheet());
    const dialog = await screen.findByRole("dialog", { name: "Common Questions" });
    const question = en.faq.questions[id].question;
    // Named for where it goes, never a bare "Show" (walk-3 W3-5).
    const label = {
      notFound: "Show in Installed",
      cantUpdate: "Show in Updates",
      twins: "Show in Installed",
      leftBehind: "Show in Installed",
      otherPrograms: "Show in Other Programs",
      sizes: "Show in Installed",
      majorUpdate: "Show in Updates",
      autoCheck: "Show in Settings",
    }[id];
    const view = within(dialog).getByRole("button", { name: `${label}: ${question}` });
    expect(view).toHaveTextContent(new RegExp(`^${label}$`));

    fireEvent.click(view);

    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(useFaqSheet.getState().open).toBe(false);
    expect(useUiStore.getState()).toMatchObject(state);
  });

  it.each([
    ["en", en, "Update History", "View Log"],
    ["zh-CN", zhCN, "最近的更新记录", "查看日志"],
    ["zh-Hant", zhHant, "最近的更新記錄", "查看記錄"],
  ] as const)(
    "sends a password stop to the button by its own name, View Steps, and says an update keeps it after a restart, in %s (r26 D4)",
    (_language, locale, history, viewLog) => {
      const answer = locale.faq.questions.password.answer;
      // The button every password stop offers: the operation bar's, an
      // update's row, a batch uninstall's result row, and -- after a
      // restart, when no log is left -- the row and Update History's line.
      expect(answer).toContain(locale.needsPassword.viewSteps);
      expect(answer).not.toContain(viewLog);
      expect(answer).toContain(history);
      expect(locale.updates.justUpdated.title).toBe(history);
    },
  );

  it.each([
    ["en", en, "the password can't be typed here", "for 30 days"],
    ["zh-CN", zhCN, "这里无法输入密码", "30天内"],
    ["zh-Hant", zhHant, "這裡無法輸入密碼", "30天內"],
  ] as const)(
    "names the password as what can't be typed here, and says how long View Steps stays and that Clear History takes it out of Update History, in %s (r26 D4 skeptic)",
    (_language, locale, cantType, days) => {
      const answer = locale.faq.questions.password.answer;
      // "it can't be typed here" after "the confirmation says so" read as
      // the confirmation; the Chinese always named the password.
      expect(answer).toContain(cantType);
      // After a restart the row keeps View Steps only for `RECENT_DAYS`
      // (src/lib/history.ts), and Clear History hides Update History's
      // line while the row keeps it (src/lib/passwordRecovery.ts).
      expect(RECENT_DAYS).toBe(30);
      expect(answer).toContain(days);
      expect(answer).toContain(locale.updates.justUpdated.clear);
    },
  );

  it.each([
    ["en", en, "After Banager is quit and reopened, an update's View Steps"],
    ["zh-CN", zhCN, "退出此App再重新打开后，更新的“查看步骤”"],
    ["zh-Hant", zhHant, "結束此App再重新開啟後，更新的「查看步驟」"],
  ] as const)(
    "says it is this app that is quit and reopened, not the Homebrew app the answer is about, in %s (r30 Z3)",
    (_language, locale, restart) => {
      // 「結束後再開啟」 with no object, in a paragraph about an app
      // installed with Homebrew, read as quitting that app; 結束 alone is
      // also "finish", right after 「完成後回到這裡」.
      expect(locale.faq.questions.password.answer).toContain(restart);
    },
  );

  it("has the README say the same as the FAQ of a password stop after a restart, in English and Chinese (r26 D4 skeptic)", () => {
    const readme = readFileSync(path.join(ROOT, "README.md"), "utf-8");
    // The one list item each block has on it, from its dash to the next.
    const item = (start: string) => {
      const from = readme.indexOf(start);
      expect(from, start).toBeGreaterThanOrEqual(0);
      const to = readme.indexOf("\n- ", from + start.length);
      return readme.slice(from, to === -1 ? undefined : to);
    };
    const english = item("- When a Homebrew update or uninstall stopped because it needed your Mac's password").replace(/\s+/g, " ");
    expect(english).toContain(`**${en.needsPassword.viewSteps}**`);
    expect(english).toContain(`**${en.updates.justUpdated.title}**`);
    expect(english).toContain("for 30 days");
    expect(english).toContain(`unless you press **${en.updates.justUpdated.clear}**`);
    const chinese = item("- Homebrew 的更新或卸载因为要输入 Mac 密码而停下时").replace(/\n\s*/g, "");
    expect(chinese).toContain(`「${zhCN.needsPassword.viewSteps}」`);
    expect(chinese).toContain(`“${zhCN.updates.justUpdated.title}”`);
    expect(chinese).toContain("30 天内");
    expect(chinese).toContain(`没有按“${zhCN.updates.justUpdated.clear}”的话`);
  });

  it("offers no 查看 where there is no one place to act: the password, and what the app changes", async () => {
    renderWithProviders(<FaqSheet />);
    act(() => openFaqSheet());
    const dialog = await screen.findByRole("dialog", { name: "Common Questions" });
    for (const id of ["password", "changesMac"] as const) {
      const section = dialog.querySelector(`section[data-faq="${id}"]`) as HTMLElement;
      expect(within(section).queryByRole("button")).toBeNull();
    }
    // Every other question has exactly one, and only these.
    const withView = FAQ_ITEMS.filter((item) => item.view !== null).map((item) => item.id);
    expect(withView).toEqual([
      "notFound",
      "cantUpdate",
      "twins",
      "leftBehind",
      "otherPrograms",
      "sizes",
      "majorUpdate",
      "autoCheck",
    ]);
    for (const id of withView) {
      const section = dialog.querySelector(`section[data-faq="${id}"]`) as HTMLElement;
      expect(within(section).getAllByRole("button")).toHaveLength(1);
    }
  });

  it("names each question's region by its question, and each 查看 by the question it is for", async () => {
    renderWithProviders(<FaqSheet />);
    act(() => openFaqSheet());
    const dialog = await screen.findByRole("dialog", { name: "Common Questions" });
    const regions = within(dialog).getAllByRole("region");
    expect(regions.map((region) => region.getAttribute("data-faq"))).toEqual(FAQ_ITEMS.map((item) => item.id));
    for (const { id, question } of questionsIn(en)) {
      expect(within(dialog).getByRole("region", { name: question }).getAttribute("data-faq")).toBe(id);
    }
    const views = within(dialog).getAllByRole("button", { name: /^Show in [^:]+: / });
    // Each one tells apart from the others by its name, which starts with
    // the words it shows: where it goes.
    expect(new Set(views.map((button) => button.getAttribute("aria-label"))).size).toBe(views.length);
    for (const button of views) {
      expect(button).toHaveTextContent(/^Show in /);
      expect(button.getAttribute("aria-label")?.startsWith(`${button.textContent}: `)).toBe(true);
    }
  });

  it("closes with Done and with Escape", async () => {
    renderWithProviders(<FaqSheet />);
    act(() => openFaqSheet());
    fireEvent.click(within(await screen.findByRole("dialog")).getByRole("button", { name: "Done" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(useFaqSheet.getState().open).toBe(false);
    // Done goes nowhere.
    expect(useUiStore.getState().page).toBe("overview");

    act(() => openFaqSheet());
    fireEvent.keyDown(await screen.findByRole("dialog"), { key: "Escape" });
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(useFaqSheet.getState().open).toBe(false);
  });

  it("opens from Help's Common Questions over the page that is showing", async () => {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "list_operations") return [];
      return undefined;
    });
    const menu = fakeMenuBar();
    renderWithProviders(<App />);
    await screen.findByRole("heading", { level: 1, name: "Overview" });
    expect(screen.queryByRole("dialog", { name: "Common Questions" })).toBeNull();

    menu.choose("commonQuestions");

    expect(await screen.findByRole("dialog", { name: "Common Questions" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { level: 1, name: "Overview", hidden: true })).toBeInTheDocument();
  });
});
