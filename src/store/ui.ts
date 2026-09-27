import { create } from "zustand";
import type { ArtifactKey, LogNote, Stream } from "../lib/types";

export type Page = "overview" | "updates" | "installed" | "unknown" | "settings";

// One entry in an operation's log: either a line the tool wrote, shown
// verbatim, or a note of Canager's own, which the drawer localises.
export type LogEntry =
  | { opId: number; stream: Stream; line: string }
  | { opId: number; note: LogNote };
export type LogLine = LogEntry & { seq: number };

export interface UiState {
  page: Page;
  setPage(p: Page): void;
  query: string;
  setQuery(q: string): void;
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
  startupRefreshError: null,
  setStartupRefreshError: (message) => set({ startupRefreshError: message }),
}));
