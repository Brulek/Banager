import { create } from "zustand";
import type { ArtifactKey, LogNote, Stream } from "../lib/types";

export type Page = "overview" | "updates" | "installed" | "unknown" | "settings";

/** How the Installed page orders its list: by the tools' names, or by source first. */
export type InstalledSort = "name" | "source";

// One entry in an operation's log: either a line the tool wrote, shown
// verbatim, or a note of Canager's own, which the drawer localises.
export type LogEntry =
  | { opId: number; stream: Stream; line: string }
  | { opId: number; note: LogNote };
export type LogLine = LogEntry & { seq: number };

export interface UiState {
  page: Page;
  setPage(p: Page): void;
  // The Installed page's search text.
  query: string;
  setQuery(q: string): void;
  // Which source the Installed page shows: an instance id, or null for
  // all of them (「全部」). Kept here, not in the page, so whatever opens
  // the page can open it on one source (`openInstalled`).
  installedFilter: string | null;
  setInstalledFilter(instanceId: string | null): void;
  installedSort: InstalledSort;
  setInstalledSort(sort: InstalledSort): void;
  // Opens the Installed page showing one source's tools -- an Overview
  // tile's -- or, with null, all of them: the sidebar's entry, whose count
  // is of everything installed.
  openInstalled(instanceId: string | null): void;
  // The ids of the sources whose dependencies are unfolded. This used to
  // be one boolean for the whole page, so unfolding pip's "N components
  // installed by other software" unfolded Homebrew's and npm's too --
  // routine on any Mac with more than one source. Every row already knows
  // which instance it came from; the expansion follows it.
  expandedDependencies: string[];
  toggleDependencies(instanceId: string): void;
  drawerOpen: boolean;
  setDrawerOpen(open: boolean): void;
  focusedOpId: number | null;
  setFocusedOpId(id: number | null): void;
  logs: LogLine[];
  appendLog(l: LogEntry): void;
  selectedUpdates: string[];
  toggleUpdate(key: ArtifactKey): void;
  // What the Updates page's Select all and Invert selection call, with the
  // keys of the rows that show a checkbox. Each changes the ids of the keys
  // it is given and no others: an id already in `selectedUpdates` for any
  // other key -- a row selected before a refresh took its checkbox away --
  // stays exactly as it was.
  //
  // Adds the id of every key given that is not selected yet.
  selectUpdates(keys: ArtifactKey[]): void;
  // Removes the id of every key given that is selected, and adds the id of
  // every one that is not.
  invertUpdateSelection(keys: ArtifactKey[]): void;
  // The version each update Canager started was for, by operation id:
  // the `target` of the candidate it was started from (`useUpdateConfirm`,
  // on the Updates page or in the Installed page's detail). An operation
  // carries no version (`OpSummary`), and a finished one stays in the
  // backend's list, so this is how a row tells an outcome that is still
  // about the version it offers -- "Updated", "Failed" -- from one about a
  // version it no longer offers, whose row gets its Update button back.
  updateTargets: Record<number, string>;
  rememberUpdateTarget(opId: number, target: string): void;
  // The name the lists showed for what each operation acts on, by
  // operation id (`useOperationName`): an operation carries only its key's
  // name -- `claude`, `visual-studio-code` -- and an uninstalled row, which
  // had the name the user knows, is gone from the snapshot once the
  // uninstall has finished. Kept so the operation bar and the log drawer go
  // on calling it what they called it while it ran.
  opNames: Record<number, string>;
  rememberOpNames(names: Record<number, string>): void;
  // When each operation finished, by operation id, in milliseconds: when
  // this window heard its `Finished` event (`useOperationEvents`). An
  // operation carries no time of its own (`OpSummary`); the Updates page's
  // "Just updated" says when each update finished, and says no time for
  // one that finished before this window was opened.
  opFinishedAt: Record<number, number>;
  rememberOpFinished(opId: number, at: number): void;
  // The updates "Clear" took off the Updates page's "Just updated", by
  // operation id: the section is hidden until an update not among them
  // succeeds.
  clearedJustUpdated: number[];
  clearJustUpdated(opIds: number[]): void;
  startupRefreshError: string | null;
  setStartupRefreshError(message: string | null): void;
}

