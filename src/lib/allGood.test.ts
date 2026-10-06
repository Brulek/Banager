import { describe, expect, it } from "vitest";
import i18n from "../i18n";
import { notCheckedHeadline, notCheckedNames } from "./allGood";
import { updatesSummary, type HidingSettings } from "./updateState";
import type { InstanceNote, ManagerInstance, SourceError, UpdateCandidate } from "./types";

/**
 * I22 (decisions round, 2026-10-06): the Overview's 「都好了」. With nothing
 * to install, nothing updating and nothing waiting for the password, the
 * summary is `upToDate` -- the green check -- whenever every source
 * answered this check and was checked in full this time, whatever else
 * the Updates page lists (hidden, can't be updated here, a copy Terminal
 * does not run): those are said in the line under it. A source whose
 * updates Banager never checks (Codex's own install) does not stop it; it
 * only stops the plain 「所有工具都是最新的」 (`everything`). Where a source
 * was not checked this time, the summary names it (`notChecked`).
 */

function instance(id: string, adapterId: string, over: Partial<ManagerInstance> = {}): ManagerInstance {
  return {
    id,
    adapter_id: adapterId,
    exe_path: `/opt/homebrew/bin/${adapterId}`,
    prefix: "/opt/homebrew",
    scope: "User",
    version: "1.0.0",
    answered_at: null,
    unverified_version: null,
    read_only_reason: null,
    status: { unavailable: null, notes: [] },
    ...over,
  };
}

const brew = instance("brew:/opt/homebrew", "brew");
const uv = instance("uv:/Users/you/.local/share/uv", "uv");
const pip = instance("pip:/usr/bin/python3", "pip", { read_only_reason: "ByDesign" });
const codex = instance("standalone-codex", "standalone-codex");
const ollama = instance("ollama:http://127.0.0.1:11434", "ollama");

const noted = (base: ManagerInstance, note: InstanceNote): ManagerInstance => ({
  ...base,
  status: { unavailable: null, notes: [note] },
});
const stopped = (base: ManagerInstance): ManagerInstance => ({
  ...base,
  status: { unavailable: "NotRunning", notes: [] },
});

function candidate(instanceId: string, name: string, over: Partial<UpdateCandidate> = {}): UpdateCandidate {
  return {
    key: { instance_id: instanceId, kind: "Formula", name },
    current: "1.0.0",
    target: "1.1.0",
    channel: "Native",
    checkable: true,
    warnings: [],
    blocked: null,
    ...over,
  };
}

const hiding = (over: Partial<HidingSettings> = {}): HidingSettings => ({
  ignored_updates: [],
  skipped_versions: [],
  snoozed_updates: [],
  ...over,
});

const summaryOf = (instances: ManagerInstance[], updates: UpdateCandidate[] = [], errors: SourceError[] = [], settings = hiding()) =>
  updatesSummary({ instances, updates, errors, artifacts: [] }, settings);

