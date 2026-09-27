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

  it("invertUpdateSelection flips each key it is given and leaves every other id alone", () => {
    // wget is selected and not handed over: the Updates page passes only
    // the rows that show a checkbox, and a row without one keeps whatever
    // selection it had.
    const glib: ArtifactKey = { ...key, name: "glib" };
    const wget: ArtifactKey = { ...key, name: "wget" };
    useUiStore.getState().toggleUpdate(wget);
    useUiStore.getState().toggleUpdate(key);

    useUiStore.getState().invertUpdateSelection([key, glib]);
    expect(useUiStore.getState().selectedUpdates).toEqual([
      artifactKeyId(wget),
      artifactKeyId(glib),
    ]);

    useUiStore.getState().invertUpdateSelection([key, glib]);
    expect(useUiStore.getState().selectedUpdates).toEqual([
      artifactKeyId(wget),
      artifactKeyId(key),
    ]);
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
