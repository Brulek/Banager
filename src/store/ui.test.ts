import { describe, expect, it, beforeEach } from "vitest";
import { useUiStore, artifactKeyId, type LogLine } from "./ui";
import type { ArtifactKey } from "../lib/types";

const lineText = (l: LogLine) => ("line" in l ? l.line : null);

const key: ArtifactKey = { instance_id: "brew:/opt/homebrew", kind: "Formula", name: "jq" };

beforeEach(() => {
  useUiStore.setState({
    page: "installed",
    query: "",
    expandedDependencies: [],
    drawerOpen: false,
    focusedOpId: null,
    logs: [],
    selectedUpdates: [],
  });
});

describe("useUiStore", () => {
  it("artifactKeyId joins the key's fields with |", () => {
    expect(artifactKeyId(key)).toBe("brew:/opt/homebrew|Formula|jq");
  });

  it("starts on the Overview", () => {
    expect(useUiStore.getInitialState().page).toBe("overview");
  });

  it("setPage changes the active page", () => {
    useUiStore.getState().setPage("updates");
    expect(useUiStore.getState().page).toBe("updates");
    useUiStore.getState().setPage("unknown");
    expect(useUiStore.getState().page).toBe("unknown");
  });

  it("setQuery changes the search text", () => {
    useUiStore.getState().setQuery("jq");
    expect(useUiStore.getState().query).toBe("jq");
  });

  it("opens the Installed page on one source, or on all of them, and keeps its sort", () => {
    // Every source, sorted by name, until something asks otherwise.
    expect(useUiStore.getInitialState().installedFilter).toBeNull();
    expect(useUiStore.getInitialState().installedSort).toBe("name");

    useUiStore.getState().setPage("overview");
    useUiStore.getState().openInstalled("brew:/opt/homebrew");
    expect(useUiStore.getState().page).toBe("installed");
    expect(useUiStore.getState().installedFilter).toBe("brew:/opt/homebrew");

    useUiStore.getState().setInstalledSort("source");
    useUiStore.getState().openInstalled(null);
    expect(useUiStore.getState().installedFilter).toBeNull();
    expect(useUiStore.getState().installedSort).toBe("source");
  });

  it("opens the Installed page with the search cleared, from a tile and from the sidebar alike", () => {
    // An Overview tile promises one source's tools; an old search left in
    // the box would show fewer than the tile's count.
    useUiStore.getState().setQuery("jq");
    useUiStore.getState().setPage("overview");
    useUiStore.getState().openInstalled("brew:/opt/homebrew");
    expect(useUiStore.getState().query).toBe("");

    useUiStore.getState().setQuery("node");
    useUiStore.getState().openInstalled(null);
    expect(useUiStore.getState().query).toBe("");
  });

  it("opens a page as its row in the sidebar does: Installed on everything, its search cleared, and any other page as it is", () => {
    // A source's tools, searched, then the sidebar's Installed or ⌘3.
    useUiStore.setState({ page: "updates", installedFilter: "npm:/opt/homebrew", query: "ts", installedSort: "source" });

    useUiStore.getState().openPage("installed");
    expect(useUiStore.getState()).toMatchObject({
      page: "installed",
      installedFilter: null,
      query: "",
      installedSort: "source",
    });

    // Another page leaves the Installed page's source and search as they
    // are, as setPage does, and drops what a page was asked to show.
    useUiStore.setState({ installedFilter: "npm:/opt/homebrew", query: "ts" });
    for (const page of ["overview", "updates", "unknown", "settings"] as const) {
      useUiStore.setState({ searchFocusRequested: true, hiddenUpdatesRequested: true, inspectRequested: "x" });
      useUiStore.getState().openPage(page);
      expect(useUiStore.getState()).toMatchObject({
        page,
        installedFilter: "npm:/opt/homebrew",
        query: "ts",
        searchFocusRequested: false,
        hiddenUpdatesRequested: false,
        inspectRequested: null,
      });
    }
  });

  it("asks for the Installed page's search box from another page, opening the page as the sidebar does", () => {
    useUiStore.setState({ page: "updates", installedFilter: "npm:/opt/homebrew", query: "ts" });

    useUiStore.getState().searchInstalled();

    const state = useUiStore.getState();
    expect(state.page).toBe("installed");
    expect(state.installedFilter).toBeNull();
    expect(state.query).toBe("");
    expect(state.searchFocusRequested).toBe(true);
  });

  it("asks for the search box on the Installed page itself, keeping its source and its search", () => {
    useUiStore.setState({ page: "installed", installedFilter: "npm:/opt/homebrew", query: "ts" });

    useUiStore.getState().searchInstalled();

    const state = useUiStore.getState();
    expect(state.installedFilter).toBe("npm:/opt/homebrew");
    expect(state.query).toBe("ts");
    expect(state.searchFocusRequested).toBe(true);
  });

  it("drops a search not yet focused once another page is chosen, or once it is focused", () => {
    // Asked for while the page loads, then left: it must not take the focus
    // when the user comes back later by any other way.
    useUiStore.getState().searchInstalled();
    useUiStore.getState().setPage("settings");
    expect(useUiStore.getState().searchFocusRequested).toBe(false);

    useUiStore.getState().searchInstalled();
    useUiStore.getState().openInstalled("brew:/opt/homebrew");
    expect(useUiStore.getState().searchFocusRequested).toBe(false);

    useUiStore.getState().searchInstalled();
    useUiStore.getState().searchFocused();
    expect(useUiStore.getState().searchFocusRequested).toBe(false);
    expect(useUiStore.getState().page).toBe("installed");
  });

  it("opens Settings at its hidden updates, and drops that once they are shown or another page is chosen", () => {
    // The Overview's count of hidden updates.
    useUiStore.getState().showHiddenUpdates();
    expect(useUiStore.getState().page).toBe("settings");
    expect(useUiStore.getState().hiddenUpdatesRequested).toBe(true);
    useUiStore.getState().hiddenUpdatesShown();
    expect(useUiStore.getState().hiddenUpdatesRequested).toBe(false);
    expect(useUiStore.getState().page).toBe("settings");

    // Left before Settings could show them: coming back to Settings by
    // any other way must not move it.
    useUiStore.getState().showHiddenUpdates();
    useUiStore.getState().setPage("updates");
    expect(useUiStore.getState().hiddenUpdatesRequested).toBe(false);

    useUiStore.getState().showHiddenUpdates();
    useUiStore.getState().openInstalled(null);
    expect(useUiStore.getState().hiddenUpdatesRequested).toBe(false);

    useUiStore.getState().showHiddenUpdates();
    useUiStore.getState().searchInstalled();
    expect(useUiStore.getState().hiddenUpdatesRequested).toBe(false);

    // Nor does a search not yet focused outlive it.
    useUiStore.getState().searchInstalled();
    useUiStore.getState().showHiddenUpdates();
    expect(useUiStore.getState().searchFocusRequested).toBe(false);
  });

  it("opens the Installed page to select a tool, keeping only that tool's own source, with the search cleared", () => {
    const grok: ArtifactKey = { instance_id: "standalone-grok", kind: "Binary", name: "grok" };
    // From another page, on another source's list, mid-search: every
    // source, no search, so the tool is in the list to select.
    useUiStore.setState({ page: "overview", installedFilter: "brew:/opt/homebrew", query: "jq" });
    useUiStore.getState().showInstalledTool(grok);
    expect(useUiStore.getState()).toMatchObject({
      page: "installed",
      installedFilter: null,
      query: "",
      inspectRequested: artifactKeyId(grok),
    });
    useUiStore.getState().inspectAnswered();
    expect(useUiStore.getState().inspectRequested).toBeNull();

    // On the tool's own source already: it stays there.
    useUiStore.setState({ installedFilter: grok.instance_id, query: "gr" });
    useUiStore.getState().showInstalledTool(grok);
    expect(useUiStore.getState().installedFilter).toBe(grok.instance_id);
    expect(useUiStore.getState().query).toBe("");
  });

  it("drops a tool not yet selected once the window goes anywhere by any other way", () => {
    const leave = [
      () => useUiStore.getState().setPage("settings"),
      () => useUiStore.getState().openInstalled(null),
      () => useUiStore.getState().searchInstalled(),
      () => useUiStore.getState().showHiddenUpdates(),
    ];
    for (const away of leave) {
      useUiStore.getState().showInstalledTool(key);
      away();
      expect(useUiStore.getState().inspectRequested).toBeNull();
    }
  });

  it("toggleDependencies expands one source at a time", () => {
    // It used to be a single boolean, so unfolding pip's dependencies also
    // unfolded Homebrew's. Every row already carries the instance it came
    // from; the expansion follows it.
    const brew = "brew:/opt/homebrew";
    const pip = "pip:/usr/bin/python3";

    expect(useUiStore.getState().expandedDependencies).toEqual([]);
    useUiStore.getState().toggleDependencies(pip);
    expect(useUiStore.getState().expandedDependencies).toEqual([pip]);

    useUiStore.getState().toggleDependencies(brew);
    expect(useUiStore.getState().expandedDependencies).toEqual([pip, brew]);

    useUiStore.getState().toggleDependencies(pip);
    expect(useUiStore.getState().expandedDependencies).toEqual([brew]);
  });

  it("setDrawerOpen and setFocusedOpId update independently", () => {
    useUiStore.getState().setDrawerOpen(true);
    useUiStore.getState().setFocusedOpId(5);
    expect(useUiStore.getState().drawerOpen).toBe(true);
    expect(useUiStore.getState().focusedOpId).toBe(5);
  });

  it("appendLog appends in order across ops", () => {
    useUiStore.getState().appendLog({ opId: 1, stream: "Stdout", line: "a" });
    useUiStore.getState().appendLog({ opId: 2, stream: "Stdout", line: "b" });
    useUiStore.getState().appendLog({ opId: 1, stream: "Stdout", line: "c" });

    expect(useUiStore.getState().logs.map(lineText)).toEqual(["a", "b", "c"]);
  });

  it("appendLog keeps only the newest 2000 lines", () => {
    for (let i = 0; i < 2001; i += 1) {
      useUiStore.getState().appendLog({ opId: 1, stream: "Stdout", line: `line-${i}` });
    }
    const logs = useUiStore.getState().logs;
    expect(logs).toHaveLength(2000);
    expect(lineText(logs[0])).toBe("line-1");
    expect(lineText(logs[1999])).toBe("line-2000");
  });

  it("toggleUpdate adds then removes the key's id from selectedUpdates", () => {
    useUiStore.getState().toggleUpdate(key);
    expect(useUiStore.getState().selectedUpdates).toEqual([artifactKeyId(key)]);
    useUiStore.getState().toggleUpdate(key);
    expect(useUiStore.getState().selectedUpdates).toEqual([]);
  });

  it("selectUpdates adds each key's id once and keeps every id already selected", () => {
    const glib: ArtifactKey = { ...key, name: "glib" };
    const wget: ArtifactKey = { ...key, name: "wget" };
    useUiStore.getState().toggleUpdate(wget);
    useUiStore.getState().toggleUpdate(key);

    useUiStore.getState().selectUpdates([glib, key, glib]);
    expect(useUiStore.getState().selectedUpdates).toEqual([
      artifactKeyId(wget),
      artifactKeyId(key),
      artifactKeyId(glib),
    ]);

    // Nothing left to add: the selection is unchanged.
    useUiStore.getState().selectUpdates([glib, key]);
    expect(useUiStore.getState().selectedUpdates).toEqual([
      artifactKeyId(wget),
      artifactKeyId(key),
      artifactKeyId(glib),
    ]);
  });

  it("deselectUpdates unticks each key it is given and leaves every other id alone", () => {
    // wget is selected and not handed over: the Updates page passes only
    // the rows that show a checkbox, and a row without one keeps whatever
    // selection it had.
    const glib: ArtifactKey = { ...key, name: "glib" };
    const wget: ArtifactKey = { ...key, name: "wget" };
    useUiStore.getState().toggleUpdate(wget);
    useUiStore.getState().toggleUpdate(key);

    // glib was never ticked: nothing to take away for it.
    useUiStore.getState().deselectUpdates([key, glib]);
    expect(useUiStore.getState().selectedUpdates).toEqual([artifactKeyId(wget)]);

    useUiStore.getState().deselectUpdates([key, glib]);
    expect(useUiStore.getState().selectedUpdates).toEqual([artifactKeyId(wget)]);
  });

  it("remembers when an operation finished the first time it hears so", () => {
    expect(useUiStore.getInitialState().opFinishedAt).toEqual({});
    useUiStore.getState().rememberOpFinished(3, 1000);
    useUiStore.getState().rememberOpFinished(4, 2000);
    useUiStore.getState().rememberOpFinished(3, 5000);
    expect(useUiStore.getState().opFinishedAt).toEqual({ 3: 1000, 4: 2000 });
  });

  it("keeps what Clear took off Just updated, each once", () => {
    expect(useUiStore.getInitialState().clearedJustUpdated).toEqual([]);
    useUiStore.getState().clearJustUpdated([7, 8]);
    useUiStore.getState().clearJustUpdated([8, 9]);
    expect(useUiStore.getState().clearedJustUpdated).toEqual([7, 8, 9]);
  });
});

