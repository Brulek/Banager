import { describe, expect, it, beforeEach } from "vitest";
import { useUiStore, artifactKeyId } from "./ui";
import type { ArtifactKey } from "../lib/types";

const key: ArtifactKey = { instance_id: "brew:/opt/homebrew", kind: "Formula", name: "jq" };

beforeEach(() => {
  useUiStore.setState({
    page: "installed",
    query: "",
    showDependencies: false,
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

  it("setPage changes the active page", () => {
    useUiStore.getState().setPage("updates");
    expect(useUiStore.getState().page).toBe("updates");
  });

  it("setQuery changes the filter text", () => {
    useUiStore.getState().setQuery("jq");
    expect(useUiStore.getState().query).toBe("jq");
  });

  it("toggleDependencies flips the flag", () => {
    expect(useUiStore.getState().showDependencies).toBe(false);
    useUiStore.getState().toggleDependencies();
    expect(useUiStore.getState().showDependencies).toBe(true);
    useUiStore.getState().toggleDependencies();
    expect(useUiStore.getState().showDependencies).toBe(false);
  });

  it("setDrawerOpen and setFocusedOpId update independently", () => {
    useUiStore.getState().setDrawerOpen(true);
    useUiStore.getState().setFocusedOpId(5);
    expect(useUiStore.getState().drawerOpen).toBe(true);
    expect(useUiStore.getState().focusedOpId).toBe(5);
  });

  it("appendLog appends in order and clearLogs removes only that op's lines", () => {
    useUiStore.getState().appendLog({ opId: 1, stream: "Stdout", line: "a" });
    useUiStore.getState().appendLog({ opId: 2, stream: "Stdout", line: "b" });
    useUiStore.getState().appendLog({ opId: 1, stream: "Stdout", line: "c" });

    expect(useUiStore.getState().logs.map((l) => l.line)).toEqual(["a", "b", "c"]);

    useUiStore.getState().clearLogs(1);
    expect(useUiStore.getState().logs.map((l) => l.line)).toEqual(["b"]);
  });

  it("appendLog keeps only the newest 2000 lines", () => {
    for (let i = 0; i < 2001; i += 1) {
      useUiStore.getState().appendLog({ opId: 1, stream: "Stdout", line: `line-${i}` });
    }
    const logs = useUiStore.getState().logs;
    expect(logs).toHaveLength(2000);
    expect(logs[0].line).toBe("line-1");
    expect(logs[1999].line).toBe("line-2000");
  });

  it("toggleUpdate adds then removes the key's id from selectedUpdates", () => {
    useUiStore.getState().toggleUpdate(key);
    expect(useUiStore.getState().selectedUpdates).toEqual([artifactKeyId(key)]);
    useUiStore.getState().toggleUpdate(key);
    expect(useUiStore.getState().selectedUpdates).toEqual([]);
  });

  it("clearSelectedUpdates empties the selection", () => {
    useUiStore.getState().toggleUpdate(key);
    useUiStore.getState().clearSelectedUpdates();
    expect(useUiStore.getState().selectedUpdates).toEqual([]);
  });
});