const MAX_LOG_LINES = 2000;

export function artifactKeyId(key: ArtifactKey): string {
  return `${key.instance_id}|${key.kind}|${key.name}`;
}

// Ever-increasing across the page's lifetime (React keys need stable
// ordering even after the ring buffer below has evicted older lines); tests
// never assert its absolute value, only that appended lines keep the order
// they were appended in.
let logSeq = 0;

export const useUiStore = create<UiState>((set) => ({
  // The Overview: what the Mac looks like at a glance, before any list.
  page: "overview",
  setPage: (p) => set({ page: p }),
  query: "",
  setQuery: (q) => set({ query: q }),
  installedFilter: null,
  setInstalledFilter: (instanceId) => set({ installedFilter: instanceId }),
  installedSort: "name",
  setInstalledSort: (sort) => set({ installedSort: sort }),
  openInstalled: (instanceId) => set({ page: "installed", installedFilter: instanceId }),
  expandedDependencies: [],
  toggleDependencies: (instanceId) =>
    set((s) => ({
      expandedDependencies: s.expandedDependencies.includes(instanceId)
        ? s.expandedDependencies.filter((id) => id !== instanceId)
        : [...s.expandedDependencies, instanceId],
    })),
  drawerOpen: false,
  setDrawerOpen: (open) => set({ drawerOpen: open }),
  focusedOpId: null,
  setFocusedOpId: (id) => set({ focusedOpId: id }),
  logs: [],
  appendLog: (l) =>
    set((s) => {
      const next = [...s.logs, { ...l, seq: logSeq++ }];
      return {
        logs: next.length > MAX_LOG_LINES ? next.slice(next.length - MAX_LOG_LINES) : next,
      };
    }),
  selectedUpdates: [],
  toggleUpdate: (key) =>
    set((s) => {
      const id = artifactKeyId(key);
      return {
        selectedUpdates: s.selectedUpdates.includes(id)
          ? s.selectedUpdates.filter((x) => x !== id)
          : [...s.selectedUpdates, id],
      };
    }),
  selectUpdates: (keys) =>
    set((s) => ({
      // A `Set` keeps each id once, in the order it was first added: the
      // ids already selected, then the new ones in the order given.
      selectedUpdates: [...new Set([...s.selectedUpdates, ...keys.map(artifactKeyId)])],
    })),
  invertUpdateSelection: (keys) =>
    set((s) => {
      const given = new Set(keys.map(artifactKeyId));
      const selected = new Set(s.selectedUpdates);
      return {
        selectedUpdates: [
          ...s.selectedUpdates.filter((id) => !given.has(id)),
          ...[...given].filter((id) => !selected.has(id)),
        ],
      };
    }),
  updateTargets: {},
  rememberUpdateTarget: (opId, target) =>
    set((s) => ({ updateTargets: { ...s.updateTargets, [opId]: target } })),
  opNames: {},
  rememberOpNames: (names) => set((s) => ({ opNames: { ...s.opNames, ...names } })),
  opFinishedAt: {},
  // The first time heard stands: an operation finishes once.
  rememberOpFinished: (opId, at) =>
    set((s) => (opId in s.opFinishedAt ? s : { opFinishedAt: { ...s.opFinishedAt, [opId]: at } })),
  clearedJustUpdated: [],
  clearJustUpdated: (opIds) =>
    set((s) => ({ clearedJustUpdated: [...new Set([...s.clearedJustUpdated, ...opIds])] })),
  startupRefreshError: null,
  setStartupRefreshError: (message) => set({ startupRefreshError: message }),
}));