describe("the Installed page's ticks for a batch uninstall", () => {
  const glib: ArtifactKey = { ...key, name: "glib" };
  const wget: ArtifactKey = { ...key, name: "wget" };

  it("start empty, and toggle a key's id in and out", () => {
    expect(useUiStore.getInitialState().selectedUninstalls).toEqual([]);
    useUiStore.getState().toggleUninstall(key);
    expect(useUiStore.getState().selectedUninstalls).toEqual([artifactKeyId(key)]);
    useUiStore.getState().toggleUninstall(key);
    expect(useUiStore.getState().selectedUninstalls).toEqual([]);
  });

  it("are ticked each once and unticked by key, leaving every other id alone", () => {
    useUiStore.getState().toggleUninstall(wget);
    useUiStore.getState().selectUninstalls([glib, key, glib]);
    expect(useUiStore.getState().selectedUninstalls).toEqual([
      artifactKeyId(wget),
      artifactKeyId(glib),
      artifactKeyId(key),
    ]);
    useUiStore.getState().deselectUninstalls([key, { ...key, name: "never-ticked" }]);
    expect(useUiStore.getState().selectedUninstalls).toEqual([artifactKeyId(wget), artifactKeyId(glib)]);
    // Not the Updates page's, which are another list's.
    expect(useUiStore.getState().selectedUpdates).toEqual([]);
  });

  it("drop for good the ticks of tools no longer listed, and change nothing when all still are", () => {
    useUiStore.getState().selectUninstalls([key, glib, wget]);
    const before = useUiStore.getState().selectedUninstalls;
    useUiStore.getState().keepUninstalls(new Set([artifactKeyId(key), artifactKeyId(glib), artifactKeyId(wget)]));
    expect(useUiStore.getState().selectedUninstalls).toBe(before);
    useUiStore.getState().keepUninstalls(new Set([artifactKeyId(wget)]));
    expect(useUiStore.getState().selectedUninstalls).toEqual([artifactKeyId(wget)]);
  });

  it("keep the last batch's record until the next one replaces it or it is dismissed", () => {
    expect(useUiStore.getInitialState().uninstallBatch).toBeNull();
    const first = { id: 1, items: [{ key, name: "jq", opId: 4, after: [] }] };
    const second = { id: 2, items: [{ key: glib, name: "glib", opId: null, after: [artifactKeyId(key)] }] };
    useUiStore.getState().setUninstallBatch(first);
    expect(useUiStore.getState().uninstallBatch).toEqual(first);
    useUiStore.getState().setUninstallBatch(second);
    expect(useUiStore.getState().uninstallBatch).toEqual(second);
    useUiStore.getState().dismissUninstallBatch();
    expect(useUiStore.getState().uninstallBatch).toBeNull();
  });
});
