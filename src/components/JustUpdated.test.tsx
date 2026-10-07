import { describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, screen, within } from "@testing-library/react";
import { renderWithProviders } from "../test/setup";
import { BUTTON } from "./ui/controls";
import i18n from "../i18n";
import {
  JUST_UPDATED_SHOWN,
  JustUpdated,
  endingOfOutcome,
  endingOfRecord,
  finishedText,
  justUpdatedOps,
  type JustUpdatedEntry,
} from "./JustUpdated";
import type { OpSummary } from "../lib/types";
import { failureCause } from "../lib/failureCause";

function upgrade(id: number, name: string, fields: Partial<OpSummary> = {}): OpSummary {
  return {
    id,
    kind: "Upgrade",
    instance_id: "brew:/opt/homebrew",
    artifact_kind: "Formula",
    name,
    status: "Done",
    outcome: "Succeeded",
    argv_preview: [],
    cancel_policy: "KillThenReconcile",
    ...fields,
  };
}

const none = { shownInRows: new Set<number>(), cleared: [], finishedAt: {} };

/**
 * What the ⓘ after a line's ending says, once pressed: its why, which a
 * keyboard and a screen reader reach as a pointer does (decision I21e) --
 * never a tooltip only the pointer shows.
 */
function why(words: HTMLElement): string {
  expect(words).not.toHaveAttribute("title");
  const info = within(words).getByRole("button");
  fireEvent.click(info);
  return document.getElementById(info.getAttribute("aria-controls") ?? "")?.textContent ?? "";
}

describe("justUpdatedOps", () => {
  it("takes each tool's newest operation: a finished update that worked, failed or asks to be checked", () => {
    const operations = [
      upgrade(1, "jq"),
      upgrade(5, "jq"),
      upgrade(2, "wget"),
      upgrade(6, "wget", { kind: "Uninstall" }),
      upgrade(3, "glib", { outcome: { Failed: { exit_code: 1, summary: "no bottle", cause: failureCause("no bottle") } } }),
      upgrade(4, "gh", { status: "Running", outcome: null }),
      upgrade(7, "fd", { outcome: "Unconfirmed" }),
      upgrade(8, "bat", { outcome: { NeedsAttention: "UnchangedAfterUpgrade" } }),
      upgrade(9, "tree", { outcome: "Cancelled" }),
      upgrade(10, "node", { outcome: { BanagerFailed: { HomebrewStillUpdating: { minutes: 10 } } } }),
    ];
    expect(justUpdatedOps(operations, none).map((op) => op.id)).toEqual([10, 8, 7, 5, 3]);
  });

  it("lists an update that failed and then worked as the one that worked, and one that worked and then failed as failed", () => {
    const failed = { outcome: { Failed: { exit_code: 1, summary: "curl: (6) Could not resolve host", cause: failureCause("curl: (6) Could not resolve host") } } } as const;
    const operations = [upgrade(1, "jq", failed), upgrade(2, "jq"), upgrade(3, "wget"), upgrade(4, "wget", failed)];
    expect(justUpdatedOps(operations, none).map((op) => [op.name, op.id])).toEqual([
      ["wget", 4],
      ["jq", 2],
    ]);
  });

  it("leaves out what a row still shows and what Clear took off", () => {
    const operations = [upgrade(1, "jq"), upgrade(2, "wget"), upgrade(3, "glib")];
    const listed = justUpdatedOps(operations, { ...none, shownInRows: new Set([1]), cleared: [2] });
    expect(listed.map((op) => op.id)).toEqual([3]);
  });

  it("puts the newest finished first, and the ones it did not see finish after them, newest first", () => {
    const operations = [upgrade(1, "jq"), upgrade(2, "wget"), upgrade(3, "glib"), upgrade(4, "gh")];
    const listed = justUpdatedOps(operations, { ...none, finishedAt: { 3: 1000, 2: 3000 } });
    expect(listed.map((op) => op.id)).toEqual([2, 3, 4, 1]);
  });
});

