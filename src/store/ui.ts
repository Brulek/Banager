import { create } from "zustand";
import type { ArtifactKey } from "../lib/types";

export type Page = "installed" | "updates" | "settings";

export interface LogLine {
  opId: number;
  stream: "Stdout" | "Stderr";
  line: string;
  seq: number;
}

export interface UiState {
  page: Page;
  setPage(p: Page): void;
  query: string;
  setQuery(q: string): void;
  showDependencies: boolean;
  toggleDependencies(): void;
  drawerOpen: boolean;
  setDrawerOpen(open: boolean): void;
  focusedOpId: number | null;
  setFocusedOpId(id: number | null): void;
  logs: LogLine[];
  appendLog(l: Omit<LogLine, "seq">): void;
  clearLogs(opId: number): void;
  selectedUpdates: string[];
  toggleUpdate(key: ArtifactKey): void;
  clearSelectedUpdates(): void;
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
  showDependencies: false,
  toggleDependencies: () => set((s) => ({ showDependencies: !s.showDependencies })),
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
  clearLogs: (opId) => set((s) => ({ logs: s.logs.filter((l) => l.opId !== opId) })),
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
  clearSelectedUpdates: () => set({ selectedUpdates: [] }),
  startupRefreshError: null,
  setStartupRefreshError: (message) => set({ startupRefreshError: message }),
}));
