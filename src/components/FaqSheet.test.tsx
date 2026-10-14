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
    // r31 E6: what Homebrew does when it renames or moves something, in
    // plain words -- "during migrations or renames" (「迁移或重命名时」)
    // left the reader to guess whose migration it was. It can change other
    // software too, so not "that software" (n1 skeptic).
    ["en", en, ["For an update or uninstall you choose here, the confirmation", "when Homebrew renames something you installed with it, or moves it to another kind of package", "it can install, move or uninstall software by itself, without a preview or confirmation"]],
    ["zh-CN", zhCN, ["在这里选择更新或卸载时", "把你用它装的软件改名或换成另一种软件包时", "未经预览或确认就自行安装、移动或卸载软件；"]],
    ["zh-Hant", zhHant, ["在這裡選擇更新或解除安裝時", "把你用它安裝的軟體重新命名或換成另一種套件時", "未經預覽或確認就自行安裝、移動或解除安裝軟體；"]],
  ] as const)("discloses Homebrew's unconfirmed refresh changes in %s", async (language, locale, disclosures) => {
    await i18n.changeLanguage(language);
    try {
      renderWithProviders(<FaqSheet />);
      act(() => openFaqSheet());
      const dialog = await screen.findByRole("dialog");
      const answer = shown(dialog).find(({ id }) => id === "changesMac")?.answer;
      expect(answer).toBe(locale.faq.questions.changesMac.answer);
      for (const disclosure of disclosures) expect(answer).toContain(disclosure);
      expect(answer).not.toMatch(/migrat|迁移|移轉/i);
      // n1 skeptic: moving a formula to a cask runs a bare `brew cleanup`,
      // which removes old versions of every formula, and `install
      // --overwrite` can install dependencies (reporter.rb:261, :288-295),
      // so what Homebrew changes is not only "that software".
      expect(answer).not.toMatch(/that software|这个软件|這個軟體/);
    } finally {
      await i18n.changeLanguage("en");
    }
  });

  it.each([
    // r20 R56-4: a uv update takes back a command in ~/.local/bin that
    // pipx linked last (`Warning::TakesBackCommand`, uv.rs), so updating
    // the copy Terminal doesn't use can change the one it runs -- and the
    // confirmation says so first (`uvLinks.takesBack`).
    ["en", en, "Updating the copy Terminal doesn't use doesn't change the one it runs, unless the confirmation says the update points the command back to the copy being updated."],
    ["zh-CN", zhCN, "更新终端用不到的那一份，不会改变终端里运行的那一份，除非确认时说明这次更新会把命令改回要更新的这一份。"],
    ["zh-Hant", zhHant, "更新終端機用不到的那一份，不會改變終端機裡執行的那一份，除非確認時說明這次更新會把指令改回要更新的這一份。"],
  ] as const)("says in %s an update can point a shared command back to the copy it updates", (_language, locale, sentence) => {
    expect(locale.faq.questions.twins.answer).toContain(sentence);
  });

  it.each([
    // r24 R79-4: WebKit, which draws the window, keeps data and a cache for
    // the app in its persistent default store (wry 0.55.1 leaves incognito
    // off), outside the app's own folder, so "only three files" was not the
    // whole of what stays on the Mac.
    ["en", en, "The app itself writes three files", "WebKit, the part of macOS that draws the window, can also keep data and a cache for it", /only three files/],
    ["zh-CN", zhCN, "此App自己保存三个文件", "显示窗口的macOS组件WebKit还可能", /只写三个文件/],
    ["zh-Hant", zhHant, "此App自己儲存三個檔案", "顯示視窗的macOS元件WebKit還可能", /只寫入三個檔案/],
  ] as const)("tells the app's own three files from WebKit's data and cache, in %s", (_language, locale, own, webkit, gone) => {
    const answer = locale.faq.questions.changesMac.answer;
    expect(answer).toContain(own);
    expect(answer).toContain(webkit);
    expect(answer).not.toMatch(gone);
    for (const place of ["~/Library/Application Support/com.brulek.banager", "~/Library/WebKit", "~/Library/Caches", "com.brulek.banager"]) {
      expect(answer).toContain(place);
    }
  });

  it.each([
    // r24 R79-2: Banager was Canager, whose builds kept settings.json in
    // com.brulek.canager's folder, which nothing here reads (the app data
    // folder follows tauri.conf.json's identifier). Canager kept no
    // history, so settings.json is the one file to bring over, and never
    // over a settings.json Banager already has.
    ["en", en, ["Canager is this app's old name", "quit this app and Canager", "Go > Go to Folder", "leaving the original there", "only if that folder has no settings.json yet", "don't replace it", "to use the old settings instead, first move that file into a new, empty folder, then paste"]],
    ["zh-CN", zhCN, ["Canager是此App以前的名字", "退出此App和Canager", "“前往”>“前往文件夹”", "原文件留在原处", "只有其中还没有settings.json时才粘贴", "不要替换它", "要改用旧的设置，请先把这个文件移到一个新建的空文件夹里，再粘贴"]],
    ["zh-Hant", zhHant, ["Canager是此App以前的名字", "結束此App和Canager", "「前往」>「前往檔案夾」", "原始檔案留在原處", "只有其中還沒有settings.json時才貼上", "不要取代它", "要改用舊的設定，請先把這個檔案移到一個新的空檔案夾裡，再貼上"]],
  ] as const)("says how to bring Canager's settings over without replacing this app's own, in %s", async (language, _locale, phrases) => {
    await i18n.changeLanguage(language);
    try {
      renderWithProviders(<FaqSheet />);
      act(() => openFaqSheet());
      const dialog = await screen.findByRole("dialog");
      const section = dialog.querySelector('section[data-faq="canagerData"]') as HTMLElement | null;
      expect(section, "a question about Canager's settings").not.toBeNull();
      const answer = section?.querySelector("p")?.textContent ?? "";
      for (const phrase of phrases) expect(answer).toContain(phrase);
      expect(answer).toContain("~/Library/Application Support/com.brulek.canager");
      expect(answer).toContain("~/Library/Application Support/com.brulek.banager");
      // Copying from Canager's folder comes before pasting into this app's.
      expect(answer.indexOf("com.brulek.canager")).toBeLessThan(answer.indexOf("com.brulek.banager"));
      expect(answer).not.toContain("history.json");
      if (section !== null) expect(within(section).queryByRole("button")).toBeNull();
    } finally {
      await i18n.changeLanguage("en");
    }
  });

  it.each([
    // r81 R81-1: uv exits 0 having left out tools it can't read; a notice
    // says so, names the command that says which and why, and the rest stay.
    ["en", en, ["uv leaves out tools it can't read", "the Python they use is gone", "the other tools are still listed", "which tools uv left out and why", "choose Check Again"]],
    ["zh-CN", zhCN, ["uv会略过无法读取的工具", "Python已不存在", "其余工具仍会列出", "uv略过了哪些工具以及原因", "“重新检查”"]],
    ["zh-Hant", zhHant, ["uv會略過無法讀取的工具", "Python已不存在", "其餘工具仍會列出", "uv略過了哪些工具以及原因", "「重新檢查」"]],
  ] as const)("says why some uv tools are missing and what the notice gives, in %s", (_language, locale, phrases) => {
    const answer = locale.faq.questions.missingTools.answer;
    for (const phrase of phrases) expect(answer).toContain(phrase);
    expect(answer).not.toMatch(/fix it for you|自动修复|自動修復/);
    expect(answer).toContain(locale.header.checkAgain);
  });

  it("answers all fifteen questions in English, each under its question, Done focused", async () => {
    renderWithProviders(<FaqSheet />);
    expect(screen.queryByRole("dialog")).toBeNull();

    act(() => openFaqSheet());

    const dialog = await screen.findByRole("dialog", { name: "Common Questions" });
    expect(FAQ_ITEMS).toHaveLength(15);
    expect(shown(dialog)).toEqual(questionsIn(en));
    expect(within(dialog).getAllByRole("heading", { level: 3 }).map((heading) => heading.textContent)).toEqual([
      "Terminal says a command isn't found. What can I do?",
      "Why are some uv tools missing?",
      "Why can't some tools be updated here?",
      "What does “Installed twice” mean, and what should I do?",
      "Why is my Mac password needed? Can it be typed for me here?",
      "What stays behind after uninstalling, and where?",
      "Why does other software keep a tool from being uninstalled?",
      "Does this app change my Mac?",
      "What's in Other Programs?",
      "How accurate are the sizes?",
      "Why are some updates marked “Major update”?",
      "Why does an update say it has a warning?",
      "Does updating a background service restart it?",
      "When does the automatic check stop?",
      "Where are my settings from Canager?",
    ]);
    expect(within(dialog).getByRole("button", { name: "Done" })).toHaveFocus();
  });

  it("answers all fifteen in Chinese", async () => {
    await i18n.changeLanguage("zh-CN");
    try {
      renderWithProviders(<FaqSheet />);
      act(() => openFaqSheet());
      const dialog = await screen.findByRole("dialog", { name: "常见问题" });
      expect(shown(dialog)).toEqual(questionsIn(zhCN));
      expect(within(dialog).getAllByRole("heading", { level: 3 }).map((heading) => heading.textContent)).toEqual([
        "终端里说找不到命令，怎么办？",
        "为什么有些uv工具没有显示？",
        "为什么有的工具不能在这里更新？",
        "“装了两份”是什么意思，该怎么办？",
        "为什么要输入Mac的密码？这里能代我输入吗？",
        "卸载后哪些东西会留下，在哪里？",
        "为什么其他软件会让工具不能卸载？",
        "此App会改动我的Mac吗？",
        "“其他程序”里的是什么？",
        "占用空间的数字准吗？",
        "为什么有些更新标着“大版本更新”？",
        "为什么更新后显示有警告？",
        "更新后台服务会重新启动它吗？",
        "自动检查什么时候会停？",
        "Canager的设置在哪里？",
      ]);
      expect(within(dialog).getAllByRole("button", { name: /^查看：/ })).toHaveLength(11);
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
    ["missingTools", { page: "installed", installedFilter: null, installedShow: "all", query: "" }],
    ["cantUpdate", { page: "updates", updatesShow: "all" }],
    ["twins", { page: "installed", installedFilter: null, installedShow: "twins", query: "" }],
    ["leftBehind", { page: "installed", installedFilter: null, installedShow: "all", query: "" }],
    ["otherPrograms", { page: "unknown" }],
    ["sizes", { page: "installed", installedFilter: null, installedShow: "all", installedSort: "size", query: "" }],
    ["majorUpdate", { page: "updates", updatesShow: "all" }],
    ["updatedWithWarning", { page: "updates", updatesShow: "all" }],
    ["serviceAfterUpdate", { page: "updates", updatesShow: "all" }],
    ["autoCheck", { page: "settings" }],
  ] as const)("closes on 查看 of “%s” and opens where its answer points", async (id, state) => {
    renderWithProviders(<FaqSheet />);
    act(() => openFaqSheet());
    const dialog = await screen.findByRole("dialog", { name: "Common Questions" });
    const question = en.faq.questions[id].question;
    // Named for where it goes, never a bare "Show" (walk-3 W3-5).
    const label = {
      notFound: "Show in Installed",
      missingTools: "Show in Installed",
      cantUpdate: "Show in Updates",
      twins: "Show in Installed",
      leftBehind: "Show in Installed",
      otherPrograms: "Show in Other Programs",
      sizes: "Show in Installed",
      majorUpdate: "Show in Updates",
      updatedWithWarning: "Show in Updates",
      serviceAfterUpdate: "Show in Updates",
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

  it("offers no 查看 where there is no one place to act: the password, what keeps a tool from being uninstalled, what the app changes, and Canager's settings", async () => {
    renderWithProviders(<FaqSheet />);
    act(() => openFaqSheet());
    const dialog = await screen.findByRole("dialog", { name: "Common Questions" });
    for (const id of ["password", "cantUninstall", "changesMac", "canagerData"] as const) {
      const section = dialog.querySelector(`section[data-faq="${id}"]`) as HTMLElement;
      expect(within(section).queryByRole("button")).toBeNull();
    }
    // Every other question has exactly one, and only these.
    const withView = FAQ_ITEMS.filter((item) => item.view !== null).map((item) => item.id);
    expect(withView).toEqual([
      "notFound",
      "missingTools",
      "cantUpdate",
      "twins",
      "leftBehind",
      "otherPrograms",
      "sizes",
      "majorUpdate",
      "updatedWithWarning",
      "serviceAfterUpdate",
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
