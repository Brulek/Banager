import { describe, expect, it } from "vitest";
import { canWrite, hasSourceNotice, isAvailable, sourceNoticesFor } from "./sources";
import type { ManagerInstance } from "./types";

function instance(over: Partial<ManagerInstance> = {}): ManagerInstance {
  return {
    id: "brew:/opt/homebrew",
    adapter_id: "brew",
    exe_path: "/opt/homebrew/bin/brew",
    prefix: "/opt/homebrew",
    scope: "User",
    version: "7.0.3",
    unverified_version: null,
    read_only_reason: null,
    status: { unavailable: null, notes: [] },
    ...over,
  };
}

describe("sourceNoticesFor", () => {
  it("says nothing about a source that answered and can be changed", () => {
    expect(sourceNoticesFor(instance(), "Homebrew")).toEqual([]);
    expect(hasSourceNotice(instance())).toBe(false);
    expect(canWrite(instance())).toBe(true);
    expect(isAvailable(instance())).toBe(true);
  });

  it("gives each read-only reason its own copy", () => {
    const [pip] = sourceNoticesFor(
      instance({ adapter_id: "pip", read_only_reason: "ByDesign" }),
      "pip",
    );
    expect(pip.axis).toBe("capability");
    expect(pip.variant).toBe("info");
    expect(pip.titleKey).toBe("sourceNotice.pipReadOnly.title");

    const [npm] = sourceNoticesFor(
      instance({ adapter_id: "npm", read_only_reason: "PrefixNotWritable" }),
      "npm",
    );
    // Not pip's advice. Telling someone whose npm prefix is root-owned to
    // install a Python tool is worse than saying nothing.
    expect(npm.titleKey).toBe("sourceNotice.prefixNotWritable.title");
    expect(npm.descriptionKey).toBe("sourceNotice.prefixNotWritable.description");
  });

  it("offers to start Ollama, and only Ollama, when a source is not running", () => {
    const [ollama] = sourceNoticesFor(
      instance({ adapter_id: "ollama", status: { unavailable: "NotRunning", notes: [] } }),
      "Ollama",
    );
    expect(ollama.axis).toBe("state");
    expect(ollama.variant).toBe("warning");
    expect(ollama.titleKey).toBe("sourceNotice.ollamaNotRunning.title");
    expect(ollama.action?.id).toBe("openOllama");

    // Any other source that reports NotRunning gets the generic copy, named
    // in the user's language, and no button: Canager has no way to start it.
    const [other] = sourceNoticesFor(
      instance({ adapter_id: "brew", status: { unavailable: "NotRunning", notes: [] } }),
      "Homebrew",
    );
    expect(other.titleKey).toBe("sourceNotice.notRunning.title");
    expect(other.values).toEqual({ source: "Homebrew" });
    expect(other.action).toBeUndefined();
  });

  it("says a source that would not answer is showing last time's data", () => {
    const [notice] = sourceNoticesFor(
      instance({ status: { unavailable: "NotResponding", notes: [] } }),
      "Homebrew",
    );
    expect(notice.axis).toBe("state");
    expect(notice.titleKey).toBe("sourceNotice.unreachable.title");
    expect(notice.values).toEqual({ source: "Homebrew" });
  });

  it("warns that a stale index makes up-to-date unreliable, with a way to retry", () => {
    const [note] = sourceNoticesFor(
      instance({ status: { unavailable: null, notes: ["IndexMayBeStale"] } }),
      "Homebrew",
    );
    expect(note.axis).toBe("state");
    expect(note.titleKey).toBe("sourceNotice.indexMayBeStale.title");
    expect(note.action?.id).toBe("retry");
  });

  it("carries both axes at once, capability first", () => {
    // Independent axes: a source can be read-only *and* silent, and each
    // half is something different for the user to do.
    const notices = sourceNoticesFor(
      instance({
        adapter_id: "pip",
        read_only_reason: "ByDesign",
        status: { unavailable: "NotResponding", notes: ["IndexMayBeStale"] },
      }),
      "pip",
    );
    expect(notices.map((n) => n.axis)).toEqual(["capability", "state", "state"]);
    expect(new Set(notices.map((n) => n.id)).size).toBe(3);
  });

  it("is the one rule hasSourceNotice answers from", () => {
    // SnapshotStatus's "Nothing installed yet" gate and the pages' group
    // headers must never disagree about which sources have something to
    // say: a Mac whose only source is a stopped Ollama would otherwise see
    // the empty state and no way to start it.
    for (const inst of [
      instance(),
      instance({ read_only_reason: "ByDesign" }),
      instance({ status: { unavailable: "NotRunning", notes: [] } }),
      instance({ status: { unavailable: null, notes: ["IndexMayBeStale"] } }),
    ]) {
      expect(hasSourceNotice(inst)).toBe(sourceNoticesFor(inst, "Homebrew").length > 0);
    }
  });
});
