import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";
import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import { InstalledPage } from "./InstalledPage";
import { UpdatesPage } from "./UpdatesPage";
import { SnapshotStatus } from "../components/SnapshotStatus";
import type {
  InstalledArtifact,
  OpRequest,
  Settings,
  Snapshot,
  UpdateCandidate,
} from "../lib/types";

const mockInvoke = vi.mocked(invoke);

const snapshot: Snapshot = {
  generation: 1,
  detect: "Found",
  instances: [
    {
      id: "brew:/opt/homebrew",
      adapter_id: "brew",
      exe_path: "/opt/homebrew/bin/brew",
      prefix: "/opt/homebrew",
      scope: "User",
      version: "7.0.3",
      status: { unavailable: null, notes: [] },
      unverified_version: null,
      read_only_reason: null,
    },
  ],
  artifacts: [
    {
      key: { instance_id: "brew:/opt/homebrew", kind: "Formula", name: "jq" },
      display_name: "jq",
      version: "1.8.2",
      reason: "Requested",
      description: "Lightweight and flexible command-line JSON processor",
      homepage: "https://jqlang.github.io/jq/",
      size_bytes: null,
      installed_at: 1783762037,
      path: null,
      auto_updates: false,
      uninstall_blocked: null,
    },
    {
      key: { instance_id: "brew:/opt/homebrew", kind: "Formula", name: "glib" },
      display_name: "glib",
      version: "2.88.3",
      reason: "Dependency",
      description: "Core application library for C",
      homepage: "https://docs.gtk.org/glib/",
      size_bytes: null,
      installed_at: 1788244409,
      path: null,
      auto_updates: false,
      uninstall_blocked: null,
    },
  ],
  updates: [
    {
      key: { instance_id: "brew:/opt/homebrew", kind: "Formula", name: "glib" },
      current: "2.88.3",
      target: "2.90.0",
      channel: "Native",
      checkable: true,
      warnings: [],
      blocked: null,
    },
  ],
  refreshed_at: 1789700000,
  stale: false,
  errors: [],
};

const settings: Settings = {
  language: "System",
  show_technical_details: false,
  ignored_updates: [],
  include_self_updating: false,
};

// One pip instance with one package: row 0 is the group header plus the
// read-only SourceNotice, row 1 is the package.
const pipSnapshot: Snapshot = {
  generation: 1,
  detect: "Found",
  instances: [
    {
      id: "pip:/usr/bin/python3",
      adapter_id: "pip",
      exe_path: "/usr/bin/python3",
      prefix: "/usr",
      scope: "User",
      version: "26.2.1",
      status: { unavailable: null, notes: [] },
      unverified_version: null,
      read_only_reason: "ByDesign",
    },
  ],
  artifacts: [
    {
      key: { instance_id: "pip:/usr/bin/python3", kind: "Package", name: "requests" },
      display_name: "requests",
      version: "2.32.3",
      // Unknown, not Requested: pip's `--not-required` marks a leaf
      // package, which is not the same as "the user asked for it", so
      // Task 8's adapter can only ever emit Unknown or Dependency here.
      // A "Requested" fixture would pass against data pip cannot produce.
      reason: "Unknown",
      description: "Python HTTP for Humans.",
      homepage: null,
      size_bytes: null,
      installed_at: null,
      path: null,
      auto_updates: false,
      uninstall_blocked: null,
    },
  ],
  updates: [],
  refreshed_at: 1789700000,
  stale: false,
  errors: [],
};

// Heights a row reports to the virtualizer. `rowHeights` lets one test make a
// single row taller than the rest, which is how the SourceNotice case is
// exercised; every other row falls back to DEFAULT_ROW_HEIGHT.
const DEFAULT_ROW_HEIGHT = 56;
let rowHeights: Record<number, number> = {};

