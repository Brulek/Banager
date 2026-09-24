import { create } from "zustand";
import type { ArtifactKey, LogNote, Stream } from "../lib/types";

export type Page = "installed" | "updates" | "unknown" | "settings";

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
  page: "installed",
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
  startupRefreshError: null,
  setStartupRefreshError: (message) => set({ startupRefreshError: message }),
}));
