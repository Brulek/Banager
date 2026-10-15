import { StrictMode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, waitFor } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type EventCallback } from "@tauri-apps/api/event";
import { fakeMenuBar } from "../test/menuBar";
import { QUIT_REQUESTED_EVENT } from "./api";
import {
  QUIT_CHECKING_KEYS,
  commandUnderWay,
  quitBodyKey,
  quitLetsFinish,
  quitStops,
  tellRustTwice,
  useQuitRequests,
} from "./quit";
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

describe("which operations a quit lets finish", () => {
  it("is one checking its result, and no other", () => {
    // Its command has ended; the quit waits for the reading after it
    // (src-tauri/src/quit.rs, `waits_for`), so that its record is kept.
    expect(quitLetsFinish(op(1, "Upgrade", "Verifying"))).toBe(true);
    expect(quitLetsFinish(op(1, "Uninstall", "Verifying"))).toBe(true);
    const not: OpStatus[] = ["Queued", "Running", "CancelRequested", "Cancelling", "Done"];
    for (const status of not) expect(quitLetsFinish(op(1, "Upgrade", status)), status).toBe(false);
    // One that nothing can stop has a line of its own already.
    expect(quitLetsFinish({ ...op(1, "Upgrade", "Verifying"), cancel_policy: "NoCancel" })).toBe(false);
  });

  it("is not among what quitting stops, and has a line of its own in the words of its kind", () => {
    const checking = op(1, "Upgrade", "Verifying");
    expect(quitStops([checking, op(2, "Upgrade", "Queued")]).map((each) => each.id)).toEqual([2]);
    expect(QUIT_CHECKING_KEYS).toEqual({
      Install: "quit.checking.Install",
      Uninstall: "quit.checking.Uninstall",
      Upgrade: "quit.checking.Upgrade",
      Link: "noAnswer.op.quitChecking",
    });
  });
});

