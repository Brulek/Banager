import { describe, expect, it, vi, beforeEach } from "vitest";
import { screen, waitFor, fireEvent } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import { UninstallDialog } from "./UninstallDialog";
import type { IssuedPlan, OpRequest, Plan } from "../lib/types";

const request: OpRequest = {
  kind: "Uninstall",
  instance_id: "brew:/opt/homebrew",
  artifact_kind: "Formula",
  name: "jq",
};

function issuedPlanFor(overrides: Partial<Plan> = {}): IssuedPlan {
  return {
    id: 1,
    plan: {
      request,
      program: "/opt/homebrew/bin/brew",
      args: ["uninstall", "--formula", "jq"],
      env: [],
      needs_password: false,
      locks: ["brew:/opt/homebrew"],
      cancel_policy: "KillThenReconcile",
      warnings: [],
      affected: [],
      timeout_secs: 1800,
      ...overrides,
    },
    issued_at: 1758000000,
  };
}

beforeEach(() => {
  vi.mocked(invoke).mockReset();
});

function submitCalls() {
  return vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === "submit_operation");
}

describe("UninstallDialog", () => {
  it("shows a checking message and a disabled confirm button while the plan is loading", async () => {
    vi.mocked(invoke).mockImplementation(() => new Promise(() => {}));

    renderWithProviders(
      <UninstallDialog open onOpenChange={() => {}} request={request} displayName="jq" />,
    );

    // `planMutation.mutate()` runs in an effect and TanStack Query v5 pushes
    // the `isPending` transition to React through a setTimeout(0) scheduler,
    // so right after render the component is still idle: wait for it.
    expect(await screen.findByText("Checking what this would affect…")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Uninstall" })).toBeDisabled();
  });

  it("disables confirm and explains what would break when something depends on it", async () => {
    vi.mocked(invoke).mockResolvedValue(issuedPlanFor({ affected: ["jq-cli-wrapper"] }));

    renderWithProviders(
      <UninstallDialog open onOpenChange={() => {}} request={request} displayName="jq" />,
    );

    await waitFor(() => expect(screen.getByText("jq-cli-wrapper")).toBeInTheDocument());
    expect(screen.getByRole("button", { name: "Uninstall" })).toBeDisabled();
    expect(
      screen.getByText("/opt/homebrew/bin/brew uninstall --formula jq"),
    ).toBeInTheDocument();
  });

  it("submits the plan id and reports the new op id when nothing would break", async () => {
    const issued = issuedPlanFor();
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "plan_operation") return issued;
      if (cmd === "submit_operation") return 7;
      throw new Error(`unexpected command ${cmd}`);
    });
    const onSubmitted = vi.fn();
    const onOpenChange = vi.fn();

    renderWithProviders(
      <UninstallDialog
        open
        onOpenChange={onOpenChange}
        request={request}
        displayName="jq"
        onSubmitted={onSubmitted}
      />,
    );

    const confirmButton = await screen.findByRole("button", { name: "Uninstall" });
    await waitFor(() => expect(confirmButton).not.toBeDisabled());
    // The exact command is on screen, and nothing has been submitted, before
    // the user is allowed to confirm.
    expect(screen.getByText("/opt/homebrew/bin/brew uninstall --formula jq")).toBeInTheDocument();
    expect(submitCalls()).toHaveLength(0);
    fireEvent.click(confirmButton);

    await waitFor(() => expect(onSubmitted).toHaveBeenCalledWith(7));
    expect(vi.mocked(invoke)).toHaveBeenCalledWith("submit_operation", { planId: 1 });
    expect(onOpenChange).toHaveBeenCalledWith(false);
  });

  it("submits nothing when the dialog is cancelled", async () => {
    vi.mocked(invoke).mockResolvedValue(issuedPlanFor());
    const onOpenChange = vi.fn();

    renderWithProviders(
      <UninstallDialog open onOpenChange={onOpenChange} request={request} displayName="jq" />,
    );

    await screen.findByText("/opt/homebrew/bin/brew uninstall --formula jq");
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));

    expect(onOpenChange).toHaveBeenCalledWith(false);
    expect(submitCalls()).toHaveLength(0);
  });

  it("shows the backend's error verbatim, re-plans, and submits the fresh id only when confirmed again", async () => {
    // The dialog sat open past the PlanId's 10-minute lifetime (or the id was
    // already consumed): the backend rejects with a bare string, and the
    // stale preview must not be resubmittable with the same id.
    let planCalls = 0;
    let submitAttempts = 0;
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "plan_operation") {
        planCalls += 1;
        return { ...issuedPlanFor(), id: planCalls };
      }
      if (cmd === "submit_operation") {
        submitAttempts += 1;
        if (submitAttempts === 1) {
          throw "this plan is older than 10 minutes; preview it again";
        }
        return 7;
      }
      throw new Error(`unexpected command ${cmd}`);
    });
    const onSubmitted = vi.fn();

    renderWithProviders(
      <UninstallDialog
        open
        onOpenChange={() => {}}
        request={request}
        displayName="jq"
        onSubmitted={onSubmitted}
      />,
    );

    const confirmButton = await screen.findByRole("button", { name: "Uninstall" });
    await waitFor(() => expect(confirmButton).not.toBeDisabled());
    fireEvent.click(confirmButton);

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "this plan is older than 10 minutes; preview it again",
    );
    await waitFor(() => expect(planCalls).toBe(2));
    // Confirm re-enables only once the fresh plan has arrived; its preview is
    // on screen, but the fresh id has not been sent — the dead id is still
    // the only submit so far, and nothing was reported as started.
    await waitFor(() => expect(confirmButton).not.toBeDisabled());
    expect(screen.getByText("/opt/homebrew/bin/brew uninstall --formula jq")).toBeInTheDocument();
    expect(submitCalls().map(([, args]) => args)).toEqual([{ planId: 1 }]);
    expect(onSubmitted).not.toHaveBeenCalled();

    fireEvent.click(confirmButton);

    await waitFor(() => expect(onSubmitted).toHaveBeenCalledWith(7));
    expect(submitCalls().map(([, args]) => args)).toEqual([{ planId: 1 }, { planId: 2 }]);
  });

  it("ignores a submit that finishes after the dialog was retargeted", async () => {
    // Deviation from the brief, recorded in the task report: the brief's five
    // tests never exercise the dialog-session guard. A reply belonging to the
    // artifact the dialog has *stopped* showing must not report its op id nor
    // close the dialog now standing over a different artifact.
    let resolveSubmit: (opId: number) => void = () => {};
    vi.mocked(invoke).mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === "plan_operation") {
        const planned = (args as { request: OpRequest }).request;
        const issued = issuedPlanFor();
        return {
          ...issued,
          plan: {
            ...issued.plan,
            request: planned,
            args: ["uninstall", "--formula", planned.name],
          },
        };
      }
      if (cmd === "submit_operation") {
        return new Promise<number>((resolve) => {
          resolveSubmit = resolve;
        });
      }
      throw new Error(`unexpected command ${cmd}`);
    });
    const onSubmitted = vi.fn();
    const onOpenChange = vi.fn();

    const { rerender } = renderWithProviders(
      <UninstallDialog
        open
        onOpenChange={onOpenChange}
        request={request}
        displayName="jq"
        onSubmitted={onSubmitted}
      />,
    );

    const confirmButton = await screen.findByRole("button", { name: "Uninstall" });
    await waitFor(() => expect(confirmButton).not.toBeDisabled());
    fireEvent.click(confirmButton);
    await waitFor(() => expect(submitCalls()).toHaveLength(1));

    rerender(
      <UninstallDialog
        open
        onOpenChange={onOpenChange}
        request={{ ...request, name: "yq" }}
        displayName="yq"
        onSubmitted={onSubmitted}
      />,
    );
    await screen.findByText("/opt/homebrew/bin/brew uninstall --formula yq");

    resolveSubmit(7);

    // The settled submit re-enables confirm; that is the observable edge the
    // stale reply would have crossed on its way to `onSubmitted`.
    await waitFor(() => expect(confirmButton).not.toBeDisabled());
    expect(onSubmitted).not.toHaveBeenCalled();
    expect(onOpenChange).not.toHaveBeenCalled();
    expect(screen.getByText("/opt/homebrew/bin/brew uninstall --formula yq")).toBeInTheDocument();
  });
});