describe("the Overview's all good", () => {
  it("is the plain up to date with nothing listed and every source checked in full", () => {
    expect(summaryOf([brew, uv, pip])).toEqual({
      kind: "upToDate",
      everything: true,
      cantUpdateHere: 0,
      hidden: 0,
      notUsed: 0,
    });
  });

  it("is all good with what can't be updated here and what the user hid, said in its numbers", () => {
    const pinned = candidate(brew.id, "jq", { blocked: "Pinned" });
    const readOnly = candidate(pip.id, "urllib3");
    const hidden = candidate(brew.id, "glib");
    expect(summaryOf([brew, pip], [pinned, readOnly, hidden], [], hiding({ ignored_updates: [hidden.key] }))).toEqual({
      kind: "upToDate",
      everything: false,
      cantUpdateHere: 2,
      hidden: 1,
      notUsed: 0,
    });
  });

  it("is all good beside Codex's own install, whose updates Banager never checks, but not the plain up to date", () => {
    expect(summaryOf([brew, codex])).toEqual({
      kind: "upToDate",
      everything: false,
      cantUpdateHere: 0,
      hidden: 0,
      notUsed: 0,
    });
  });

  it("is the plain up to date only with nothing listed at all, as the Updates page says it", () => {
    // glib has just been updated and waits for the check that drops its
    // row: the Updates page still lists it, so not "Everything is up to date".
    const glib = candidate(brew.id, "glib");
    expect(updatesSummary({ instances: [brew], updates: [glib], errors: [], artifacts: [] }, hiding(), (u) => u === glib)).toEqual({
      kind: "upToDate",
      everything: false,
      cantUpdateHere: 0,
      hidden: 0,
      notUsed: 0,
    });
  });

  it("names the source that was not checked this time, and says whether any other was", () => {
    expect(summaryOf([brew, stopped(uv)])).toEqual({
      kind: "nothingToUpdate",
      notChecked: { ids: [uv.id], partly: false, rest: true },
      everythingElse: true,
      cantUpdateHere: 0,
      hidden: 0,
      notUsed: 0,
    });
    // Nothing else checked in full: no 「其余」 to speak of.
    expect(summaryOf([stopped(ollama)])).toEqual({
      kind: "nothingToUpdate",
      notChecked: { ids: [ollama.id], partly: false, rest: false },
      everythingElse: true,
      cantUpdateHere: 0,
      hidden: 0,
      notUsed: 0,
    });
    // Codex's own install is never checked: it is no 「其余」 either.
    expect(summaryOf([stopped(ollama), codex])).toMatchObject({
      notChecked: { ids: [ollama.id], partly: false, rest: false },
    });
  });

  it("calls a source checked in part -- a failed step, Homebrew's list not downloaded -- partly checked", () => {
    const failed = { instance_id: brew.id, message: "brew outdated exited 1" };
    expect(summaryOf([brew, uv], [], [failed])).toMatchObject({
      kind: "nothingToUpdate",
      notChecked: { ids: [brew.id], partly: true, rest: true },
    });
    expect(summaryOf([noted(brew, "IndexMayBeStale"), uv])).toMatchObject({
      notChecked: { ids: [brew.id], partly: true, rest: true },
    });
    // Still downloading its list: not checked at all.
    expect(summaryOf([noted(brew, "IndexUpdating"), uv])).toMatchObject({
      notChecked: { ids: [brew.id], partly: false, rest: true },
    });
  });

  it("names no source that the next check would find the same way: that is so every time, not news of this check", () => {
    // A Python with no pip, an Ollama at an https:// address Banager never
    // asks, a launcher left without its program: as Codex's own install,
    // neither named nor the rest, and never the plain up to date.
    const noPip = instance("pip:/opt/local/bin/python3.13", "pip", {
      read_only_reason: "ByDesign",
      status: { unavailable: "NoPip", notes: [] },
    });
    const httpsOllama: ManagerInstance = {
      ...ollama,
      id: "ollama:https://ollama.home.lan",
      status: { unavailable: "HttpsHostRefused", notes: [] },
    };
    const launcher = noted(instance("standalone-claude", "standalone-claude"), "LauncherOnly");
    expect(summaryOf([brew, noPip, httpsOllama, launcher])).toEqual({
      kind: "upToDate",
      everything: false,
      cantUpdateHere: 0,
      hidden: 0,
      notUsed: 0,
    });
    expect(summaryOf([brew, noted(codex, "LauncherOnly")])).toMatchObject({ kind: "upToDate", everything: false });
    // Beside a source not checked this time: not counted as the rest that was.
    expect(summaryOf([stopped(uv), noPip])).toMatchObject({
      kind: "nothingToUpdate",
      notChecked: { ids: [uv.id], partly: false, rest: false },
    });
    expect(summaryOf([brew, stopped(uv), noPip])).toMatchObject({
      kind: "nothingToUpdate",
      notChecked: { ids: [uv.id], partly: false, rest: true },
      everythingElse: false,
    });
    // Named all the same where an error of this round names it.
    expect(summaryOf([brew, noPip], [], [{ instance_id: noPip.id, message: "python3.13 -m pip exited 1" }])).toMatchObject({
      kind: "nothingToUpdate",
      notChecked: { ids: [noPip.id] },
    });
    // Started with sudo: opening Banager again normally is what changes it,
    // so 「这次」 holds, and Homebrew is named.
    const asRoot: ManagerInstance = { ...brew, status: { unavailable: "RefusesAsRoot", notes: [] } };
    expect(summaryOf([asRoot, uv])).toMatchObject({
      kind: "nothingToUpdate",
      notChecked: { ids: [brew.id], partly: false, rest: true },
    });
  });

  it("names each source once, in the snapshot's order, then the ones an error names that it does not list", () => {
    const errors = [
      { instance_id: uv.id, message: "uv tool list exited 2" },
      { instance_id: uv.id, message: "uv tool upgrade --dry-run exited 2" },
      { instance_id: "npm", message: "internal error detecting this source" },
    ];
    expect(summaryOf([brew, stopped(ollama), uv], [], errors)).toMatchObject({
      notChecked: { ids: [ollama.id, uv.id, "npm"], partly: true, rest: true },
    });
  });

  it("says whether the sources it does not name list anything, as the all good's `everything` does", () => {
    // uv's rows from its last answer are its own, said under the headline:
    // not held against the rest.
    const carried = candidate(uv.id, "ruff");
    expect(summaryOf([brew, stopped(uv)], [carried])).toMatchObject({
      kind: "nothingToUpdate",
      everythingElse: true,
      cantUpdateHere: 1,
    });
    // A pinned formula of a Homebrew that answered: not every other tool is up to date.
    const pinned = candidate(brew.id, "jq", { blocked: "Pinned" });
    expect(summaryOf([brew, stopped(uv)], [pinned])).toMatchObject({
      kind: "nothingToUpdate",
      everythingElse: false,
      cantUpdateHere: 1,
    });
    // pip, read-only, answered with an update.
    expect(summaryOf([brew, pip, stopped(ollama)], [candidate(pip.id, "urllib3")])).toMatchObject({
      kind: "nothingToUpdate",
      everythingElse: false,
    });
    // An update of Homebrew's the user hid.
    const glib = candidate(brew.id, "glib");
    expect(summaryOf([brew, stopped(uv)], [glib], [], hiding({ ignored_updates: [glib.key] }))).toMatchObject({
      kind: "nothingToUpdate",
      everythingElse: false,
      hidden: 1,
    });
    // Codex's own install, whose updates Banager never checks.
    expect(summaryOf([brew, stopped(uv), codex])).toMatchObject({ kind: "nothingToUpdate", everythingElse: false });
    // A row of a source only an error names, by its kind: that source's own.
    const typescript = candidate("npm:/opt/homebrew", "typescript");
    const detection = { instance_id: "npm", message: "internal error detecting this source" };
    expect(summaryOf([brew], [typescript], [detection])).toMatchObject({
      kind: "nothingToUpdate",
      everythingElse: true,
      cantUpdateHere: 1,
    });
  });

  it("still counts what there is to install, update or type the password for first", () => {
    const glib = candidate(brew.id, "glib");
    expect(summaryOf([brew, stopped(uv)], [glib]).kind).toBe("updates");
  });
});