describe("endingOfOutcome and endingOfRecord", () => {
  it("say how an update ended, with a failure's cause, and nothing for one cancelled or not finished", () => {
    expect(endingOfOutcome("Succeeded")).toEqual({ kind: "succeeded" });
    expect(endingOfOutcome({ Failed: { exit_code: 1, summary: "sudo: a terminal is required to read the password", cause: failureCause("sudo: a terminal is required to read the password") } })).toEqual({
      kind: "failed",
      cause: "needsPassword",
    });
    expect(endingOfOutcome({ Failed: { exit_code: 1, summary: "Error: no bottle", cause: failureCause("Error: no bottle") } })).toEqual({ kind: "failed", cause: null, detail: "no bottle" });
    expect(endingOfOutcome({ Failed: { exit_code: 1, summary: " ", cause: null } })).toEqual({ kind: "failed", cause: null });
    expect(endingOfOutcome({ BanagerFailed: "Panicked" })).toEqual({ kind: "failed", cause: null });
    expect(endingOfOutcome("Unconfirmed")).toEqual({ kind: "attention", outcome: "Unconfirmed" });
    expect(endingOfOutcome({ NeedsAttention: "UnchangedAfterUpgrade" })).toEqual({
      kind: "attention",
      outcome: { NeedsAttention: "UnchangedAfterUpgrade" },
    });
    expect(endingOfOutcome("Cancelled")).toBeNull();
    expect(endingOfOutcome(null)).toBeNull();

    expect(endingOfRecord("Succeeded")).toEqual({ kind: "succeeded" });
    expect(endingOfRecord({ Failed: { cause: "network" } })).toEqual({ kind: "failed", cause: "network" });
    expect(endingOfRecord({ Failed: { cause: null } })).toEqual({ kind: "failed", cause: null });
    expect(endingOfRecord("Unconfirmed")).toEqual({ kind: "attention", outcome: "Unconfirmed" });
    expect(endingOfRecord({ NeedsAttention: "GoneAfterUpgrade" })).toEqual({
      kind: "attention",
      outcome: { NeedsAttention: "GoneAfterUpgrade" },
    });
    expect(endingOfRecord("Cancelled")).toBeNull();
  });

  it("say how an update already at its target was done, and keep a failure's line where no cause is named (r6 y3-batch)", () => {
    expect(endingOfOutcome("Succeeded", "ByEarlierUpdate")).toEqual({ kind: "succeeded", already: "ByEarlierUpdate" });
    expect(endingOfOutcome("Succeeded", null)).toEqual({ kind: "succeeded" });
    expect(endingOfRecord("Succeeded", "BeforeItsTurn")).toEqual({ kind: "succeeded", already: "BeforeItsTurn" });
    // Only a success says it.
    expect(endingOfRecord({ NeedsAttention: "UnchangedAfterUpgrade" }, "BeforeItsTurn")).toEqual({
      kind: "attention",
      outcome: { NeedsAttention: "UnchangedAfterUpgrade" },
    });
    expect(endingOfRecord({ Failed: { cause: "appMissing" } })).toEqual({ kind: "failed", cause: "appMissing" });
    expect(endingOfRecord({ Failed: { cause: null, detail: "SHA256 mismatch" } })).toEqual({
      kind: "failed",
      cause: null,
      detail: "SHA256 mismatch",
    });
    // This window's own, the line read off its summary as the history keeps it.
    expect(endingOfOutcome({ Failed: { exit_code: 1, summary: "==> Fetching\nError: SHA256 mismatch", cause: null } })).toEqual({
      kind: "failed",
      cause: null,
      detail: "SHA256 mismatch",
    });
  });
});

describe("a cause whose words point at the tool's (review of r6 y3-batch, finding 4)", () => {
  it("keeps the tool's first error line beside a conflict, something missing and an unsupported Mac", () => {
    expect(endingOfRecord({ Failed: { cause: "conflict", detail: "Could not symlink bin/npm" } })).toEqual({
      kind: "failed",
      cause: "conflict",
      detail: "Could not symlink bin/npm",
    });
    expect(
      endingOfOutcome({ Failed: { exit_code: 127, summary: "env: node: No such file or directory", cause: "notFound" } }),
    ).toEqual({ kind: "failed", cause: "notFound", detail: "env: node: No such file or directory" });
    // A cause whose words say it all keeps none, as the history keeps none.
    expect(
      endingOfOutcome({ Failed: { exit_code: 1, summary: 'Error: Failed to download resource "jq"', cause: "network" } }),
    ).toEqual({ kind: "failed", cause: "network" });
  });
});

