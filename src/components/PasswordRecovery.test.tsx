import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import { PasswordRecovery } from "./PasswordRecovery";
import type { ArtifactKey, IssuedPlan } from "../lib/types";

const key: ArtifactKey = { instance_id: "brew:/opt/homebrew", kind: "Cask", name: "example" };
const fresh: IssuedPlan = {
  id: "0123456789abcdef0123456789abcdef", issued_at: 123,
  plan: {
    request: { kind: "Upgrade", instance_id: key.instance_id, artifact_kind: key.kind, name: key.name },
    action: { Command: { program: "/opt/homebrew/bin/brew", args: ["upgrade", "--cask", "example"], env: [["HOMEBREW_NO_INSTALL_CLEANUP", "1"], ["SUDO_ASKPASS", "/tmp/synthetic-helper"]] } },
    needs_password: true, locks: [key.instance_id], cancel_policy: "KillThenReconcile", warnings: [], affected: [], timeout_secs: 1800,
  },
};
const mockInvoke = vi.mocked(invoke);
beforeEach(() => { mockInvoke.mockReset(); });
/** `plan_operation` answered by `answer`; settings as a fresh install has them (details off). */
function answerPlans(answer: () => Promise<IssuedPlan>) {
  mockInvoke.mockImplementation((cmd: string) => (cmd === "plan_operation" ? answer() : Promise.resolve(undefined)));
}
const planCalls = () => mockInvoke.mock.calls.filter(([cmd]) => cmd === "plan_operation");
function open() {
  renderWithProviders(<PasswordRecovery artifactKey={key} name="Example" />);
  fireEvent.click(screen.getByRole("button", { name: "View steps: Example" }));
}

describe("fresh password recovery preview", () => {
  it("copies only the freshly planned primary command, keeping its environment except askpass", async () => {
    answerPlans(() => Promise.resolve(fresh));
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
    try {
      open();
      fireEvent.click(await screen.findByRole("button", { name: "Copy Command" }));
      await waitFor(() => expect(writeText).toHaveBeenCalledWith("HOMEBREW_NO_INSTALL_CLEANUP=1 /opt/homebrew/bin/brew upgrade --cask example"));
      // One preview, asked for once; nothing submitted, nothing else run.
      expect(planCalls()).toHaveLength(1);
      expect(mockInvoke.mock.calls.map(([cmd]) => cmd).filter((cmd) => cmd !== "get_settings")).toEqual(["plan_operation"]);
    } finally {
      Object.defineProperty(navigator, "clipboard", { value: undefined, configurable: true });
    }
  });

  it("shows a recovery step on rejected validation, in the window's words, and never hands over a stale command", async () => {
    // What `plan_operation` rejects with when a refresh has replaced the
    // row (`plan_operation_error` in src-tauri/src/ipc.rs): JSON, not prose.
    answerPlans(() => Promise.reject(JSON.stringify({ kind: "not_listed" })));
    open();
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("Couldn't prepare the command. Click Check Again, then click View Steps.");
    expect(screen.queryByRole("button", { name: "Copy Command" })).toBeNull();
    expect(screen.getByRole("dialog").querySelector("code")).toBeNull();
    fireEvent.click(within(alert).getByRole("button", { name: "Details: Example" }));
    expect(await screen.findByText("Couldn't start because it's no longer in the list. Click Check Again.")).toBeInTheDocument();
    expect(document.body.textContent).not.toContain("not_listed");
    expect(document.body.textContent).not.toContain("{");
  });

  it("offers no ⓘ for a refusal it has no words for", async () => {
    answerPlans(() => Promise.reject("Snapshot is stale"));
    open();
    const alert = await screen.findByRole("alert");
    expect(within(alert).queryByRole("button", { name: "Details: Example" })).toBeNull();
    expect(document.body.textContent).not.toContain("Snapshot is stale");
  });

  it("is titled and worded as the log of the same stop is, and says to check again once", async () => {
    answerPlans(() => Promise.resolve(fresh));
    open();
    const dialog = await screen.findByRole("dialog", { name: "Example" });
    expect(within(dialog).getByText("Update · Needs your password")).toBeInTheDocument();
    const step = within(dialog).getByText("This step needs your Mac login password, which can't be entered here.");
    expect(dialog.getAttribute("aria-describedby")?.split(" ")).toContain(step.id);
    await within(dialog).findByRole("button", { name: "Copy Command" });
    expect(dialog.textContent?.match(/check again/gi)).toHaveLength(1);
  });

  it.each([
    { ...fresh, plan: { ...fresh.plan, request: { ...fresh.plan.request, name: "another" } } },
    { ...fresh, plan: { ...fresh.plan, action: { TrashPaths: { paths: ["/tmp/synthetic-only"] } } } },
    { ...fresh, plan: { ...fresh.plan, action: { Command: { program: "/tmp/other", args: [], env: [] } } } },
  ])("refuses a preview for a different tool or action", async (answer) => {
    answerPlans(() => Promise.resolve(answer));
    open();
    await screen.findByRole("alert");
    expect(screen.queryByRole("button", { name: "Copy Command" })).toBeNull();
  });

  it("discards an old response when closed and reopened, and shows only the new preview", async () => {
    // Two previews in flight: the first, from before Done, answers last-but-one.
    const finish: Array<(plan: IssuedPlan) => void> = [];
    answerPlans(() => new Promise<IssuedPlan>((resolve) => { finish.push(resolve); }));
    open();
    fireEvent.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Done" }));
    fireEvent.click(screen.getByRole("button", { name: "View steps: Example" }));
    await waitFor(() => expect(finish).toHaveLength(2));
    const stale: IssuedPlan = {
      ...fresh,
      plan: { ...fresh.plan, action: { Command: { program: "/opt/homebrew/bin/brew", args: ["upgrade", "--cask", "example"], env: [["STALE", "1"]] } } },
    };
    finish[0](stale);
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(screen.getByRole("status")).toHaveTextContent("Preparing the command…");
    expect(screen.queryByRole("button", { name: "Copy Command" })).toBeNull();
    finish[1](fresh);
    await screen.findByRole("button", { name: "Copy Command" });
    const code = screen.getByRole("dialog").querySelector("code")?.textContent ?? "";
    expect(code).toContain("HOMEBREW_NO_INSTALL_CLEANUP=1");
    expect(code).not.toContain("STALE");
    expect(planCalls()).toHaveLength(2);
  });
});