describe("the names in the headline", () => {
  const en = i18n.getFixedT("en");
  const zh = i18n.getFixedT("zh-CN");
  const intel = instance("brew:/usr/local", "brew", { prefix: "/usr/local", exe_path: "/usr/local/bin/brew" });

  it("names one of two Homebrews as the sidebar does, both by the kind's name alone", () => {
    expect(notCheckedNames(en, { ids: [intel.id], partly: true, rest: true }, [brew, intel, uv])).toEqual([
      "Homebrew (Intel)",
    ]);
    expect(notCheckedNames(zh, { ids: [brew.id, intel.id, uv.id], partly: true, rest: false }, [brew, intel, uv])).toEqual([
      "Homebrew",
      "uv",
    ]);
  });

  it("names a source only an error names by its kind, once", () => {
    expect(notCheckedNames(en, { ids: ["npm", "npm:/opt/homebrew"], partly: false, rest: true }, [brew])).toEqual(["npm"]);
  });

  it("says 没检查 or 未检查完 -- the problems row's word under it -- with 其余 only where something else was checked", () => {
    const sources = [brew, ollama, uv];
    expect(notCheckedHeadline(zh, { ids: [ollama.id], partly: false, rest: true }, true, sources)).toBe(
      "Ollama这次没检查，其余都是最新的",
    );
    expect(notCheckedHeadline(zh, { ids: [ollama.id, uv.id], partly: true, rest: true }, true, sources)).toBe(
      "Ollama和uv这次未检查完，其余都是最新的",
    );
    expect(notCheckedHeadline(en, { ids: [ollama.id], partly: false, rest: false }, true, sources)).toBe(
      "Ollama wasn't checked this time",
    );
    expect(notCheckedHeadline(en, { ids: [brew.id, ollama.id, uv.id], partly: true, rest: false }, true, sources)).toBe(
      "Homebrew, Ollama and uv weren't fully checked this time",
    );
  });

  it("says of the rest only that what can be updated here is up to date, where the rest lists something", () => {
    const sources = [brew, ollama, uv];
    const hant = i18n.getFixedT("zh-Hant");
    const ollamaOnly = { ids: [ollama.id], partly: false, rest: true };
    expect(notCheckedHeadline(zh, ollamaOnly, false, sources)).toBe("Ollama这次没检查，其余能在这里更新的都已是最新");
    expect(notCheckedHeadline(hant, ollamaOnly, false, sources)).toBe("Ollama這次沒檢查，其餘能在這裡更新的都已是最新");
    expect(notCheckedHeadline(en, ollamaOnly, false, sources)).toBe(
      "Ollama wasn't checked this time; everything else you can update here is up to date",
    );
    const both = { ids: [ollama.id, uv.id], partly: true, rest: true };
    expect(notCheckedHeadline(en, both, false, sources)).toBe(
      "Ollama and uv weren't fully checked this time; everything else you can update here is up to date",
    );
    expect(notCheckedHeadline(zh, both, false, sources)).toBe("Ollama和uv这次未检查完，其余能在这里更新的都已是最新");
    expect(notCheckedHeadline(hant, both, false, sources)).toBe("Ollama和uv這次未檢查完，其餘能在這裡更新的都已是最新");
    // No rest to speak of: the same either way.
    expect(notCheckedHeadline(en, { ids: [ollama.id], partly: false, rest: false }, false, sources)).toBe(
      "Ollama wasn't checked this time",
    );
  });
});
