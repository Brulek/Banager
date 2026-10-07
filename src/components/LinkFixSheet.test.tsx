import { describe, expect, it, vi, beforeEach } from "vitest";
import { act, screen, waitFor, fireEvent, within } from "@testing-library/react";
import type { QueryClient } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import { command } from "../test/command";
import i18n from "../i18n";
import { LinkFixSheet, linkRequest } from "./LinkFixSheet";
import { SourceNotices } from "./SourceNotices";
import { artifactKeyId } from "../store/ui";
import { sourceNoticesFor } from "../lib/sources";
import { queryKeys } from "../lib/queries";
import type { IssuedPlan, LinkFix, ManagerInstance, OpRequest, Plan, Snapshot, Warning } from "../lib/types";

const NODE_22: LinkFix = {
  key: { instance_id: "brew:/opt/homebrew", kind: "Formula", name: "node@22" },
  version: "22.23.3_1",
};
const NODE_20: LinkFix = {
  key: { instance_id: "brew:/opt/homebrew", kind: "Formula", name: "node@20" },
  version: "20.19.5",
};

function instance(over: Partial<ManagerInstance>): ManagerInstance {
  return {
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
    ...over,
  };
}

/** The author's Mac on 2026-10-07: npm found no node, and Homebrew has node@22 unlinked. */
function npmWithout(fixes: LinkFix[]): ManagerInstance {
  return instance({
    id: "npm:/opt/homebrew",
    adapter_id: "npm",
    exe_path: "/opt/homebrew/bin/npm",
    version: null,
    status: {
      unavailable: "NotResponding",
      notes: [],
      no_answer: { kind: "CouldNotStart", missing_program: "node", link_fixes: fixes },
    },
  });
}

function snapshotWith(npm: ManagerInstance): Snapshot {
  return {
    generation: 1,
    round: 1,
    detect: "Found",
    instances: [instance({}), npm],
    artifacts: [],
    updates: [],
    refreshed_at: 1,
    stale: false,
    errors: [],
  };
}

function issued(request: OpRequest, warnings: Warning[] = []): IssuedPlan {
  const plan: Plan = {
    request,
    action: { Command: { program: "/opt/homebrew/bin/brew", args: ["link", "--formula", "--force", request.name], env: [] } },
    needs_password: false,
    locks: ["brew:/opt/homebrew"],
    cancel_policy: "KillThenReconcile",
    warnings,
    affected: [],
    timeout_secs: 300,
  };
  return { id: `plan-${request.name}`, plan, issued_at: 1791338400 };
}

/** Answers the sheet's calls: the snapshot, and each link's plan with `warnings`. */
function backend(npm: ManagerInstance, warnings: Warning[] = []) {
  vi.mocked(invoke).mockImplementation(async (cmd: string, args?: unknown) => {
    if (cmd === "get_snapshot") return snapshotWith(npm);
    if (cmd === "get_settings") return undefined;
    if (cmd === "plan_operation") return issued((args as { request: OpRequest }).request, warnings);
    if (cmd === "submit_operation") return 7;
    if (cmd === "refresh") return snapshotWith(npm);
    return undefined;
  });
}

function calls(command: string) {
  return vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === command);
}

beforeEach(() => {
  vi.mocked(invoke).mockReset();
  void i18n.changeLanguage("en");
});

