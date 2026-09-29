import { StrictMode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, waitFor } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type EventCallback } from "@tauri-apps/api/event";
import { fakeMenuBar } from "../test/menuBar";
import { QUIT_REQUESTED_EVENT } from "./api";
import { commandUnderWay, quitBodyKey, useQuitRequests } from "./quit";
import type { OpKind, OpStatus, OpSummary } from "./types";

const mockInvoke = vi.mocked(invoke);

function op(id: number, kind: OpKind, status: OpStatus): OpSummary {
  return {
    id,
    kind,
    instance_id: "brew:/opt/homebrew",
    artifact_kind: "Formula",
    name: `tool-${id}`,
    status,
    outcome: null,
    argv_preview: [],
    cancel_policy: "KillThenReconcile",
  };
}

beforeEach(() => {
  mockInvoke.mockReset();
  mockInvoke.mockResolvedValue(undefined as never);
});

describe("whether an operation's command may be running", () => {
  it("is so while it runs or is being cancelled, and not while it waits or checks its result", () => {
    const under: OpStatus[] = ["Running", "CancelRequested", "Cancelling"];
    const not: OpStatus[] = ["Queued", "Verifying", "Done"];
    for (const status of under) expect(commandUnderWay(op(1, "Upgrade", status)), status).toBe(true);
    for (const status of not) expect(commandUnderWay(op(1, "Upgrade", status)), status).toBe(false);
  });
});

describe("what the question says under its title", () => {
  it("says only that quitting stops them when no command is under way", () => {
    // Queued: nothing started. Verifying: the command has ended.
    expect(quitBodyKey([op(1, "Upgrade", "Queued")])).toBe("quit.body.stops");
    expect(quitBodyKey([op(1, "Uninstall", "Verifying"), op(2, "Upgrade", "Queued")])).toBe(
      "quit.body.stops",
    );
  });

  it("says what can be left half done in the words of the kind under way", () => {
    expect(quitBodyKey([op(1, "Upgrade", "Running")])).toBe("quit.body.Upgrade");
    expect(quitBodyKey([op(1, "Uninstall", "Cancelling"), op(2, "Upgrade", "Queued")])).toBe(
      "quit.body.Uninstall",
    );
    expect(quitBodyKey([op(1, "Install", "CancelRequested")])).toBe("quit.body.Install");
    // Two updates running, one queued: still one kind under way.
    expect(
      quitBodyKey([op(1, "Upgrade", "Running"), op(2, "Upgrade", "Running"), op(3, "Uninstall", "Queued")]),
    ).toBe("quit.body.Upgrade");
  });

  it("says it in words for any kind when more than one kind is under way", () => {
    expect(quitBodyKey([op(1, "Upgrade", "Running"), op(2, "Uninstall", "Running")])).toBe("quit.body.mixed");
  });
});

describe("listening for the question", () => {
  function Listener({ onRequest }: { onRequest: () => void }) {
    useQuitRequests(onRequest);
    return null;
  }

  /** The commands the page sent, in order. */
  function sent(): string[] {
    return mockInvoke.mock.calls.map(([cmd]) => cmd);
  }

  it("tells Rust to ask before quitting once it listens, and not before", async () => {
    let finishListening: (() => void) | undefined;
    vi.mocked(listen).mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          finishListening = () => resolve(() => {});
        }),
    );
    render(<Listener onRequest={() => {}} />);

    await waitFor(() => expect(finishListening).toBeDefined());
    expect(sent()).toEqual([]);
    finishListening?.();

    await waitFor(() => expect(sent()).toEqual(["ask_before_quit"]));
  });

  it("calls back each time Rust asks, with the newest callback it was handed", async () => {
    const rust = fakeMenuBar();
    const first = vi.fn();
    const second = vi.fn();
    const { rerender } = render(<Listener onRequest={first} />);
    await waitFor(() => expect(sent()).toEqual(["ask_before_quit"]));

    rust.hear(QUIT_REQUESTED_EVENT);
    rerender(<Listener onRequest={second} />);
    rust.hear(QUIT_REQUESTED_EVENT);

    expect(first).toHaveBeenCalledTimes(1);
    expect(second).toHaveBeenCalledTimes(1);
    // Listened for once, however many renders.
    expect(sent()).toEqual(["ask_before_quit"]);
  });

  it("tells Rust nothing when it cannot listen, so that a quit quits at once", async () => {
    const error = vi.spyOn(console, "error").mockImplementation(() => {});
    vi.mocked(listen).mockRejectedValueOnce("event.listen not allowed");
    render(<Listener onRequest={() => {}} />);

    await waitFor(() => expect(error).toHaveBeenCalledWith("listening for Quit failed", expect.any(Error)));
    expect(sent()).toEqual([]);
    error.mockRestore();
  });

  it("listens once under StrictMode, tells Rust once, and stops listening when unmounted", async () => {
    // StrictMode mounts, unmounts and mounts again at once: the first
    // mount's listening, which resolves after its unmount, is stopped
    // then and tells Rust nothing.
    const listening = new Set<EventCallback<unknown>>();
    vi.mocked(listen).mockImplementation(async (event, handler) => {
      expect(event).toBe(QUIT_REQUESTED_EVENT);
      const callback = handler as EventCallback<unknown>;
      listening.add(callback);
      return () => {
        listening.delete(callback);
      };
    });
    const onRequest = vi.fn();
    const { unmount } = render(
      <StrictMode>
        <Listener onRequest={onRequest} />
      </StrictMode>,
    );
    await waitFor(() => expect(sent()).toEqual(["ask_before_quit"]));
    expect(listening.size).toBe(1);

    for (const callback of listening) callback({ event: QUIT_REQUESTED_EVENT, id: 1, payload: null });
    expect(onRequest).toHaveBeenCalledTimes(1);

    unmount();
    expect(listening.size).toBe(0);
    expect(sent()).toEqual(["ask_before_quit"]);
  });
});