describe("what the question says under its title", () => {
  it("says only that quitting stops them when no command is under way", () => {
    // Queued: nothing started.
    expect(quitBodyKey([op(1, "Upgrade", "Queued")])).toBe("quit.body.stops");
    expect(quitBodyKey([op(1, "Upgrade", "Queued"), op(2, "Uninstall", "Queued")])).toBe("quit.body.stops");
  });

  it("does not say that quitting stops one checking its result, which it lets finish", () => {
    // r38 skeptic 1: the quit waits for it, 7 seconds at the most, and
    // its record is kept; its own line says so.
    expect(quitBodyKey([op(1, "Upgrade", "Verifying")])).toBeNull();
    expect(quitBodyKey([op(1, "Upgrade", "Verifying"), op(2, "Uninstall", "Verifying")])).toBeNull();
    // Beside it, what quitting stops is "the others".
    expect(quitBodyKey([op(1, "Uninstall", "Verifying"), op(2, "Upgrade", "Queued")])).toBe("quit.body.othersStop");
    expect(quitBodyKey([op(1, "Upgrade", "Verifying"), op(2, "Upgrade", "Running")])).toBe(
      "quit.body.othersHalfDone",
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

  it("says it of the others beside one that nothing can stop, and nothing with only such ones", () => {
    const rustup = (id: number, status: OpSummary["status"]) => ({
      ...op(id, "Upgrade", status),
      cancel_policy: "NoCancel" as const,
    });
    expect(quitBodyKey([rustup(1, "Running")])).toBeNull();
    expect(quitBodyKey([rustup(1, "Running"), op(2, "Upgrade", "Queued")])).toBe("quit.body.othersStop");
    expect(quitBodyKey([rustup(1, "Running"), op(2, "Uninstall", "Running")])).toBe("quit.body.othersHalfDone");
    expect(quitStops([rustup(1, "Running"), op(2, "Upgrade", "Queued")]).map((each) => each.id)).toEqual([2]);
    // Queued, rustup's update has started nothing, and quitting stops it.
    expect(quitBodyKey([rustup(1, "Queued")])).toBe("quit.body.stops");
  });

  it("says it in words for any kind when more than one kind is under way", () => {
    expect(quitBodyKey([op(1, "Upgrade", "Running"), op(2, "Uninstall", "Running")])).toBe("quit.body.mixed");
  });
});

describe("telling Rust about the question", () => {
  it("sends once when that gets through", async () => {
    const send = vi.fn(() => Promise.resolve());
    await tellRustTwice(send, "quit_question_shown");
    expect(send).toHaveBeenCalledTimes(1);
  });

  it("sends once more when the first fails, and no more than that", async () => {
    const error = vi.spyOn(console, "error").mockImplementation(() => {});
    const again = vi.fn().mockRejectedValueOnce("busy").mockResolvedValueOnce(undefined);
    await tellRustTwice(again, "quit_kept_waiting");
    expect(again).toHaveBeenCalledTimes(2);
    expect(error).toHaveBeenCalledWith("quit_kept_waiting failed, sending it once more", "busy");

    const never = vi.fn(() => Promise.reject("gone"));
    await tellRustTwice(never, "quit_kept_waiting");
    expect(never).toHaveBeenCalledTimes(2);
    expect(error).toHaveBeenLastCalledWith("quit_kept_waiting failed", "gone");
    error.mockRestore();
  });
});

describe("listening for the question", () => {
  function Listener({ onRequest }: { onRequest: (question: number) => void }) {
    useQuitRequests(onRequest);
    return null;
  }

  /** The commands the page sent, in order, with their arguments. */
  function sent(): unknown[][] {
    return mockInvoke.mock.calls.map((call) => [...call]);
  }

  const ASK = ["ask_before_quit", { ask: true }];
  const STOP_ASKING = ["ask_before_quit", { ask: false }];

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

    await waitFor(() => expect(sent()).toEqual([ASK]));
  });

  it("calls back each time Rust asks, with the question's number and the newest callback it was handed", async () => {
    const rust = fakeMenuBar();
    const first = vi.fn();
    const second = vi.fn();
    const { rerender } = render(<Listener onRequest={first} />);
    await waitFor(() => expect(sent()).toEqual([ASK]));

    rust.hear(QUIT_REQUESTED_EVENT, 1);
    rerender(<Listener onRequest={second} />);
    rust.hear(QUIT_REQUESTED_EVENT, 2);

    expect(first.mock.calls).toEqual([[1]]);
    expect(second.mock.calls).toEqual([[2]]);
    // Listened for once, however many renders.
    expect(sent()).toEqual([ASK]);
  });

  it("tells Rust to stop asking when unmounted, so that a quit quits at once again", async () => {
    const rust = fakeMenuBar();
    const { unmount } = render(<Listener onRequest={() => {}} />);
    await waitFor(() => expect(sent()).toEqual([ASK]));

    unmount();

    expect(sent()).toEqual([ASK, STOP_ASKING]);
    expect(rust.listening()).toEqual([]);
  });

  it("tells Rust to stop asking when an error in drawing the page takes it down", async () => {
    // Nothing catches the error: React takes the whole page down, and the
    // question with it -- nobody would be there to ask.
    const error = vi.spyOn(console, "error").mockImplementation(() => {});
    const reported = vi.fn();
    window.addEventListener("error", reported);
    function Breaks({ now }: { now: boolean }) {
      if (now) throw new Error("a row could not be drawn");
      return null;
    }
    const rust = fakeMenuBar();
    const page = (breaks: boolean) => (
      <>
        <Listener onRequest={() => {}} />
        <Breaks now={breaks} />
      </>
    );
    const { rerender } = render(page(false));
    await waitFor(() => expect(sent()).toEqual([ASK]));

    try {
      rerender(page(true));
    } catch {
      // Thrown back out of the render by React's `act`, as it may be.
    }

    await waitFor(() => expect(sent()).toEqual([ASK, STOP_ASKING]));
    expect(rust.listening()).toEqual([]);
    window.removeEventListener("error", reported);
    error.mockRestore();
  });

  it("tells Rust nothing when it cannot listen, so that a quit quits at once, and nothing when unmounted", async () => {
    const error = vi.spyOn(console, "error").mockImplementation(() => {});
    vi.mocked(listen).mockRejectedValueOnce("event.listen not allowed");
    const { unmount } = render(<Listener onRequest={() => {}} />);

    await waitFor(() => expect(error).toHaveBeenCalledWith("listening for Quit failed", expect.any(Error)));
    expect(sent()).toEqual([]);
    unmount();
    expect(sent()).toEqual([]);
    error.mockRestore();
  });

  it("listens once under StrictMode, tells Rust once, and when unmounted stops listening and tells Rust to stop", async () => {
    // StrictMode mounts, unmounts and mounts again at once: the first
    // mount's listening, which resolves after its unmount, is stopped
    // then, and tells Rust nothing, either way.
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
    await waitFor(() => expect(sent()).toEqual([ASK]));
    expect(listening.size).toBe(1);

    for (const callback of listening) callback({ event: QUIT_REQUESTED_EVENT, id: 1, payload: 1 });
    expect(onRequest.mock.calls).toEqual([[1]]);

    unmount();
    expect(listening.size).toBe(0);
    expect(sent()).toEqual([ASK, STOP_ASKING]);
  });

  it("says why when Rust cannot be told to stop asking", async () => {
    // Rust then asks a page that is gone, and quits once nothing has said
    // the question is on screen (src-tauri/src/quit.rs).
    const error = vi.spyOn(console, "error").mockImplementation(() => {});
    fakeMenuBar();
    const { unmount } = render(<Listener onRequest={() => {}} />);
    await waitFor(() => expect(sent()).toEqual([ASK]));
    mockInvoke.mockRejectedValueOnce("the window is gone");

    unmount();

    await waitFor(() => expect(error).toHaveBeenCalledWith("ask_before_quit failed", new Error("the window is gone")),
    );
    error.mockRestore();
  });
});
