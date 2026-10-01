import { create } from "zustand";
import type { InstalledShow, ToolShow } from "../lib/families";
import type { ArtifactKey, LogNote, Stream } from "../lib/types";

export type Page = "overview" | "updates" | "installed" | "unknown" | "settings";

/**
 * How the Installed page orders its list: by the tools' names, by source
 * first, or by how much each takes on disk, the largest first.
 */
export type InstalledSort = "name" | "source" | "size";

// One entry in an operation's log: either a line the tool wrote, shown
// verbatim, or a note of Banager's own, which the drawer localises.
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
  // What the Installed and the Updates page's 「显示」 popups show: every
  // tool, or only the AI coding tools (`ToolShow`, src/lib/families.ts);
  // on Installed, also the tools installed more than once (`InstalledShow`).
  // One for each page, kept while the page stays open. Every way onto a
  // page from outside it -- the sidebar's row, the menu bar's View menu,
  // ⌘F from another page, a notice's Show, the Overview's Review Updates,
  // the update notification -- sets it back to every tool, as each of
  // those promises a list (or a count) of everything.
  installedShow: InstalledShow;
  setInstalledShow(show: InstalledShow): void;
  updatesShow: ToolShow;
  setUpdatesShow(show: ToolShow): void;
  // Opens the Installed page showing one source's tools or, with null,
  // all of them: the sidebar's entry, whose count is of everything
  // installed. Either way the search starts empty and every tool is shown
  // (`installedShow`), so the list is the one the source or the count
  // promised, not an old search's or an old 「显示」's.
  openInstalled(instanceId: string | null): void;
  // A page's row in the sidebar, and its item in the menu bar's View menu
  // (⌘1 to ⌘4, src/lib/menu.ts): the page, and Installed on everything
  // installed (`openInstalled(null)`), which the row's count counts.
  openPage(p: Page): void;
  // The menu bar's Search (⌘F, src/lib/menu.ts): the Installed page, whose
  // search box takes the focus as soon as it is on screen -- at once, or
  // once the page has loaded -- and says so (`searchFocused`). From another
  // page, the page opens as the sidebar opens it, on everything with the
  // search empty and every tool shown; on it, it keeps its source, its
  // 「显示」 and its search, whose text is
  // then selected, to be typed over. Going to any page by any other way
  // drops a search not yet focused, so it cannot take the focus later.
  searchFocusRequested: boolean;
  searchInstalled(): void;
  searchFocused(): void;
  // A notice's Show (a launcher left without its program): the Installed
  // page with this tool selected, by artifact key id, its inspector open
  // and the focus on its row, as soon as the page has its snapshot --
  // at once, or once it has loaded -- which says so (`inspectAnswered`).
  // The page opens on every source, or stays on this tool's own, with the
  // search empty and every tool shown, so the tool is in the list; going to any page by any
  // other way drops it, as it drops a search not yet focused.
  inspectRequested: string | null;
  showInstalledTool(key: ArtifactKey): void;
  inspectAnswered(): void;
  // The Overview's 「2个已隐藏」: the Settings page, which brings its two
  // groups of hidden updates, 「已跳过的版本」 and 「不再提醒的工具」, into
  // view and puts the focus on the first one's title as soon as it is on
  // screen -- at once, or once its settings have loaded -- and
  // says so (`hiddenUpdatesShown`). Going to any page by any other way
  // drops it, as it drops a search not yet focused.
  hiddenUpdatesRequested: boolean;
  showHiddenUpdates(): void;
  hiddenUpdatesShown(): void;
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
  // What the Updates page's list header box calls, ticked and unticked,
  // and Update all, with the keys of the rows that show a checkbox. Each
  // changes the ids of the keys it is given and no others: an id already
  // in `selectedUpdates` for any other key -- a row selected before a
  // refresh took its checkbox away -- stays exactly as it was.
  //
  // Adds the id of every key given that is not selected yet.
  selectUpdates(keys: ArtifactKey[]): void;
  // Removes the id of every key given that is selected.
  deselectUpdates(keys: ArtifactKey[]): void;
  // The version each update Banager started was for, by operation id:
  // the `target` of the candidate it was started from (`useUpdateConfirm`,
  // on the Updates page or in the Installed page's detail). An operation
  // carries no version (`OpSummary`), and a finished one stays in the
  // backend's list, so this is how a row tells an outcome that is still
  // about the version it offers -- "Updated", "Update failed" -- from one about a
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
  // "Recently Updated" says when each update finished, and says no time for
  // one that finished before this window was opened.
  opFinishedAt: Record<number, number>;
  rememberOpFinished(opId: number, at: number): void;
  // The updates "Clear" took off the Updates page's "Recently Updated", by
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

export const useUiStore = create<UiState>((set, get) => ({
  // The Overview: what the Mac looks like at a glance, before any list.
  page: "overview",
  // The Updates page, by whatever way (the sidebar, the menu bar, the
  // Overview's Review Updates, the update notification), opens on every
  // update: each of them counted them all (`updatesShow`).
  setPage: (p) =>
    set({
      page: p,
      searchFocusRequested: false,
      hiddenUpdatesRequested: false,
      inspectRequested: null,
      ...(p === "updates" ? { updatesShow: "all" as const } : {}),
    }),
  query: "",
  setQuery: (q) => set({ query: q }),
  installedFilter: null,
  setInstalledFilter: (instanceId) => set({ installedFilter: instanceId }),
  installedSort: "name",
  setInstalledSort: (sort) => set({ installedSort: sort }),
  installedShow: "all",
  setInstalledShow: (show) => set({ installedShow: show }),
  updatesShow: "all",
  setUpdatesShow: (show) => set({ updatesShow: show }),
  openInstalled: (instanceId) =>
    set({
      page: "installed",
      installedFilter: instanceId,
      installedShow: "all",
      query: "",
      searchFocusRequested: false,
      hiddenUpdatesRequested: false,
      inspectRequested: null,
    }),
  openPage: (p) => (p === "installed" ? get().openInstalled(null) : get().setPage(p)),
  searchFocusRequested: false,
  searchInstalled: () =>
    set((s) =>
      s.page === "installed"
        ? { searchFocusRequested: true, inspectRequested: null }
        : {
            page: "installed",
            installedFilter: null,
            installedShow: "all",
            query: "",
            searchFocusRequested: true,
            hiddenUpdatesRequested: false,
            inspectRequested: null,
          },
    ),
  searchFocused: () => set({ searchFocusRequested: false }),
  inspectRequested: null,
  showInstalledTool: (key) =>
    set((s) => ({
      page: "installed",
      installedFilter: s.installedFilter === key.instance_id ? key.instance_id : null,
      installedShow: "all",
      query: "",
      searchFocusRequested: false,
      hiddenUpdatesRequested: false,
      inspectRequested: artifactKeyId(key),
    })),
  inspectAnswered: () => set({ inspectRequested: null }),
  hiddenUpdatesRequested: false,
  showHiddenUpdates: () =>
    set({ page: "settings", searchFocusRequested: false, hiddenUpdatesRequested: true, inspectRequested: null }),
  hiddenUpdatesShown: () => set({ hiddenUpdatesRequested: false }),
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
  deselectUpdates: (keys) =>
    set((s) => {
      const given = new Set(keys.map(artifactKeyId));
      return { selectedUpdates: s.selectedUpdates.filter((id) => !given.has(id)) };
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