describe("LinkFixSheet", () => {
  it("previews brew link --formula --force for the formula, and runs that plan only once Link is pressed", async () => {
    const npm = npmWithout([NODE_22]);
    backend(npm);
    const onClose = vi.fn();
    renderWithProviders(<LinkFixSheet instanceId={npm.id} onClose={onClose} />);

    const dialog = await screen.findByRole("alertdialog", { name: "Link “node@22”?" });
    expect(dialog).toHaveTextContent(
      "“node@22” has the node that npm needs, but it isn't linked where Terminal looks.",
    );
    // No promise that npm will then run: the check after the link says.
    expect(dialog).not.toHaveTextContent("can run");
    await waitFor(() => expect(calls("plan_operation")).toHaveLength(1));
    expect(calls("plan_operation")[0][1]).toEqual({ request: linkRequest(NODE_22) });
    expect(linkRequest(NODE_22)).toEqual({
      kind: "Link",
      instance_id: "brew:/opt/homebrew",
      artifact_kind: "Formula",
      name: "node@22",
    });
    // Nothing runs until the person says so.
    expect(calls("submit_operation")).toHaveLength(0);
    const link = await screen.findByRole("button", { name: "Link" });
    await waitFor(() => expect(link).toBeEnabled());
    fireEvent.click(screen.getByRole("button", { name: "Show Command" }));
    expect(await screen.findByText(command("/opt/homebrew/bin/brew link --formula --force node@22"))).toBeInTheDocument();
    fireEvent.click(link);
    await waitFor(() => expect(calls("submit_operation")).toHaveLength(1));
    expect(calls("submit_operation")[0][1]).toEqual({ planId: "plan-node@22" });
    await waitFor(() => expect(onClose).toHaveBeenCalled());
  });

  it("names the linked commands without promising which copy Terminal runs", async () => {
    const npm = npmWithout([NODE_22]);
    backend(npm, [{ LinkPutsCommands: { names: ["corepack", "node", "npm", "npx"] } }]);
    renderWithProviders(<LinkFixSheet instanceId={npm.id} onClose={() => {}} />);

    const dialog = await screen.findByRole("alertdialog", { name: "Link “node@22”?" });
    await waitFor(() =>
      expect(dialog).toHaveTextContent(
        "Linking adds links for its node, corepack, npm and npx under /opt/homebrew. Terminal uses these commands only if their folder is among those Terminal searches and no command with the same name comes before them.",
      ),
    );
    await waitFor(() => expect(screen.getByRole("button", { name: "Link" })).toBeEnabled());
  });

  it("lets the person choose among several, newest first, and plans the one chosen", async () => {
    const npm = npmWithout([NODE_22, NODE_20]);
    backend(npm);
    renderWithProviders(<LinkFixSheet instanceId={npm.id} onClose={() => {}} />);

    const popup = await screen.findByLabelText("Version to link:");
    const options = [...(popup as HTMLSelectElement).options].map((option) => option.textContent);
    expect(options).toEqual(["node@22 · 22.23.3_1", "node@20 · 20.19.5"]);
    expect((popup as HTMLSelectElement).value).toBe(artifactKeyId(NODE_22.key));
    fireEvent.change(popup, { target: { value: artifactKeyId(NODE_20.key) } });
    expect(await screen.findByRole("alertdialog", { name: "Link “node@20”?" })).toBeInTheDocument();
    await waitFor(() =>
      expect(calls("plan_operation").map(([, args]) => (args as { request: OpRequest }).request.name)).toEqual([
        "node@22",
        "node@20",
      ]),
    );
  });

  it("selects the same formula in a second Homebrew and submits only its newly issued plan", async () => {
    const intel = instance({ id: "brew:/usr/local", exe_path: "/usr/local/bin/brew", prefix: "/usr/local" });
    const intelNode = { ...NODE_22, key: { ...NODE_22.key, instance_id: intel.id } };
    const npm = npmWithout([NODE_22, intelNode]);
    const snapshot = snapshotWith(npm);
    snapshot.instances.push(intel);
    let resolveIntel!: (plan: IssuedPlan) => void;
    vi.mocked(invoke).mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === "get_snapshot") return snapshot;
      if (cmd === "plan_operation") {
        const request = (args as { request: OpRequest }).request;
        return request.instance_id === intel.id
          ? new Promise<IssuedPlan>((resolve) => { resolveIntel = resolve; })
          : issued(request);
      }
      if (cmd === "submit_operation") return 7;
      return undefined;
    });
    renderWithProviders(<LinkFixSheet instanceId={npm.id} onClose={() => {}} />);
    const popup = await screen.findByLabelText("Version to link:");
    const options = [...(popup as HTMLSelectElement).options];
    expect(options.map((option) => option.value)).toEqual([artifactKeyId(NODE_22.key), artifactKeyId(intelNode.key)]);
    expect(options.map((option) => option.textContent)).toEqual([
      "node@22 · 22.23.3_1 · Apple silicon",
      "node@22 · 22.23.3_1 · Intel",
    ]);
    // In full under the title, as the sidebar names it.
    expect(screen.getByRole("alertdialog")).toHaveTextContent("Homebrew (Apple silicon)");
    const link = screen.getByRole("button", { name: "Link" });
    await waitFor(() => expect(link).toBeEnabled());
    fireEvent.change(popup, { target: { value: artifactKeyId(intelNode.key) } });
    expect(link).toBeDisabled();
    fireEvent.click(link);
    expect(calls("submit_operation")).toHaveLength(0);
    await waitFor(() => expect(calls("plan_operation").map(([, args]) => args)).toEqual([
      { request: linkRequest(NODE_22) },
      { request: linkRequest(intelNode) },
    ]));
    const intelPlan = issued(linkRequest(intelNode));
    intelPlan.id = "plan-intel-node@22";
    intelPlan.plan.action = { Command: {
      program: intel.exe_path, args: ["link", "--formula", "--force", "node@22"], env: [],
    } };
    intelPlan.plan.locks = [intel.id];
    resolveIntel(intelPlan);
    await waitFor(() => expect(link).toBeEnabled());
    expect(screen.getByRole("alertdialog")).toHaveTextContent("Linking adds links for its commands under /usr/local.");
    fireEvent.click(screen.getByRole("button", { name: "Show Command" }));
    expect(await screen.findByText(command("/usr/local/bin/brew link --formula --force node@22"))).toBeInTheDocument();
    fireEvent.click(link);
    await waitFor(() => expect(calls("submit_operation")).toEqual([
      ["submit_operation", { planId: intelPlan.id }],
    ]));
  });

  it("names every choice's Homebrew once the formulae come from more than one, not only those that share a name", async () => {
    // node@22 in Apple silicon Homebrew and node@20 only in Intel's: the
    // choice is also which Homebrew, and so which folder gets the links.
    // Where it is, as the sidebar says it under "Homebrew": the whole
    // 「Homebrew (Apple silicon)」 would not fit beside the label.
    const intel = instance({ id: "brew:/usr/local", exe_path: "/usr/local/bin/brew", prefix: "/usr/local" });
    const intelNode20 = { ...NODE_20, key: { ...NODE_20.key, instance_id: intel.id } };
    const npm = npmWithout([NODE_22, intelNode20]);
    const snapshot = snapshotWith(npm);
    snapshot.instances.push(intel);
    vi.mocked(invoke).mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === "get_snapshot") return snapshot;
      if (cmd === "plan_operation") return issued((args as { request: OpRequest }).request);
      return undefined;
    });
    renderWithProviders(<LinkFixSheet instanceId={npm.id} onClose={() => {}} />);
    const popup = await screen.findByLabelText("Version to link:");
    expect([...(popup as HTMLSelectElement).options].map((option) => option.textContent)).toEqual([
      "node@22 · 22.23.3_1 · Apple silicon",
      "node@20 · 20.19.5 · Intel",
    ]);
    fireEvent.change(popup, { target: { value: artifactKeyId(intelNode20.key) } });
    expect(await screen.findByRole("alertdialog", { name: "Link “node@20”?" })).toHaveTextContent(
      "Homebrew (Intel) · 20.19.5",
    );
  });

  it("names no Homebrew in the choices while there is one", async () => {
    const npm = npmWithout([NODE_22, NODE_20]);
    const snapshot = snapshotWith(npm);
    // A second Homebrew that offers nothing: the choices are all the first's.
    snapshot.instances.push(instance({ id: "brew:/usr/local", exe_path: "/usr/local/bin/brew", prefix: "/usr/local" }));
    vi.mocked(invoke).mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === "get_snapshot") return snapshot;
      if (cmd === "plan_operation") return issued((args as { request: OpRequest }).request);
      return undefined;
    });
    renderWithProviders(<LinkFixSheet instanceId={npm.id} onClose={() => {}} />);
    const popup = await screen.findByLabelText("Version to link:");
    expect([...(popup as HTMLSelectElement).options].map((option) => option.textContent)).toEqual([
      "node@22 · 22.23.3_1",
      "node@20 · 20.19.5",
    ]);
    // Which one it is stays under the title.
    expect(screen.getByRole("alertdialog")).toHaveTextContent("Homebrew (Apple silicon)");
  });

  it("where Homebrew would link nothing, says what is in the way and the command that would, then Check Again", async () => {
    // npm updated through itself keeps its own `npm` in `bin`: what the
    // author's Mac had on 2026-10-07. No 「要链接…吗？」 whose only button
    // is Cancel: what is in the way, the Terminal command that replaces
    // it, Copy Command, and Check Again for after.
    const npm = npmWithout([NODE_22]);
    backend(npm, [
      { LinkPutsCommands: { names: ["corepack", "node", "npm", "npx"] } },
      { LinkConflicts: { paths: ["/opt/homebrew/bin/npm", "/opt/homebrew/bin/npx"] } },
    ]);
    const onClose = vi.fn();
    renderWithProviders(<LinkFixSheet instanceId={npm.id} onClose={onClose} />);

    const dialog = await screen.findByRole("alertdialog", { name: "Can't link “node@22”" });
    expect(
      screen.getByText(
        "/opt/homebrew/bin/npm, /opt/homebrew/bin/npx are already there, and Homebrew won't replace them on its own, so it can't be linked.",
      ),
    ).toBeInTheDocument();
    expect(dialog).toHaveTextContent(
      "To link it, you can run this command in Terminal. It deletes the files in the way, the ones above among them, and links the ones in “node@22” in their place.",
    );
    expect(screen.getByRole("group", { name: "Command to run in Terminal" })).toHaveTextContent(
      "/opt/homebrew/bin/brew link --formula --force --overwrite node@22",
    );
    // A line breaks between its tokens, never inside one: each a box of
    // its own that goes to the next line whole (`unbrokenTokens`), and
    // only the spaces between them loose -- 「--」 / 「overwrite」 reads as
    // something else.
    const code = screen.getByRole("group", { name: "Command to run in Terminal" }).querySelector("code") as HTMLElement;
    const tokens = [...code.querySelectorAll<HTMLElement>("[data-command-token]")];
    expect(tokens.map((token) => token.textContent)).toEqual([
      "/opt/homebrew/bin/brew",
      "link",
      "--formula",
      "--force",
      "--overwrite",
      "node@22",
    ]);
    for (const token of tokens) expect(token.className.split(" ")).toEqual(["inline-block", "max-w-full", "break-words"]);
    const loose = [...code.childNodes].filter((node) => node.nodeType === Node.TEXT_NODE);
    expect(loose.map((node) => node.textContent)).toEqual(Array(tokens.length - 1).fill(" "));
    expect(screen.getByRole("button", { name: "Copy Command" })).toBeInTheDocument();
    expect(dialog).toHaveTextContent("When it's done, click Check Again.");
    expect(screen.queryByRole("button", { name: "Link" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Cancel" })).toBeNull();
    // Banager itself runs nothing here: Check Again checks, and closes.
    fireEvent.click(screen.getByRole("button", { name: "Check Again" }));
    await waitFor(() => expect(calls("refresh")).toHaveLength(1));
    expect(onClose).toHaveBeenCalled();
    expect(calls("submit_operation")).toHaveLength(0);
  });

  it("is opened by the notice's Fix…", async () => {
    const npm = npmWithout([NODE_22]);
    backend(npm);
    renderWithProviders(<SourceNotices notices={sourceNoticesFor(npm, "npm")} />);

    expect(screen.getByText("npm can't run")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Fix…" }));
    expect(await screen.findByRole("alertdialog", { name: "Link “node@22”?" })).toBeInTheDocument();
  });
});

describe("link effects depend on PATH", () => {
  it.each([
    ["en", true], ["en", false],
    ["zh-CN", true], ["zh-CN", false],
    ["zh-Hant", true], ["zh-Hant", false],
  ] as const)("qualifies both the folder and command priority in %s, listed=%s", async (language, listed) => {
    await i18n.changeLanguage(language);
    const npm = npmWithout([NODE_22]);
    backend(npm, listed ? [{ LinkPutsCommands: { names: ["node"] } }] : []);
    renderWithProviders(<LinkFixSheet instanceId={npm.id} onClose={() => {}} />);
    const dialog = await screen.findByRole("alertdialog");
    const effect = {
      en: listed
        ? "Linking adds links for its node under /opt/homebrew."
        : "Linking adds links for its commands under /opt/homebrew.",
      "zh-CN": listed
        ? "链接会在/opt/homebrew下创建它的node的链接。"
        : "链接会在/opt/homebrew下创建它的命令的链接。",
      "zh-Hant": listed
        ? "連結會在/opt/homebrew底下建立它的node的連結。"
        : "連結會在/opt/homebrew底下建立它的指令的連結。",
    }[language];
    const condition = {
      en: "Terminal uses these commands only if their folder is among those Terminal searches and no command with the same name comes before them.",
      "zh-CN": "只有命令所在的文件夹在终端的查找范围内，且没有其他同名命令排在前面，终端才会使用这些命令。",
      "zh-Hant": "只有指令所在的檔案夾在終端機的搜尋範圍內，且沒有其他同名指令排在前面，終端機才會使用這些指令。",
    }[language];
    await waitFor(() => expect(dialog).toHaveTextContent(effect));
    expect(dialog).toHaveTextContent(condition);
    expect(dialog).not.toHaveTextContent(/so typing|运行的是|執行的是|執行的都是/);
  });
});

describe("LinkFixSheet in Chinese", () => {
  it("asks in the window's language", async () => {
    await i18n.changeLanguage("zh-CN");
    const npm = npmWithout([NODE_22]);
    backend(npm, [{ LinkConflicts: { paths: ["/opt/homebrew/bin/npm"] } }]);
    renderWithProviders(<LinkFixSheet instanceId={npm.id} onClose={() => {}} />);
    const dialog = await screen.findByRole("alertdialog", { name: "无法链接“node@22”" });
    expect(dialog).toHaveTextContent("“node@22”中有npm需要的node，但没有链接到终端能找到的地方。");
    expect(dialog).not.toHaveTextContent("就能运行");
    expect(await screen.findByText("/opt/homebrew/bin/npm已存在，Homebrew不会自行替换，因此无法链接。")).toBeInTheDocument();
    expect(dialog).toHaveTextContent(
      "要链接它，可以在终端里运行下面的命令。它会删除挡住链接的文件，包括上面这些，换成“node@22”中的。",
    );
    expect(screen.getByRole("button", { name: "重新检查" })).toBeInTheDocument();
    await i18n.changeLanguage("en");
  });

  it("says what linking changes, in Taiwan's usage", async () => {
    await i18n.changeLanguage("zh-Hant");
    const npm = npmWithout([NODE_22]);
    backend(npm, [{ LinkPutsCommands: { names: ["corepack", "node", "npm", "npx"] } }]);
    renderWithProviders(<LinkFixSheet instanceId={npm.id} onClose={() => {}} />);
    const dialog = await screen.findByRole("alertdialog", { name: "要連結「node@22」嗎？" });
    await waitFor(() =>
      expect(dialog).toHaveTextContent(
        "連結會在/opt/homebrew底下建立它的node、corepack、npm和npx的連結。只有指令所在的檔案夾在終端機的搜尋範圍內，且沒有其他同名指令排在前面，終端機才會使用這些指令。",
      ),
    );
    await i18n.changeLanguage("en");
  });
});

describe("LinkFixSheet where Homebrew's own links are already there, its link not recorded (r11 F2)", () => {
  const risk: Warning = { LinkRollbackRisk: { paths: ["/opt/homebrew/bin/npm", "/opt/homebrew/bin/npx"] } };
  it.each([
    [
      "en",
      "/opt/homebrew/bin/npm, /opt/homebrew/bin/npx are already linked to it. If linking stops partway, Homebrew removes those links too, so it can't be linked here.",
    ],
    ["zh-CN", "/opt/homebrew/bin/npm、/opt/homebrew/bin/npx已链接到它。如果链接中途停止，Homebrew也会删除已有的链接，因此无法在这里链接。"],
    ["zh-Hant", "/opt/homebrew/bin/npm、/opt/homebrew/bin/npx已連結到它。若連結中途停止，Homebrew也會刪除已有的連結，因此無法在這裡連結。"],
  ].flatMap(([language, line]) => [false, true].map((conflict) => ({ language, line, conflict }))))(
    "offers no Link and names them, in $language (files also in the way: $conflict)",
    async ({ language, line, conflict }) => {
      await i18n.changeLanguage(language);
      const npm = npmWithout([NODE_22]);
      backend(npm, conflict ? [risk, { LinkConflicts: { paths: ["/opt/homebrew/bin/corepack"] } }] : [risk]);
      renderWithProviders(<LinkFixSheet instanceId={npm.id} onClose={() => {}} />);
      await screen.findByText(line);
      expect(screen.queryByRole("button", { name: i18n.t("noAnswer.sheet.confirm") })).toBeNull();
      expect(screen.getByRole("button", { name: i18n.t("common.copyCommand") })).toBeInTheDocument();
      // `--overwrite` only where files are in the way: it would not keep
      // the links already there.
      expect(screen.getByRole("group", { name: i18n.t("noAnswer.sheet.commandLabel") })).toHaveTextContent(
        `/opt/homebrew/bin/brew link --formula --force ${conflict ? "--overwrite " : ""}node@22`,
      );
      expect(screen.getByRole("alertdialog")).toHaveTextContent(
        i18n.t(conflict ? "linkRollback.handoffConflicts" : "linkRollback.handoff", { formula: "node@22", count: 2 }),
      );
      expect(calls("submit_operation")).toHaveLength(0);
      await i18n.changeLanguage("en");
    },
  );

  it("says one link already there in the singular", async () => {
    const npm = npmWithout([NODE_22]);
    backend(npm, [{ LinkRollbackRisk: { paths: ["/opt/homebrew/bin/npm"] } }]);
    renderWithProviders(<LinkFixSheet instanceId={npm.id} onClose={() => {}} />);
    await screen.findByText(
      "/opt/homebrew/bin/npm is already linked to it. If linking stops partway, Homebrew removes that link too, so it can't be linked here.",
    );
    // And so does the sentence under it, not "the links above" (r21 C10).
    expect(
      screen.getByText("To link it, you can run this command in Terminal. If it stops partway, Homebrew removes the link above too."),
    ).toBeInTheDocument();
  });

  it.each([
    ["zh-CN", "要链接它，可以在终端里运行下面的命令。如果命令中途停止，Homebrew也会删除上面的链接。"],
    ["zh-Hant", "要連結它，可以在終端機執行下面的指令。若指令中途停止，Homebrew也會刪除上面的連結。"],
  ])("says the links above without a number in %s, one or more (r21 C10)", async (language, handoff) => {
    await i18n.changeLanguage(language);
    try {
      const npm = npmWithout([NODE_22]);
      backend(npm, [{ LinkRollbackRisk: { paths: ["/opt/homebrew/bin/npm"] } }]);
      renderWithProviders(<LinkFixSheet instanceId={npm.id} onClose={() => {}} />);
      expect(await screen.findByText(handoff)).toBeInTheDocument();
    } finally {
      await i18n.changeLanguage("en");
    }
  });

  it("says the one link already there in the singular when files are in the way too", async () => {
    const npm = npmWithout([NODE_22]);
    backend(npm, [
      { LinkRollbackRisk: { paths: ["/opt/homebrew/bin/npm"] } },
      { LinkConflicts: { paths: ["/opt/homebrew/bin/corepack"] } },
    ]);
    renderWithProviders(<LinkFixSheet instanceId={npm.id} onClose={() => {}} />);
    expect(
      await screen.findByText(
        "To link it, you can run this command in Terminal. It deletes the files in the way and links the ones in “node@22” in their place. If it still stops partway, Homebrew also removes the link that was already there.",
      ),
    ).toBeInTheDocument();
  });
});

describe("LinkFixSheet to a screen reader (r27 A3)", () => {
  const inTheWay: Warning[] = [
    { LinkPutsCommands: { names: ["corepack", "node", "npm", "npx"] } },
    { LinkConflicts: { paths: ["/opt/homebrew/bin/npm", "/opt/homebrew/bin/npx"] } },
  ];
  const linkable: Warning[] = [{ LinkPutsCommands: { names: ["corepack", "node", "npm", "npx"] } }];

  /**
   * Answers each formula's preview with its own warnings, as the mock's
   * `?state=nonode` does (node@22 in the way, node@20 not); `held`'s
   * preview waits until it is let go.
   */
  function backendBy(npm: ManagerInstance, warningsOf: (name: string) => Warning[], held?: string) {
    let release: () => void = () => {};
    vi.mocked(invoke).mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === "get_snapshot") return snapshotWith(npm);
      if (cmd === "plan_operation") {
        const request = (args as { request: OpRequest }).request;
        const plan = issued(request, warningsOf(request.name));
        if (request.name !== held) return plan;
        return new Promise<IssuedPlan>((resolve) => {
          release = () => resolve(plan);
        });
      }
      if (cmd === "refresh") return snapshotWith(npm);
      return undefined;
    });
    return { release: () => release() };
  }

  /** The sheet's own status: the one that says what it turned into (not the one that says it is checking). */
  function saidStatus(dialog: HTMLElement): HTMLElement {
    const statuses = within(dialog).getAllByRole("status");
    const said = statuses.find((status) => status.className.split(" ").includes("sr-only"));
    expect(said).toBeDefined();
    return said as HTMLElement;
  }

  it.each([
    [
      "en",
      "Link “node@22”?",
      "Can't link “node@22”: /opt/homebrew/bin/npm, /opt/homebrew/bin/npx are already there, and Homebrew won't replace them on its own, so it can't be linked.",
      "/opt/homebrew/bin/npm, /opt/homebrew/bin/npx are already there, and Homebrew won't replace them on its own, so it can't be linked.",
    ],
    [
      "zh-CN",
      "要链接“node@22”吗？",
      "无法链接“node@22”：/opt/homebrew/bin/npm、/opt/homebrew/bin/npx已存在，Homebrew不会自行替换，因此无法链接。",
      "/opt/homebrew/bin/npm、/opt/homebrew/bin/npx已存在，Homebrew不会自行替换，因此无法链接。",
    ],
  ])("says the refusal in %s, from a status there since the sheet opened, as the focused Cancel turns into Close", async (language, question, said, line) => {
    await i18n.changeLanguage(language);
    try {
      const npm = npmWithout([NODE_22]);
      const { release } = backendBy(npm, () => inTheWay, "node@22");
      renderWithProviders(<LinkFixSheet instanceId={npm.id} onClose={() => {}} />);
      const dialog = await screen.findByRole("alertdialog", { name: question });
      await waitFor(() => expect(calls("plan_operation")).toHaveLength(1));
      // Open, on the question, checking: the status is there and says nothing.
      const status = saidStatus(dialog);
      expect(status).toHaveTextContent(/^$/);
      // What says it is checking is `SheetPending`'s own.
      expect(within(dialog).getByText(i18n.t("noAnswer.sheet.checking"))).toHaveAttribute("role", "status");
      const cancel = within(dialog).getByRole("button", { name: i18n.t("common.cancel") });
      await waitFor(() => expect(cancel).toHaveFocus());

      release();
      await screen.findByRole("alertdialog", { name: i18n.t("noAnswer.sheet.blockedTitle", { formula: "node@22" }) });
      // The same status, now with the refusal: its title and what is in the way.
      await waitFor(() => expect(status).toHaveTextContent(said));
      expect(status.isConnected).toBe(true);
      expect(saidStatus(dialog)).toBe(status);
      // The focus stayed on the button it was on, renamed where it stood;
      // the dialog is now described by what is in the way too.
      expect(cancel).toHaveFocus();
      expect(cancel).toHaveAccessibleName(i18n.t("common.close"));
      expect(dialog).toHaveAccessibleDescription(expect.stringContaining(line));
    } finally {
      await i18n.changeLanguage("en");
    }
  });

  it("says nothing more as it opens on a question the preview finds nothing in the way of", async () => {
    const npm = npmWithout([NODE_22]);
    backendBy(npm, () => linkable);
    renderWithProviders(<LinkFixSheet instanceId={npm.id} onClose={() => {}} />);
    const dialog = await screen.findByRole("alertdialog", { name: "Link “node@22”?" });
    await waitFor(() => expect(within(dialog).getByRole("button", { name: "Link" })).toBeEnabled());
    expect(saidStatus(dialog)).toHaveTextContent(/^$/);
    // Described by the reason alone: there is no line of what is in the way.
    expect(dialog).toHaveAccessibleDescription(expect.not.stringContaining("already there"));
  });

  it.each(["en", "zh-CN"])(
    "says what a choice in the version popup turned the sheet into, either way, in %s",
    async (language) => {
      await i18n.changeLanguage(language);
      try {
        // The mock's `?state=nonode`: node@22 has npm's own files in its
        // way, node@20 none.
        const npm = npmWithout([NODE_22, NODE_20]);
        backendBy(npm, (name) => (name === "node@22" ? inTheWay : linkable));
        renderWithProviders(<LinkFixSheet instanceId={npm.id} onClose={() => {}} />);
        const blockedTitle = i18n.t("noAnswer.sheet.blockedTitle", { formula: "node@22" });
        const dialog = await screen.findByRole("alertdialog", { name: blockedTitle });
        const status = saidStatus(dialog);
        await waitFor(() => expect(status).toHaveTextContent(new RegExp(`^${blockedTitle}`)));

        // node@20: back to the question, Link the default button again.
        const popup = within(dialog).getByLabelText(i18n.t("noAnswer.sheet.version"));
        popup.focus();
        fireEvent.change(popup, { target: { value: artifactKeyId(NODE_20.key) } });
        const question = i18n.t("noAnswer.sheet.title", { formula: "node@20" });
        await screen.findByRole("alertdialog", { name: question });
        await waitFor(() => expect(status).toHaveTextContent(question));
        expect(saidStatus(dialog)).toBe(status);
        expect(within(dialog).getByRole("button", { name: i18n.t("noAnswer.sheet.confirm") })).toBeEnabled();
        expect(dialog).toHaveAccessibleDescription(expect.not.stringContaining("/opt/homebrew/bin/npx"));
        expect(popup).toHaveFocus();

        // And node@22 again: the refusal, said once more.
        fireEvent.change(popup, { target: { value: artifactKeyId(NODE_22.key) } });
        await screen.findByRole("alertdialog", { name: blockedTitle });
        await waitFor(() => expect(status).toHaveTextContent(new RegExp(`^${blockedTitle}`)));
        expect(status).toHaveTextContent("/opt/homebrew/bin/npx");
        expect(within(dialog).getByRole("button", { name: i18n.t("header.checkAgain") })).toBeInTheDocument();
      } finally {
        await i18n.changeLanguage("en");
      }
    },
  );

  it("says the refusal where the version chosen in the popup turns a question into one", async () => {
    const npm = npmWithout([NODE_22, NODE_20]);
    backendBy(npm, (name) =>
      name === "node@20" ? [...linkable, { LinkRollbackRisk: { paths: ["/opt/homebrew/bin/npm"] } }] : linkable,
    );
    renderWithProviders(<LinkFixSheet instanceId={npm.id} onClose={() => {}} />);
    const dialog = await screen.findByRole("alertdialog", { name: "Link “node@22”?" });
    await waitFor(() => expect(within(dialog).getByRole("button", { name: "Link" })).toBeEnabled());
    const status = saidStatus(dialog);
    expect(status).toHaveTextContent(/^$/);
    fireEvent.change(within(dialog).getByLabelText("Version to link:"), { target: { value: artifactKeyId(NODE_20.key) } });
    await screen.findByRole("alertdialog", { name: "Can't link “node@20”" });
    await waitFor(() =>
      expect(status).toHaveTextContent(
        "Can't link “node@20”: /opt/homebrew/bin/npm is already linked to it. If linking stops partway, Homebrew removes that link too, so it can't be linked here.",
      ),
    );
    expect(dialog).toHaveAccessibleDescription(expect.stringContaining("/opt/homebrew/bin/npm is already linked to it."));
  });

  /** A fresh snapshot while the sheet is open, as an operation that finishes behind it brings. */
  function refreshWith(queryClient: QueryClient, npm: ManagerInstance) {
    act(() => {
      queryClient.setQueryData(queryKeys.snapshot, { ...snapshotWith(npm), generation: 2 });
    });
  }

  it.each(["en", "zh-CN"])(
    "says the question a fresh snapshot turns the refusal back into, with no choice in the popup, in %s",
    async (language) => {
      await i18n.changeLanguage(language);
      try {
        const warningsOf = (name: string) => (name === "node@22" ? inTheWay : linkable);
        const npm = npmWithout([NODE_22]);
        backendBy(npm, warningsOf);
        const { queryClient } = renderWithProviders(<LinkFixSheet instanceId={npm.id} onClose={() => {}} />);
        const blockedTitle = i18n.t("noAnswer.sheet.blockedTitle", { formula: "node@22" });
        const dialog = await screen.findByRole("alertdialog", { name: blockedTitle });
        const status = saidStatus(dialog);
        await waitFor(() => expect(status).toHaveTextContent(new RegExp(`^${blockedTitle}`)));
        const close = within(dialog).getByRole("button", { name: i18n.t("common.close") });
        expect(close).toHaveFocus();

        // node@20 is put first: the sheet is its question, the focused
        // Close renamed Cancel where it stands, Link the default again.
        const later = npmWithout([NODE_20, NODE_22]);
        backendBy(later, warningsOf);
        refreshWith(queryClient, later);
        const question = i18n.t("noAnswer.sheet.title", { formula: "node@20" });
        await screen.findByRole("alertdialog", { name: question });
        await waitFor(() => expect(status.textContent).toBe(question));
        expect(saidStatus(dialog)).toBe(status);
        expect(close).toHaveFocus();
        expect(close).toHaveAccessibleName(i18n.t("common.cancel"));
        expect(within(dialog).getByRole("button", { name: i18n.t("noAnswer.sheet.confirm") })).toBeEnabled();
      } finally {
        await i18n.changeLanguage("en");
      }
    },
  );

  it.each(["en", "zh-CN"])(
    "says the other question a fresh snapshot puts first where the sheet opened on one, in %s",
    async (language) => {
      await i18n.changeLanguage(language);
      try {
        const npm = npmWithout([NODE_20]);
        backendBy(npm, () => linkable);
        const { queryClient } = renderWithProviders(<LinkFixSheet instanceId={npm.id} onClose={() => {}} />);
        const dialog = await screen.findByRole("alertdialog", {
          name: i18n.t("noAnswer.sheet.title", { formula: "node@20" }),
        });
        const link = within(dialog).getByRole("button", { name: i18n.t("noAnswer.sheet.confirm") });
        await waitFor(() => expect(link).toBeEnabled());
        const status = saidStatus(dialog);
        expect(status).toHaveTextContent(/^$/);

        const later = npmWithout([NODE_22, NODE_20]);
        backendBy(later, () => linkable);
        refreshWith(queryClient, later);
        const question = i18n.t("noAnswer.sheet.title", { formula: "node@22" });
        await screen.findByRole("alertdialog", { name: question });
        await waitFor(() => expect(status.textContent).toBe(question));
        expect(saidStatus(dialog)).toBe(status);
      } finally {
        await i18n.changeLanguage("en");
      }
    },
  );
});
