import { describe, expect, it, vi, beforeEach } from "vitest";
import { screen, waitFor, fireEvent } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import i18n from "../i18n";
import { LinkFixSheet, linkRequest } from "./LinkFixSheet";
import { SourceNotices } from "./SourceNotices";
import { sourceNoticesFor } from "../lib/sources";
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
    expect(await screen.findByText("/opt/homebrew/bin/brew link --formula --force node@22")).toBeInTheDocument();
    fireEvent.click(link);
    await waitFor(() => expect(calls("submit_operation")).toHaveLength(1));
    expect(calls("submit_operation")[0][1]).toEqual({ planId: "plan-node@22" });
    await waitFor(() => expect(onClose).toHaveBeenCalled());
  });

  it("says what linking changes in Terminal: the formula's commands, and which node runs", async () => {
    // Linking node@20 -- or a node@N only ever a dependency -- changes
    // which node, npm, npx and corepack Terminal runs for everything.
    const npm = npmWithout([NODE_22]);
    backend(npm, [{ LinkPutsCommands: { names: ["corepack", "node", "npm", "npx"] } }]);
    renderWithProviders(<LinkFixSheet instanceId={npm.id} onClose={() => {}} />);

    const dialog = await screen.findByRole("alertdialog", { name: "Link “node@22”?" });
    await waitFor(() =>
      expect(dialog).toHaveTextContent(
        "Linking puts its node, corepack, npm and npx there, so typing any of them in Terminal runs the one in “node@22”, version 22.23.3_1.",
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
    expect((popup as HTMLSelectElement).value).toBe("node@22");
    fireEvent.change(popup, { target: { value: "node@20" } });
    expect(await screen.findByRole("alertdialog", { name: "Link “node@20”?" })).toBeInTheDocument();
    await waitFor(() =>
      expect(calls("plan_operation").map(([, args]) => (args as { request: OpRequest }).request.name)).toEqual([
        "node@22",
        "node@20",
      ]),
    );
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
        "連結會把它的node、corepack、npm和npx放到那裡，之後在終端機輸入這些指令，執行的都是「node@22」中的這一份，版本是22.23.3_1。",
      ),
    );
    await i18n.changeLanguage("en");
  });
});