describe("finishedText", () => {
  const morning = new Date(2026, 8, 28, 8, 5).getTime();

  it("says the time for today, and the date and time in full in its title", () => {
    const now = new Date(2026, 8, 28, 23, 59).getTime();
    expect(finishedText(morning, now, "zh-CN")).toEqual({
      text: new Intl.DateTimeFormat("zh-CN", { timeStyle: "short" }).format(morning),
      title: new Intl.DateTimeFormat("zh-CN", { dateStyle: "medium", timeStyle: "short" }).format(morning),
      today: true,
    });
  });

  it("says the date for another day: 9月28日, Sep 28", () => {
    const nextDay = new Date(2026, 8, 29, 0, 1).getTime();
    expect(finishedText(morning, nextDay, "en")).toMatchObject({ text: "Sep 28", today: false });
    expect(finishedText(morning, nextDay, "zh-CN").text).toBe("9月28日");
  });
});

describe("JustUpdated", () => {
  const entry = {
    id: "op:4",
    opId: 4,
    key: { instance_id: "brew:/opt/homebrew", kind: "Formula" as const, name: "git" },
    adapterId: "brew",
    sourceLabel: "Homebrew",
    name: "git",
    version: "2.55.1",
    finishedAt: Date.now(),
    verified: false,
    ending: { kind: "succeeded" },
  } satisfies JustUpdatedEntry;

  it("is a grouped container under its 13 bold title, with a small grey Clear beside the title", () => {
    const onClear = vi.fn();
    renderWithProviders(<JustUpdated entries={[entry]} onClear={onClear} />);

    const section = screen.getByRole("region", { name: "Update History" });
    const title = within(section).getByRole("heading", { name: "Update History" });
    expect(title).toHaveClass("text-title");
    const clear = within(section).getByRole("button", { name: "Clear the Update History list" });
    expect(clear.className).toBe(BUTTON.small.grey);
    // Beside the title, not at the far end.
    expect(clear.parentElement).toBe(title.parentElement);
    expect(title.parentElement?.className).not.toMatch(/justify-between/);
    // No card: no edge and no white; the group's fill and corners.
    expect(section.className).not.toMatch(/border|bg-surface/);
    const list = within(section).getByRole("list");
    expect(list).toHaveClass("bg-group", "rounded-group");
    fireEvent.click(clear);
    expect(onClear).toHaveBeenCalledTimes(1);
  });

  it("draws each tool as a quiet 28 line: a 20 icon, the name in 13, then 11 for the rest", () => {
    renderWithProviders(<JustUpdated entries={[entry]} onClear={() => {}} />);

    const line = screen.getByRole("listitem");
    expect(line).toHaveClass("h-7");
    // git has no logo in the test's pack: the program tile, at 20 (I8).
    expect(line.querySelector("[data-program-tile]")?.className).toMatch(/h-5 w-5/);
    expect(within(line).getByText("git")).toHaveClass("text-body");
    expect(within(line).getByText("2.55.1")).toHaveClass("text-small", "text-muted");
    const done = within(line).getByText("Updated");
    expect(done).toHaveClass("text-small", "text-foreground");
    expect(done.querySelector("svg")).toHaveAttribute("width", "12");
    expect(line.querySelector("time")?.parentElement).toHaveClass("text-small", "text-muted");
  });

  it("says each line's source to a screen reader, so two copies of one tool are two lines apart", () => {
    const npm = {
      ...entry,
      id: "op:5",
      opId: 5,
      key: { ...entry.key, instance_id: "npm:/opt/homebrew" },
      adapterId: "npm",
      sourceLabel: "npm",
    };
    renderWithProviders(<JustUpdated entries={[entry, npm]} onClear={() => {}} />);

    const lines = screen.getAllByRole("listitem");
    expect(lines.map((line) => line.querySelector("[data-just-updated-source]")?.textContent)).toEqual([
      "Homebrew",
      "npm",
    ]);
    // Heard, not seen: the avatar's mark says it in sight.
    expect(within(lines[1]).getByText("npm")).toHaveClass("sr-only");
  });

  it("shows in sight which source a tool with no logo came from: the mark on its tile's corner (I8)", () => {
    // tokei, with no logo of its own, updated from Cargo and from
    // Homebrew: the two tiles alike but for the source's mark on each.
    const cargo = {
      ...entry,
      id: "op:6",
      opId: 6,
      key: { instance_id: "cargo:/Users/you/.cargo", kind: "Binary" as const, name: "tokei" },
      adapterId: "cargo",
      sourceLabel: "Cargo",
      name: "tokei",
    };
    const brew = { ...entry, key: { ...entry.key, name: "tokei" }, name: "tokei" };
    renderWithProviders(<JustUpdated entries={[cargo, brew]} onClear={() => {}} />);

    const marks = screen.getAllByRole("listitem").map((line) => {
      const tile = line.querySelector("[data-program-tile]");
      expect(tile).not.toBeNull();
      const badge = line.querySelector("[data-source-badge]") as HTMLElement;
      expect(badge).not.toBeNull();
      // Drawn, not said: the source is the line's sr-only text.
      expect(badge.closest("[aria-hidden='true']")).not.toBeNull();
      return badge.textContent;
    });
    // This test's empty pack: each source's initial.
    expect(marks).toEqual(["C", "H"]);
  });

  it("says Today and the time for one that finished today, and Updated, as the row says it, with what Banager read behind its ⓘ (walk-3 W3-19)", () => {
    renderWithProviders(<JustUpdated entries={[{ ...entry, verified: true }]} onClear={() => {}} />);

    const line = screen.getByRole("listitem");
    const time = line.querySelector("time");
    expect(time?.textContent).toBe(
      `Today ${new Intl.DateTimeFormat("en", { timeStyle: "short" }).format(entry.finishedAt)}`,
    );
    const done = within(line).getByText("Updated");
    // Named by its line's tool, as each line has one.
    expect(within(done).getByRole("button", { name: "Details: git" })).toHaveAttribute("aria-expanded", "false");
    expect(why(done)).toBe("The installed version was read before and after the update, and it had changed.");
    expect(done.querySelector("svg")).toHaveAttribute("width", "12");
    // One word for an update that worked, read or not: no "Update confirmed".
    expect(within(line).queryByText("Update confirmed")).toBeNull();
  });

  it("says for a model that the model, not a version, was read and had changed", () => {
    const model = {
      ...entry,
      key: { instance_id: "ollama:http://127.0.0.1:11434", kind: "Model" as const, name: "qwen3:8b" },
      adapterId: "ollama",
      name: "qwen3:8b",
      version: null,
      verified: true,
    };
    renderWithProviders(<JustUpdated entries={[model]} onClear={() => {}} />);
    expect(why(screen.getByText("Updated"))).toBe("The model was read before and after the update, and it had changed.");
  });

  it(`shows the newest ${JUST_UPDATED_SHOWN} and folds the rest under a line that shows them`, () => {
    const many = Array.from({ length: JUST_UPDATED_SHOWN + 3 }, (_, i) => ({
      ...entry,
      id: `op:${i}`,
      opId: i,
      name: `tool${i}`,
      key: { ...entry.key, name: `tool${i}` },
    }));
    renderWithProviders(<JustUpdated entries={many} onClear={() => {}} />);

    expect(screen.getAllByRole("listitem")).toHaveLength(JUST_UPDATED_SHOWN);
    expect(screen.getByText(`tool${JUST_UPDATED_SHOWN - 1}`)).toBeInTheDocument();
    expect(screen.queryByText(`tool${JUST_UPDATED_SHOWN}`)).toBeNull();
    // Says what pressing it does, and of what: walk-2 W2-12.
    const more = screen.getByRole("button", { name: "Show 3 More" });
    expect(more).toHaveAttribute("aria-expanded", "false");
    expect(more).toHaveClass("text-small", "text-muted");

    fireEvent.click(more);
    expect(screen.getAllByRole("listitem")).toHaveLength(JUST_UPDATED_SHOWN + 3);
    const fewer = screen.getByRole("button", { name: "Show Fewer" });
    expect(fewer).toHaveAttribute("aria-expanded", "true");
    fireEvent.click(fewer);
    expect(screen.getAllByRole("listitem")).toHaveLength(JUST_UPDATED_SHOWN);
  });

  it("says 「未能更新」 with the cause beside a red sign, and what to do about it behind its ⓘ", () => {
    const failed: JustUpdatedEntry[] = [
      { ...entry, id: "a", version: null, ending: { kind: "failed", cause: "network" } },
      { ...entry, id: "b", name: "wget", version: null, ending: { kind: "failed", cause: null } },
    ];
    renderWithProviders(<JustUpdated entries={failed} onClear={() => {}} />);

    const [withCause, without] = screen.getAllByRole("listitem");
    const words = within(withCause).getByText("Couldn't update: Connection failed");
    expect(words).toHaveClass("text-small", "text-foreground");
    expect(why(words)).toBe("The connection failed. Check your internet connection, then try again.");
    expect(words.querySelector("svg")).toHaveClass("text-danger");
    expect(words.querySelector("svg")).toHaveAttribute("width", "12");
    const plain = within(without).getByText("Couldn't update");
    expect(plain).not.toHaveAttribute("title");
    expect(within(plain).queryByRole("button")).toBeNull();
    expect(plain.querySelector("svg")).toHaveClass("text-danger");
    // Neither says it updated, nor shows a version it might be read as updated to.
    for (const line of [withCause, without]) {
      expect(within(line).queryByText(/^Updated$/)).toBeNull();
      expect(within(line).queryByText("2.55.1")).toBeNull();
    }
  });

  it("says an update an earlier one already did as done, and why behind its ⓘ (r6 y3-batch)", () => {
    const lines: JustUpdatedEntry[] = [
      { ...entry, id: "a", name: "libpng", ending: { kind: "succeeded", already: "ByEarlierUpdate" } },
      { ...entry, id: "b", name: "pcre2", verified: true, ending: { kind: "succeeded", already: "BeforeItsTurn" } },
    ];
    renderWithProviders(<JustUpdated entries={lines} onClear={() => {}} />);
    const [byEarlier, before] = screen.getAllByRole("listitem");
    const words = within(byEarlier).getByText("Done by an earlier update");
    expect(why(words)).toBe(
      "When its turn came it was already up to date: an earlier update had updated it as well.",
    );
    expect(words.querySelector("svg")).toHaveClass("text-success");
    expect(within(byEarlier).getByText("2.55.1")).toBeInTheDocument();
    expect(why(within(before).getByText("Already up to date"))).toBe(
      "When its turn came it was already up to date, so its own command changed nothing.",
    );
  });

  it("says why an update failed after the window that watched it closed: its cause, or the tool's line (r6 y3-batch)", () => {
    const failed: JustUpdatedEntry[] = [
      { ...entry, id: "a", name: "Claudebar", version: null, ending: { kind: "failed", cause: "appMissing" } },
      {
        ...entry,
        id: "b",
        name: "OnyX",
        version: null,
        ending: { kind: "failed", cause: null, detail: "SHA256 mismatch" },
      },
    ];
    renderWithProviders(<JustUpdated entries={failed} onClear={() => {}} />);
    const [missing, other] = screen.getAllByRole("listitem");
    expect(why(within(missing).getByText("Couldn't update: The app isn't where it was installed"))).toBe(
      "The app isn't where it was installed; it may have been moved to the Trash or deleted. To keep using it, put it back in Applications and try again, or uninstall it if you no longer need it.",
    );
    expect(why(within(other).getByText("Couldn't update"))).toBe("Reason: SHA256 mismatch");
  });

  it("says the tool's line behind the ⓘ of a cause that points at it, and never sends a person to a log that is gone", () => {
    const failed: JustUpdatedEntry[] = [
      {
        ...entry,
        id: "a",
        name: "node",
        version: null,
        ending: { kind: "failed", cause: "notFound", detail: "env: node: No such file or directory" },
      },
    ];
    renderWithProviders(<JustUpdated entries={failed} onClear={() => {}} />);
    const [missing] = screen.getAllByRole("listitem");
    const text = why(within(missing).getByText("Couldn't update: Something it needs is missing"));
    expect(text).toBe("Something it needs is missing. The error says what. The error: env: node: No such file or directory");
    expect(text).not.toMatch(/log/i);
  });

  it("says what did not add up, in plain words beside an orange sign, not the row's short 「结果不符」", () => {
    const toCheck: JustUpdatedEntry[] = [
      {
        ...entry,
        id: "a",
        version: null,
        ending: { kind: "attention", outcome: { NeedsAttention: "UnchangedAfterUpgrade" } },
      },
      { ...entry, id: "b", name: "wget", version: null, ending: { kind: "attention", outcome: "Unconfirmed" } },
    ];
    renderWithProviders(<JustUpdated entries={toCheck} onClear={() => {}} />);

    const [unchanged, unconfirmed] = screen.getAllByRole("listitem");
    // Said as what it means, that it did not update, and what was read in
    // its title (walk-2 W2-12).
    const words = within(unchanged).getByText("Didn't update: same version");
    expect(why(words)).toBe("The command said it finished, but the version read before and after the update is the same.");
    expect(within(unchanged).queryByText("Unexpected result")).toBeNull();
    expect(within(unchanged).queryByText(/reported success/)).toBeNull();
    expect(within(words).getByRole("img", { name: "Needs attention" }).querySelector("svg")).toHaveClass(
      "text-warning",
    );
    expect(within(unconfirmed).getByText("Result unconfirmed")).toBeInTheDocument();
  });

  it("says them in Chinese: 未能更新：需要输入密码, 结果未确认, 没有更新成功, 已更新, 再显示N条", async () => {
    await i18n.changeLanguage("zh-CN");
    try {
      const lines: JustUpdatedEntry[] = [
        { ...entry, id: "a", verified: true },
        { ...entry, id: "b", version: null, ending: { kind: "failed", cause: "needsPassword" } },
        { ...entry, id: "c", version: null, ending: { kind: "failed", cause: null } },
        { ...entry, id: "d", version: null, ending: { kind: "attention", outcome: "Unconfirmed" } },
        {
          ...entry,
          id: "e",
          version: null,
          ending: { kind: "attention", outcome: { NeedsAttention: "UnchangedAfterUpgrade" } },
        },
        ...Array.from({ length: JUST_UPDATED_SHOWN - 3 }, (_, i) => ({ ...entry, id: `more:${i}` })),
      ];
      renderWithProviders(<JustUpdated entries={lines} onClear={() => {}} />);

      const [verified, password, plain, unconfirmed, unchanged] = screen.getAllByRole("listitem");
      expect(why(within(verified).getByText("已更新"))).toBe("更新前后各读了一次已安装的版本，版本已经变了。");
      expect(why(within(unchanged).getByText("没有更新成功：版本没有变"))).toBe("命令显示已完成，但更新前后读到的版本相同。");
      expect(screen.getByText("清除记录")).toBeInTheDocument();
      expect(screen.getByRole("button", { name: "再显示2条" })).toBeInTheDocument();
      expect(why(within(password).getByText("未能更新：需要输入密码"))).toBe("需要输入Mac的登录密码，无法在这里输入。");
      expect(within(plain).getByText("未能更新")).toBeInTheDocument();
      expect(within(unconfirmed).getByText("结果未确认")).toBeInTheDocument();
      expect(within(unconfirmed).getByRole("img", { name: "需要查看" })).toBeInTheDocument();
    } finally {
      await i18n.changeLanguage("en");
    }
  });

  it("says in Chinese: 已由前面的更新一并完成, 未能更新：App已不在原来的位置, 原因：…", async () => {
    await i18n.changeLanguage("zh-CN");
    try {
      const lines: JustUpdatedEntry[] = [
        { ...entry, id: "a", ending: { kind: "succeeded", already: "ByEarlierUpdate" } },
        { ...entry, id: "b", version: null, ending: { kind: "failed", cause: "appMissing" } },
        { ...entry, id: "c", version: null, ending: { kind: "failed", cause: null, detail: "SHA256 mismatch" } },
      ];
      renderWithProviders(<JustUpdated entries={lines} onClear={() => {}} />);
      const [already, missing, other] = screen.getAllByRole("listitem");
      expect(why(within(already).getByText("已由前面的更新一并完成"))).toBe(
        "轮到这一项时，它已是新版本：前面的一项更新已把它一并更新了。",
      );
      expect(why(within(missing).getByText("未能更新：App已不在原来的位置"))).toBe(
        "App已不在原来的位置，可能已被移到废纸篓或删除；还要用，就把它放回“应用程序”后重试，不再需要可以卸载它。",
      );
      expect(why(within(other).getByText("未能更新"))).toBe("原因：SHA256 mismatch");
      cleanup();
      renderWithProviders(
        <JustUpdated
          entries={[{ ...entry, id: "d", version: null, ending: { kind: "failed", cause: "conflict", detail: "Could not symlink bin/npm" } }]}
          onClear={() => {}}
        />,
      );
      expect(why(screen.getByText("未能更新：与已有的文件冲突"))).toBe(
        "与已有的文件冲突，报错里写着是哪个文件，处理后重试。报错：Could not symlink bin/npm",
      );
    } finally {
      await i18n.changeLanguage("en");
    }
  });

  it(`has no fold line for ${JUST_UPDATED_SHOWN} or fewer`, () => {
    const few = Array.from({ length: JUST_UPDATED_SHOWN }, (_, i) => ({ ...entry, id: `op:${i}`, opId: i }));
    renderWithProviders(<JustUpdated entries={few} onClear={() => {}} />);
    expect(screen.getAllByRole("listitem")).toHaveLength(JUST_UPDATED_SHOWN);
    expect(screen.queryByRole("button", { name: /More|Show Fewer/ })).toBeNull();
  });
});
