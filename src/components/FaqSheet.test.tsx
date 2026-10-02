import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import App from "../App";
import i18n from "../i18n";
import en from "../i18n/en.json";
import zhCN from "../i18n/zh-CN.json";
import { FAQ_ITEMS, openFaqSheet, useFaqSheet, type FaqId } from "../lib/faq";
import { useUiStore } from "../store/ui";
import { fakeMenuBar } from "../test/menuBar";
import { renderWithProviders } from "../test/setup";
import { FaqSheet } from "./FaqSheet";

const mockInvoke = vi.mocked(invoke);

const BREW = "brew:/opt/homebrew";

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