beforeEach(() => {
  mockInvoke.mockReset();
  rowHeights = {};
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({
    width: 800,
    height: 600,
    top: 0,
    left: 0,
    bottom: 600,
    right: 800,
    x: 0,
    y: 0,
    toJSON: () => {},
  } as DOMRect);
  // @tanstack/react-virtual measures both its scroll container (virtual-core's
  // `getRect`) and each individual row (`measureElement`) through offsetWidth /
  // offsetHeight, not getBoundingClientRect. jsdom hardcodes both offset
  // properties to 0 with no layout engine behind them, so without this the
  // virtualizer sees a zero-size viewport and renders no rows at all,
  // regardless of the getBoundingClientRect stub above. Deviation from the
  // brief's transcribed test, recorded in the task report. The rows are the
  // elements carrying `data-index`; everything else, the scroll container
  // included, gets the viewport height.
  vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockImplementation(function (
    this: HTMLElement,
  ) {
    const index = this.getAttribute("data-index");
    if (index === null) return 600;
    return rowHeights[Number(index)] ?? DEFAULT_ROW_HEIGHT;
  });
  vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(800);
  mockInvoke.mockImplementation((cmd: string) => {
    if (cmd === "get_snapshot") return Promise.resolve(snapshot);
    if (cmd === "get_settings") return Promise.resolve(settings);
    return Promise.resolve(undefined);
  });
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe("InstalledPage", () => {
  it("shows the requested artifact and collapses the dependency behind a toggle", async () => {
    const { findByText, queryByText, getByRole } = renderWithProviders(<InstalledPage />);

    await findByText("jq");
    expect(queryByText("glib")).not.toBeInTheDocument();
    expect(
      getByRole("button", { name: "1 component installed by other software" }),
    ).toBeInTheDocument();
  });

  it("reveals the dependency once the toggle is clicked", async () => {
    const { findByText, getByRole } = renderWithProviders(<InstalledPage />);

    await findByText("jq");
    fireEvent.click(getByRole("button", { name: "1 component installed by other software" }));

    await findByText("glib");
  });

  it("filters rows by the query box", async () => {
    const { findByText, queryByText, getByLabelText } = renderWithProviders(<InstalledPage />);

    await findByText("jq");
    fireEvent.change(getByLabelText("Filter installed items"), {
      target: { value: "nonexistent" },
    });

    // Controller's binding note: interactions that assert on DOM changes use
    // fireEvent + findBy/waitFor, never a synchronous assertion right after
    // the interaction. The state path (Zustand setQuery -> synchronous
    // re-render) makes this unlikely to flake today, but wrapping it in
    // waitFor keeps the test robust if the query update ever becomes async
    // (e.g. a debounced filter).
    await waitFor(() => expect(queryByText("jq")).not.toBeInTheDocument());
  });

  it("opens the uninstall dialog and plans it when the row's primary button is clicked", async () => {
    mockInvoke.mockImplementation((cmd: string, args?: unknown) => {
      if (cmd === "get_snapshot") return Promise.resolve(snapshot);
      if (cmd === "get_settings") return Promise.resolve(settings);
      if (cmd === "plan_operation") {
        return Promise.resolve({
          id: 1,
          plan: {
            request: (args as { request: OpRequest }).request,
            program: "/opt/homebrew/bin/brew",
            args: ["uninstall", "--formula", "jq"],
            env: [],
            needs_password: false,
            locks: ["brew:/opt/homebrew"],
            cancel_policy: "KillThenReconcile",
            warnings: [],
            affected: [],
            timeout_secs: 1800,
          },
          issued_at: 1758000000,
        });
      }
      return Promise.resolve(undefined);
    });

    const { findByText, getByRole, findByRole } = renderWithProviders(<InstalledPage />);

    await findByText("jq");
    fireEvent.click(getByRole("button", { name: "Uninstall" }));

    const dialog = await findByRole("dialog");
    expect(mockInvoke).toHaveBeenCalledWith("plan_operation", {
      request: {
        kind: "Uninstall",
        instance_id: "brew:/opt/homebrew",
        artifact_kind: "Formula",
        name: "jq",
      },
    });
    await within(dialog).findByText("/opt/homebrew/bin/brew uninstall --formula jq");
  });

  it("disables the dialog's confirm button when the plan reports dependents", async () => {
    mockInvoke.mockImplementation((cmd: string, args?: unknown) => {
      if (cmd === "get_snapshot") return Promise.resolve(snapshot);
      if (cmd === "get_settings") return Promise.resolve(settings);
      if (cmd === "plan_operation") {
        return Promise.resolve({
          id: 1,
          plan: {
            request: (args as { request: OpRequest }).request,
            program: "/opt/homebrew/bin/brew",
            args: ["uninstall", "--formula", "jq"],
            env: [],
            needs_password: false,
            locks: ["brew:/opt/homebrew"],
            cancel_policy: "KillThenReconcile",
            warnings: [],
            affected: ["jq-cli-wrapper"],
            timeout_secs: 1800,
          },
          issued_at: 1758000000,
        });
      }
      return Promise.resolve(undefined);
    });

    const { findByText, getByRole, findByRole } = renderWithProviders(<InstalledPage />);

    await findByText("jq");
    fireEvent.click(getByRole("button", { name: "Uninstall" }));

    const dialog = await findByRole("dialog");
    await within(dialog).findByText("jq-cli-wrapper");
    expect(within(dialog).getByRole("button", { name: "Uninstall" })).toBeDisabled();
  });

  it("shows an unverified-version badge next to a source whose detected version is not verified", async () => {
    const unverifiedSnapshot: Snapshot = {
      ...snapshot,
      instances: [{ ...snapshot.instances[0], unverified_version: "99.9.9" }],
    };
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "get_snapshot") return Promise.resolve(unverifiedSnapshot);
      if (cmd === "get_settings") return Promise.resolve(settings);
      return Promise.resolve(undefined);
    });

    const { findByText } = renderWithProviders(<InstalledPage />);

    await findByText("jq");
    await findByText("Unverified version (99.9.9)");
  });

  it("offers no Uninstall on a pinned package, and says how to release the pin", async () => {
    // `brew uninstall jq` refuses a pinned formula without `--force` and
    // still exits 0 (`UninstallBlocked::Pinned` in
    // crates/canager-core/src/model.rs), so the row must not offer it. jq
    // is up to date: the pin comes from the inventory, not from an update.
    // The cask shows the `--cask` form; the unpinned formula keeps its button.
    const pinnedSnapshot: Snapshot = {
      ...snapshot,
      artifacts: [
        { ...snapshot.artifacts[0], uninstall_blocked: "Pinned" },
        {
          ...snapshot.artifacts[0],
          key: { instance_id: "brew:/opt/homebrew", kind: "Cask", name: "onyx" },
          display_name: "OnyX",
          description: "Verify system files structure",
          uninstall_blocked: "Pinned",
        },
        {
          ...snapshot.artifacts[0],
          key: { instance_id: "brew:/opt/homebrew", kind: "Formula", name: "wget" },
          display_name: "wget",
          description: "Internet file retriever",
        },
      ],
      updates: [],
    };
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "get_snapshot") return Promise.resolve(pinnedSnapshot);
      if (cmd === "get_settings") return Promise.resolve(settings);
      return Promise.resolve(undefined);
    });

    const { findByText, getAllByRole, getByText, queryByText } = renderWithProviders(
      <InstalledPage />,
    );

    await findByText("wget");
    // Only wget's.
    expect(getAllByRole("button", { name: "Uninstall" })).toHaveLength(1);
    expect(
      getByText(
        (_content, element) =>
          element?.tagName === "P" &&
          element.textContent ===
            "This has been pinned in Homebrew, and Homebrew won't remove a pinned package, so Canager doesn't offer to uninstall it. To uninstall it, first run /opt/homebrew/bin/brew unpin jq in Terminal to release the pin; Canager will offer to uninstall it the next time it checks, which at the latest is the next time you start Canager.",
      ),
    ).toBeInTheDocument();
    expect(getByText("/opt/homebrew/bin/brew unpin jq").tagName).toBe("CODE");
    expect(getByText("/opt/homebrew/bin/brew unpin --cask onyx").tagName).toBe("CODE");
    // The explanation replaces the blurb; the unpinned row keeps its own.
    expect(queryByText("Lightweight and flexible command-line JSON processor")).toBeNull();
    expect(getByText("Internet file retriever")).toBeInTheDocument();
  });

  it("promises Uninstall back on a silent source's pinned row only once the source answers", async () => {
    // A row carried forward from a Homebrew that did not answer has no
    // Uninstall button until Homebrew answers a check again, pinned or
    // not (`actionable` needs `isAvailable`). "The next time it checks,
    // which at the latest is the next time you start Canager" would not
    // hold while Homebrew stays silent.
    const silentSnapshot: Snapshot = {
      ...snapshot,
      instances: [
        { ...snapshot.instances[0], status: { unavailable: "NotResponding", notes: [] } },
      ],
      artifacts: [{ ...snapshot.artifacts[0], uninstall_blocked: "Pinned" }],
      updates: [],
    };
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "get_snapshot") return Promise.resolve(silentSnapshot);
      if (cmd === "get_settings") return Promise.resolve(settings);
      return Promise.resolve(undefined);
    });

    const { findByText, getByText, queryAllByRole, queryByText } = renderWithProviders(
      <InstalledPage />,
    );

    await findByText("jq");
    expect(queryAllByRole("button", { name: "Uninstall" })).toHaveLength(0);
    expect(
      getByText(
        (_content, element) =>
          element?.tagName === "P" &&
          element.textContent ===
            "This has been pinned in Homebrew, and Homebrew won't remove a pinned package, so Canager doesn't offer to uninstall it. To uninstall it, first run /opt/homebrew/bin/brew unpin jq in Terminal to release the pin; after that, Canager will offer to uninstall it the next time it checks and Homebrew answers.",
      ),
    ).toBeInTheDocument();
    expect(getByText("/opt/homebrew/bin/brew unpin jq").tagName).toBe("CODE");
    expect(queryByText(/next time you start Canager/)).toBeNull();
  });

  it("offers no Uninstall on a tool with no safe uninstall method, and says so without a command", async () => {
    // `UninstallBlocked::NoSafeMethod` (phase 4): the tool has no
    // uninstall command and Canager has no safe way yet to remove its
    // files, so the row explains itself in place of its blurb and hides
    // the button -- and, unlike a pin, sets no command as code, because
    // there is nothing to run first. `Session::issue_plan` refuses it in
    // Rust too.
    const claudeSnapshot: Snapshot = {
      ...snapshot,
      instances: [
        {
          id: "standalone-claude",
          adapter_id: "standalone-claude",
          exe_path: "/Users/someone/.local/bin/claude",
          prefix: "/Users/someone/.local/share/claude",
          scope: "User",
          version: "2.1.281",
          status: { unavailable: null, notes: [] },
          unverified_version: null,
          read_only_reason: null,
        },
      ],
      artifacts: [
        {
          key: { instance_id: "standalone-claude", kind: "Binary", name: "claude" },
          display_name: "Claude Code",
          version: "2.1.281",
          reason: "Requested",
          description: null,
          homepage: "https://code.claude.com/docs/en/setup",
          size_bytes: null,
          installed_at: null,
          path: "/Users/someone/.local/share/claude/versions/2.1.281",
          auto_updates: true,
          uninstall_blocked: "NoSafeMethod",
        },
      ],
      updates: [],
    };
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "get_snapshot") return Promise.resolve(claudeSnapshot);
      if (cmd === "get_settings") return Promise.resolve(settings);
      return Promise.resolve(undefined);
    });

    const { findByText, getByText, queryAllByRole, container } = renderWithProviders(
      <InstalledPage />,
    );

    await findByText("Can't uninstall here");
    expect(queryAllByRole("button", { name: "Uninstall" })).toHaveLength(0);
    expect(
      getByText(
        "Claude Code has no uninstall command, and Canager can't yet move its files to the Trash safely, so it doesn't offer to. Claude Code's official documentation explains how to uninstall it.",
      ),
    ).toBeInTheDocument();
    expect(container.querySelector("code")).toBeNull();
  });

  it("shows the standalone summary alongside its real uninstall refusal", async () => {
    // A standalone artifact carries `description: null` (the sentence has
    // to be localised, so its key lives in `STANDALONE_SUMMARY_KEYS`); a
    // Homebrew package with no blurb keeps "No description available".
    const mixed: Snapshot = {
      ...snapshot,
      instances: [
        snapshot.instances[0],
        {
          id: "standalone-claude",
          adapter_id: "standalone-claude",
          exe_path: "/Users/someone/.local/bin/claude",
          prefix: "/Users/someone/.local/share/claude",
          scope: "User",
          version: "2.1.281",
          status: { unavailable: null, notes: [] },
          unverified_version: null,
          read_only_reason: null,
        },
      ],
      artifacts: [
        { ...snapshot.artifacts[0], description: null },
        {
          key: { instance_id: "standalone-claude", kind: "Binary", name: "claude" },
          display_name: "Claude Code",
          version: "2.1.281",
          reason: "Requested",
          description: null,
          homepage: "https://code.claude.com/docs/en/setup",
          size_bytes: null,
          installed_at: null,
          path: "/Users/someone/.local/share/claude/versions/2.1.281",
          auto_updates: true,
          uninstall_blocked: "NoSafeMethod",
        },
      ],
      updates: [],
    };
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "get_snapshot") return Promise.resolve(mixed);
      if (cmd === "get_settings") return Promise.resolve(settings);
      return Promise.resolve(undefined);
    });

    const { findByText, getByText, queryAllByRole } = renderWithProviders(<InstalledPage />);

    expect(
      await findByText(
        "Anthropic's coding assistant for the terminal. Installed with its own installer, not with Homebrew or npm.",
      ),
    ).toBeInTheDocument();
    expect(getByText("No description available")).toBeInTheDocument();
    expect(getByText("Claude Code has no uninstall command, and Canager can't yet move its files to the Trash safely, so it doesn't offer to. Claude Code's official documentation explains how to uninstall it.")).toBeInTheDocument();
    // Only the Homebrew artifact may offer Uninstall; B's actual Claude
    // artifact is NoSafeMethod and must still show both sentences.
    expect(queryAllByRole("button", { name: "Uninstall" })).toHaveLength(1);
  });

  describe("the Update available badge", () => {
    // One snapshot with one package per reason the Updates page may list
    // an update and not offer it, plus one it does offer. The badge used
    // to say "Update available" for every entry in `snapshot.updates`, and
    // then still for a source that did not answer (a stopped Ollama whose
    // update was carried forward), which has no Update button either.
    const OLLAMA = "ollama:http://127.0.0.1:11434";
    const artifact = (name: string, over: Partial<InstalledArtifact> = {}): InstalledArtifact => ({
      ...snapshot.artifacts[0],
      key: { instance_id: "brew:/opt/homebrew", kind: "Formula", name },
      display_name: name,
      description: `${name} blurb`,
      ...over,
    });
    const update = (
      name: string,
      over: Partial<UpdateCandidate> = {},
    ): UpdateCandidate => ({
      ...snapshot.updates[0],
      key: { instance_id: "brew:/opt/homebrew", kind: "Formula", name },
      ...over,
    });
    const mixed: Snapshot = {
      ...snapshot,
      artifacts: [
        artifact("offered"),
        artifact("pinned-outdated", { uninstall_blocked: "Pinned" }),
        artifact("pipx-pinned", {
          key: { instance_id: "pipx", kind: "Tool", name: "pipx-pinned" },
        }),
        artifact("unchecked"),
        artifact("ignored"),
        artifact("pinned-current", { uninstall_blocked: "Pinned" }),
        artifact("current"),
        artifact("stopped-model", {
          key: { instance_id: OLLAMA, kind: "Model", name: "stopped-model" },
        }),
      ],
      instances: [
        ...snapshot.instances,
        {
          ...snapshot.instances[0],
          id: "pipx",
          adapter_id: "pipx",
          exe_path: "/opt/homebrew/bin/pipx",
          prefix: "/Users/a/.local",
        },
        {
          ...snapshot.instances[0],
          id: OLLAMA,
          adapter_id: "ollama",
          exe_path: "/usr/local/bin/ollama",
          prefix: "/Users/a/.ollama",
          version: "0.13.0",
          status: { unavailable: "NotRunning", notes: [] },
        },
      ],
      updates: [
        update("offered"),
        update("pinned-outdated", { blocked: "Pinned" }),
        update("pipx-pinned", {
          key: { instance_id: "pipx", kind: "Tool", name: "pipx-pinned" },
          channel: "Registry",
          blocked: "Pinned",
        }),
        update("unchecked", { checkable: false, warnings: [{ Message: "timed out" }] }),
        update("ignored"),
        update("stopped-model", {
          key: { instance_id: OLLAMA, kind: "Model", name: "stopped-model" },
          channel: "Registry",
        }),
      ],
    };
    const mixedSettings: Settings = {
      ...settings,
      ignored_updates: [{ instance_id: "brew:/opt/homebrew", kind: "Formula", name: "ignored" }],
    };

    beforeEach(() => {
      mockInvoke.mockImplementation((cmd: string) => {
        if (cmd === "get_snapshot") return Promise.resolve(mixed);
        if (cmd === "get_settings") return Promise.resolve(mixedSettings);
        return Promise.resolve(undefined);
      });
    });

    /** The badge on `name`'s row: the row is the name's grandparent. */
    function badgeOf(container: HTMLElement, name: string): string | null | undefined {
      const nameEl = within(container).getByText(name, { selector: "p" });
      return nameEl.parentElement?.parentElement?.querySelector("span.rounded-full")?.textContent;
    }

    it("says Update available only for an update the Updates page offers", async () => {
      const { container, findByText } = renderWithProviders(<InstalledPage />);
      await findByText("current");

      expect(badgeOf(container, "offered")).toBe("Update available");
      expect(badgeOf(container, "pinned-outdated")).toBe("Pinned");
      expect(badgeOf(container, "pipx-pinned")).toBe("Pinned");
      expect(badgeOf(container, "unchecked")).toBe("Can't check");
      expect(badgeOf(container, "ignored")).toBe("Update ignored");
      // Pinned in Homebrew and up to date: still pinned, from the inventory.
      expect(badgeOf(container, "pinned-current")).toBe("Pinned");
      expect(badgeOf(container, "current")).toBe("Up to date");
      // Its source is not running, so there is no Update button for it.
      expect(badgeOf(container, "stopped-model")).toBe("Newer version, can't update now");
    });

    it("agrees with the Updates page's buttons row for row", async () => {
      const installed = renderWithProviders(<InstalledPage />);
      await installed.findByText("current");
      const badged = mixed.artifacts
        .map((a) => a.display_name)
        .filter((name) => badgeOf(installed.container, name) === "Update available");
      installed.unmount();

      const updates = renderWithProviders(<UpdatesPage />);
      await updates.findByText("offered");
      const offered = mixed.updates
        .map((u) => u.key.name)
        .filter((name) => {
          const row = updates.queryByText(name, { selector: "p" })?.parentElement?.parentElement;
          return row ? within(row).queryByRole("button", { name: "Update" }) !== null : false;
        });

      // The stopped source's update is listed there, with no button: it is
      // left out of `offered` for that, not for a missing row.
      expect(updates.queryByText("stopped-model", { selector: "p" })).not.toBeNull();
      expect(badged).toEqual(["offered"]);
      expect(offered).toEqual(badged);
    });
  });

  it("hides the uninstall button and shows a read-only note for pip rows", async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "get_snapshot") return Promise.resolve(pipSnapshot);
      if (cmd === "get_settings") return Promise.resolve(settings);
      return Promise.resolve(undefined);
    });

    const { findByText, queryByRole } = renderWithProviders(<InstalledPage />);

    await findByText("requests");
    expect(queryByRole("button", { name: "Uninstall" })).not.toBeInTheDocument();
    expect(await findByText("Read-only: pip packages")).toBeInTheDocument();
  });

  it("lets each row measure itself so a source notice cannot be overlapped by the row below it", async () => {
    // A group header that carries a SourceNotice is a title line plus a
    // banner -- taller than the flat estimate every row used to be pinned to.
    // jsdom has no layout engine, so the height comes from the mock above;
    // what this test checks is that the virtualizer *reads* it. Two things
    // have to hold: the next row's offset follows the measured size, and no
    // row carries a fixed inline height. With a fixed height the banner
    // overflows its slot and the following row -- later in DOM order, so
    // painted on top -- covers its tail, which for the Ollama notice is the
    // "Open Ollama" button.
    rowHeights[0] = 128;
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "get_snapshot") return Promise.resolve(pipSnapshot);
      if (cmd === "get_settings") return Promise.resolve(settings);
      return Promise.resolve(undefined);
    });

    const { findByText, container } = renderWithProviders(<InstalledPage />);

    await findByText("Read-only: pip packages");
    const rowAt = (index: number) =>
      container.querySelector<HTMLElement>(`[data-index="${index}"]`);

    await waitFor(() => expect(rowAt(1)?.style.transform).toBe("translateY(128px)"));
    expect(rowAt(0)?.style.height).toBe("");
    expect(rowAt(1)?.style.height).toBe("");
  });

  it("names a silent source and says Canager cannot reach it, instead of dropping its group", async () => {
    // brew, npm, uv, pipx and cargo can all report `NotResponding`,
    // and it means the same thing for all five: the CLI is on PATH but
    // Canager could not talk to it. The backend keeps such an instance in
    // `snapshot.instances` precisely so the UI can say so -- it pushes no
    // error, so this notice is the only place the user can learn that their
    // global npm packages are missing from the list rather than gone.
    const silentNpmSnapshot: Snapshot = {
      ...snapshot,
      instances: [
        ...snapshot.instances,
        {
          id: "npm:/opt/homebrew/lib",
          adapter_id: "npm",
          exe_path: "/opt/homebrew/bin/npm",
          prefix: "/opt/homebrew/lib",
          scope: "User",
          version: "11.2.0",
          status: { unavailable: "NotResponding", notes: [] },
          unverified_version: null,
          read_only_reason: null,
        },
      ],
    };
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "get_snapshot") return Promise.resolve(silentNpmSnapshot);
      if (cmd === "get_settings") return Promise.resolve(settings);
      return Promise.resolve(undefined);
    });

    const { findByText, queryByText } = renderWithProviders(<InstalledPage />);

    await findByText("jq");
    expect(await findByText("Canager can't reach npm right now")).toBeInTheDocument();
    // Nothing was carried forward for npm -- and nothing ever is on the
    // first refresh after a launch, because the snapshot is in memory
    // only (`Session::new` starts from `Snapshot::empty()`). The notice
    // used to say "Below is what Canager saw last time" over an empty
    // group, which for a source whose CLI simply fails is every launch,
    // forever.
    expect(
      await findByText(
        "npm is installed but didn't answer, so Canager doesn't know what's in it right now.",
      ),
    ).toBeInTheDocument();
    expect(queryByText(/What's listed here/)).not.toBeInTheDocument();
    // And no promise of a recovery that may never come.
    expect(queryByText(/Reopening Canager/)).not.toBeInTheDocument();
  });

  it("says what is listed is last time's answer when a silent source did carry rows forward", async () => {
    // The other half of the same sentence. `refresh` keeps an unavailable
    // source's last known artifacts, so once there has been a good
    // refresh these rows are real and the user needs telling how old they
    // are.
    const silentBrewSnapshot: Snapshot = {
      ...snapshot,
      instances: [
        { ...snapshot.instances[0], status: { unavailable: "NotResponding", notes: [] } },
      ],
    };
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "get_snapshot") return Promise.resolve(silentBrewSnapshot);
      if (cmd === "get_settings") return Promise.resolve(settings);
      return Promise.resolve(undefined);
    });

    const { findByText, queryByText } = renderWithProviders(<InstalledPage />);

    await findByText("jq");
    expect(
      await findByText(
        "Homebrew is installed but didn't answer. What's listed here is what Canager saw the last time it did, so anything added or removed since then is missing.",
      ),
    ).toBeInTheDocument();
    expect(queryByText(/doesn't know what's in it right now/)).not.toBeInTheDocument();
  });

  it("gives a root-owned npm prefix its own notice and no Uninstall button", async () => {
    // Read-only, like pip, but for a reason pip's copy would misdescribe:
    // the tool can install and uninstall perfectly well, it just cannot
    // write where this machine put it. The fix is to reinstall Node with
    // Homebrew, and telling this user about pipx or uv is noise.
    const readOnlyNpmSnapshot: Snapshot = {
      generation: 1,
      detect: "Found",
      instances: [
        {
          id: "npm:/usr/local",
          adapter_id: "npm",
          exe_path: "/usr/local/bin/npm",
          prefix: "/usr/local",
          scope: "User",
          version: "12.0.2",
          status: { unavailable: null, notes: [] },
          unverified_version: null,
          read_only_reason: "PrefixNotWritable",
        },
      ],
      artifacts: [
        {
          key: { instance_id: "npm:/usr/local", kind: "Package", name: "typescript" },
          display_name: "typescript",
          version: "5.6.2",
          reason: "Requested",
          description: "TypeScript is a language for application scale JavaScript development",
          homepage: null,
          size_bytes: null,
          installed_at: null,
          path: null,
          auto_updates: false,
          uninstall_blocked: null,
        },
      ],
      updates: [],
      refreshed_at: 1789700000,
      stale: false,
      errors: [],
    };
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "get_snapshot") return Promise.resolve(readOnlyNpmSnapshot);
      if (cmd === "get_settings") return Promise.resolve(settings);
      return Promise.resolve(undefined);
    });

    const { findByText, queryByText, queryAllByRole } = renderWithProviders(<InstalledPage />);

    await findByText("typescript");
    expect(await findByText("Read-only: npm packages")).toBeInTheDocument();
    expect(queryByText("Read-only: pip packages")).not.toBeInTheDocument();
    expect(queryAllByRole("button", { name: "Uninstall" })).toHaveLength(0);
  });

  it("never appends a digest to a model's name, while other sources still show their version", async () => {
    // An Ollama model's `version` is the local manifest digest, not a
    // version number. Printing it turned every model row into
    // "qwen3:8b · 5642e97495e1a0888838…", a 64-hex string shown to someone
    // who does not write code. The Formula below shares the setting and
    // must still get its version, so the suppression stays on ArtifactKind.
    const modelSnapshot: Snapshot = {
      ...snapshot,
      instances: [
        ...snapshot.instances,
        {
          id: "ollama:http://127.0.0.1:11434",
          adapter_id: "ollama",
          exe_path: "/usr/local/bin/ollama",
          prefix: "/usr/local",
          scope: "User",
          version: null,
          status: { unavailable: null, notes: [] },
          unverified_version: null,
          read_only_reason: null,
        },
      ],
      artifacts: [
        ...snapshot.artifacts,
        {
          key: { instance_id: "ollama:http://127.0.0.1:11434", kind: "Model", name: "qwen3:8b" },
          display_name: "qwen3:8b",
          version: "5642e97495e1a0888838ee1b3b1a0b1c6a0f0f5e6c2d4a8b9e7c3d1f0a2b4c6d",
          reason: "Requested",
          description: null,
          homepage: null,
          size_bytes: null,
          installed_at: null,
          path: null,
          auto_updates: false,
          uninstall_blocked: null,
        },
      ],
    };
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "get_snapshot") return Promise.resolve(modelSnapshot);
      if (cmd === "get_settings")
        return Promise.resolve({ ...settings, show_technical_details: true });
      return Promise.resolve(undefined);
    });

    const { findByText, queryByText } = renderWithProviders(<InstalledPage />);

    await findByText("qwen3:8b");
    expect(queryByText(/5642e97495e1a0888838/)).not.toBeInTheDocument();
    expect(await findByText("jq · 1.8.2")).toBeInTheDocument();
  });

  // Rendered through SnapshotStatus, exactly as App.tsx does. Rendering
  // InstalledPage on its own would bypass the gate the real app always goes
  // through, and this snapshot -- a stopped Ollama and nothing installed
  // anywhere -- is precisely the one that gate used to swallow.
  it("shows a not-running notice with an Open Ollama button when the daemon is not running", async () => {
    const ollamaSnapshot: Snapshot = {
      generation: 1,
      detect: "Found",
      instances: [
        {
          id: "ollama:http://127.0.0.1:11434",
          adapter_id: "ollama",
          exe_path: "/usr/local/bin/ollama",
          prefix: "/usr/local",
          scope: "User",
          version: null,
          status: { unavailable: "NotRunning", notes: [] },
          unverified_version: null,
          read_only_reason: null,
        },
      ],
      artifacts: [],
      updates: [],
      refreshed_at: 1789700000,
      stale: false,
      errors: [],
    };
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "get_snapshot") return Promise.resolve(ollamaSnapshot);
      if (cmd === "get_settings") return Promise.resolve(settings);
      if (cmd === "open_ollama_app") return Promise.resolve(undefined);
      return Promise.resolve(undefined);
    });

    const { findByText, getByRole } = renderWithProviders(
      <SnapshotStatus>
        <InstalledPage />
      </SnapshotStatus>,
    );

    await findByText("Ollama isn't running");
    fireEvent.click(getByRole("button", { name: "Open Ollama" }));

    await waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("open_ollama_app"));
  });

  it("says why Open Ollama did nothing when there is no Ollama app to open", async () => {
    // The button used to be able to fail in silence: the backend never
    // read `open`'s exit status, and even once it did, its structured
    // rejection had nothing on this side to turn it into words. A snapshot
    // taken before the app was removed still shows the button, so this is
    // the path a real person can reach.
    const ollamaSnapshot: Snapshot = {
      generation: 1,
      detect: "Found",
      instances: [
        {
          id: "ollama:http://127.0.0.1:11434",
          adapter_id: "ollama",
          exe_path: "/usr/local/bin/ollama",
          prefix: "/usr/local",
          scope: "User",
          version: null,
          status: { unavailable: "NotRunning", notes: [] },
          unverified_version: null,
          read_only_reason: null,
        },
      ],
      artifacts: [],
      updates: [],
      refreshed_at: 1789700000,
      stale: false,
      errors: [],
    };
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "get_snapshot") return Promise.resolve(ollamaSnapshot);
      if (cmd === "get_settings") return Promise.resolve(settings);
      if (cmd === "open_ollama_app")
        return Promise.reject('{"kind":"ollama_open_failed","reason":"not_installed"}');
      return Promise.resolve(undefined);
    });

    const { findByText, getByRole, queryByText } = renderWithProviders(
      <SnapshotStatus>
        <InstalledPage />
      </SnapshotStatus>,
    );

    await findByText("Ollama isn't running");
    fireEvent.click(getByRole("button", { name: "Open Ollama" }));

    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent(/find the Ollama app on this Mac/);
    expect(alert).toHaveTextContent(/separate download/);
    expect(queryByText(/ollama_open_failed/)).not.toBeInTheDocument();
  });
  it("keeps a stopped source's rows on screen but offers no Uninstall on them", async () => {
    // `refresh` carries an unavailable source's last known artifacts
    // forward, which is what makes the notice's "below is what Canager saw
    // last time" true instead of a sentence above an empty group. Every
    // one of those rows would otherwise carry an Uninstall button, and
    // `ollama rm` against a daemon that is not listening cannot succeed --
    // spec §2.5's conjunction, on the button rather than only in the
    // backend's refusal.
    const stoppedOllamaSnapshot: Snapshot = {
      generation: 4,
      detect: "Found",
      instances: [
        {
          id: "ollama:http://127.0.0.1:11434",
          adapter_id: "ollama",
          exe_path: "/usr/local/bin/ollama",
          prefix: "/usr/local",
          scope: "User",
          version: "0.34.1",
          status: { unavailable: "NotRunning", notes: [] },
          unverified_version: null,
          read_only_reason: null,
        },
      ],
      artifacts: [
        {
          key: { instance_id: "ollama:http://127.0.0.1:11434", kind: "Model", name: "qwen3:8b" },
          display_name: "qwen3:8b",
          version: "5642e97495e1",
          reason: "Requested",
          description: null,
          homepage: null,
          size_bytes: null,
          installed_at: null,
          path: null,
          auto_updates: false,
          uninstall_blocked: null,
        },
      ],
      updates: [],
      refreshed_at: 1789700000,
      stale: true,
      errors: [],
    };
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "get_snapshot") return Promise.resolve(stoppedOllamaSnapshot);
      if (cmd === "get_settings") return Promise.resolve(settings);
      return Promise.resolve(undefined);
    });

    const { findByText, queryByRole } = renderWithProviders(
      <SnapshotStatus>
        <InstalledPage />
      </SnapshotStatus>,
    );

    await findByText("Ollama isn't running");
    expect(await findByText("qwen3:8b")).toBeInTheDocument();
    expect(queryByRole("button", { name: "Uninstall" })).toBeNull();
  });
  it("expands one source's dependencies without expanding another's", async () => {
    // One global flag meant clicking pip's "N components installed by other
    // software" also unfolded Homebrew's, on any Mac that has both. The
    // toggle is per source now, and it can be folded back up again.
    const twoSources: Snapshot = {
      ...snapshot,
      instances: [...snapshot.instances, ...pipSnapshot.instances],
      artifacts: [
        ...snapshot.artifacts,
        ...pipSnapshot.artifacts,
        {
          key: { instance_id: "pip:/usr/bin/python3", kind: "Package", name: "charset-normalizer" },
          display_name: "charset-normalizer",
          version: "3.4.0",
          reason: "Dependency",
          description: "The Real First Universal Charset Detector.",
          homepage: null,
          size_bytes: null,
          installed_at: null,
          path: null,
          auto_updates: false,
          uninstall_blocked: null,
        },
      ],
    };
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "get_snapshot") return Promise.resolve(twoSources);
      if (cmd === "get_settings") return Promise.resolve(settings);
      return Promise.resolve(undefined);
    });

    const { findByText, findByRole, queryByText } = renderWithProviders(<InstalledPage />);

    await findByText("requests");
    expect(queryByText("glib")).not.toBeInTheDocument();
    expect(queryByText("charset-normalizer")).not.toBeInTheDocument();

    // Both groups hide exactly one dependency, so the two toggles carry the
    // same label; the one inside pip's group is the one to click.
    const toggles = await screen.findAllByRole("button", {
      name: "1 component installed by other software",
    });
    expect(toggles).toHaveLength(2);
    fireEvent.click(toggles[1]);

    await findByText("charset-normalizer");
    expect(queryByText("glib")).not.toBeInTheDocument();

    // And it folds back up, which a toggle that vanishes on expand cannot.
    const collapse = await findByRole("button", {
      name: "Hide the 1 component installed by other software",
    });
    fireEvent.click(collapse);
    await waitFor(() => expect(queryByText("charset-normalizer")).not.toBeInTheDocument());
  });
});
