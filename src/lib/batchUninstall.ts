/**
 * Uninstalling several tools at once from the Installed page (round 5,
 * r7; the spec is .superpowers/r5/specs/r7-batch-uninstall.md): which
 * rows can be ticked, which of the ticked tools one batch includes and
 * why each of the others is left out, the order the included ones run
 * in, and what the confirmation says about all of them together -- the
 * commands Terminal will no longer find, the data that stays, the space
 * they take. Pure: no React, no `t` beyond a parameter, so every rule is
 * tested on its own (`batchUninstall.test.ts`). The sheet that shows it
 * is `BatchUninstallSheet`; each tool's preview and uninstall are the
 * single uninstall's own (`plan_operation`, `submit_operation`), nothing
 * new runs on the Mac.
 */
import { canWrite, isAvailable, uninstallHoldKey } from "./sources";
import { deletesForGood } from "./warnings";
import { saysSize, sizeViewOf } from "./sizes";
import { artifactKeyId } from "../store/ui";
import type {
  InstalledArtifact,
  ManagerInstance,
  Measured,
  OpSummary,
  Plan,
  Sizes,
  UninstallScope,
  Warning,
} from "./types";

// ---------------------------------------------------------------- choices
//
// Four of this feature's choices are the author's to confirm (the spec's
// 「需要作者拍板的四件事」). Each is said once, here, so that changing one
// is changing a line:
//
// 1. The always-visible checkbox column on the Installed page: the rows'
//    `selectable` and the list header (`InstalledSelectionHeader`), both
//    decided by `tickable` below.
// 2. At most `MAX_BATCH_UNINSTALL` tools in one batch.
// 3. The four kinds of uninstall a batch leaves to their own row
//    (`SINGLE_ONLY`).
// 4. Dependents first on one Homebrew (`DEPENDENTS_FIRST`), which needs
//    the engine to start the operations of one source in the order they
//    were confirmed (`OperationManager`'s queue,
//    crates/banager-core/src/ops/mod.rs).

/**
 * The most tools one batch uninstalls (author decision 2): each Homebrew
 * one runs `brew uses --installed` before the sheet can say anything, and
 * the sheet has to be read before it is confirmed. A beginner removing
 * more than this at once deserves a second look.
 */
export const MAX_BATCH_UNINSTALL = 20;

/**
 * The most `plan_operation` calls one batch has in flight: a Homebrew
 * preview runs `brew uses --installed`, which can take a while (up to
 * 120 s), and nothing else limits how many of them run at once.
 */
export const PLAN_CONCURRENCY = 3;

/**
 * How long an issued preview is trusted before the batch plans again
 * instead of submitting it: a little under `PLAN_LIFETIME` (ten minutes,
 * crates/banager-core/src/session/plans.rs), after which `submit` refuses
 * it as expired.
 */
export const PLAN_FRESH_FOR_MS = 9.5 * 60 * 1000;

/**
 * The kinds of uninstall a batch never includes (author decision 3): each
 * is listed under 「不会卸载」 with why, and uninstalled from its own row,
 * whose alert says 「永久卸载」 and the rest. `true` leaves the kind out.
 *
 * - `noCancel`: it cannot be cancelled once it starts (rustup's own
 *   uninstall), so 「全部取消」 could not stop the batch.
 * - `permanent`: a line says it deletes files for good (`deletesForGood`).
 * - `unseen`: a cask whose own steps delete what Banager cannot know in
 *   advance (`UNSEEN_SCOPES`).
 * - `host`: the program another included tool needs to be uninstalled at
 *   all (Homebrew's `ollama` for an Ollama model): different sources run
 *   in parallel, so its turn cannot be put after theirs.
 */
export const SINGLE_ONLY: Readonly<Record<"noCancel" | "permanent" | "unseen" | "host", boolean>> = {
  noCancel: true,
  permanent: true,
  unseen: true,
  host: true,
};

/**
 * Whether a Homebrew formula a ticked tool of the same Homebrew depends
 * on is uninstalled in the same batch, after it (author decision 4) --
 * which needs the engine to start one source's operations in the order
 * they were confirmed. Without that, `false`: such a formula is left out
 * this time (`afterOthers`), to be uninstalled on its own afterwards.
 */
export const DEPENDENTS_FIRST = true;

// ------------------------------------------------------------ ticking rows

/**
 * Whether a row offers Uninstall at all: its source can be changed and the
 * tool does not refuse (`uninstall_blocked`). `Session::issue_plan`
 * refuses both in Rust whatever the page shows (spec §2.5).
 */
export function uninstallOffered(artifact: InstalledArtifact, instance: ManagerInstance): boolean {
  return canWrite(instance) && artifact.uninstall_blocked === null;
}

/** What else holds a row's Uninstall, beside its source (`uninstallHeld`). */
export interface UninstallHolds {
  /** The first check's list, before its update checks are done. */
  preview: boolean;
  /** An uninstall of this tool is queued or running. */
  underway: boolean;
  /**
   * This tool's newest uninstall succeeded while another operation on its
   * source still runs: its row is the last check's, carried forward until
   * the source is free to be read again (`uninstalledWhileBusy`).
   */
  uninstalled: boolean;
}

/**
 * Whether a row's offered Uninstall is held, disabled, for now: the first
 * check is not done, the source did not answer, it is updating its list
 * (`uninstallHoldKey`), or this tool is being -- or has just been --
 * uninstalled.
 */
export function uninstallHeld(artifact: InstalledArtifact, instance: ManagerInstance, holds: UninstallHolds): boolean {
  return (
    uninstallOffered(artifact, instance) &&
    (holds.preview ||
      !isAvailable(instance) ||
      uninstallHoldKey(instance) !== null ||
      holds.underway ||
      holds.uninstalled)
  );
}

/**
 * Whether a row gets a checkbox (author decision 1): exactly when its own
 * Uninstall button is there and enabled -- the Updates page's rule, a box
 * with the button's state and no other.
 */
export function tickable(artifact: InstalledArtifact, instance: ManagerInstance, holds: UninstallHolds): boolean {
  return uninstallOffered(artifact, instance) && !uninstallHeld(artifact, instance, holds);
}

/** The facts `uninstalledWhileBusy` reads of an operation. */
type OpFacts = Pick<OpSummary, "id" | "kind" | "instance_id" | "artifact_kind" | "name" | "status" | "outcome">;

/**
 * The tools (by artifact key id) whose newest uninstall ended `Succeeded`
 * while another operation on their source is still active (spec §6.5): a
 * refresh skips a source an operation holds and carries its rows forward,
 * so the row of a tool a batch has already removed stays listed until the
 * source's last operation ends. Its row then says 「已卸载」, with no box and
 * no Uninstall. One pass over the operations, for every row of the list.
 */
export function uninstalledWhileBusy(operations: readonly OpFacts[]): Set<string> {
  const busy = new Set(operations.filter((op) => op.status !== "Done").map((op) => op.instance_id));
  const newest = new Map<string, OpFacts>();
  for (const op of operations) {
    if (op.kind !== "Uninstall" || !busy.has(op.instance_id)) continue;
    const id = artifactKeyId({ instance_id: op.instance_id, kind: op.artifact_kind, name: op.name });
    const was = newest.get(id);
    if (was === undefined || op.id > was.id) newest.set(id, op);
  }
  const ids = new Set<string>();
  for (const [id, op] of newest) {
    if (op.status === "Done" && op.outcome === "Succeeded") ids.add(id);
  }
  return ids;
}

/**
 * The ticks a toolbar button and a batch act on: the ticked rows the list
 * shows now, of those that can be ticked -- "what you see is what you act
 * on", as on the Updates page. A tick hidden by a search or another
 * source is kept, and neither counted nor uninstalled.
 */
export function countedTicks<T extends { id: string }>(shownTickable: readonly T[], ticked: readonly string[]): T[] {
  const ticks = new Set(ticked);
  return shownTickable.filter((row) => ticks.has(row.id));
}

/**
 * What the list header's 「全选」 does with the rows the list shows that
 * can be ticked: untick them all when every one is ticked; else tick them
 * all, when there are no more than the most one batch takes; else untick
 * them, the only thing it can do then.
 */
export function selectAllAction(shownTickable: number, counted: number): "select" | "clear" {
  if (shownTickable > 0 && counted === shownTickable) return "clear";
  return shownTickable <= MAX_BATCH_UNINSTALL ? "select" : "clear";
}

// ----------------------------------------------------------- the batch

/** One ticked tool, from the moment the sheet opens. */
export interface BatchCandidate {
  /** `artifactKeyId` of its key. */
  id: string;
  artifact: InstalledArtifact;
  instance: ManagerInstance;
  /** The name its row showed when the sheet opened. */
  name: string;
  /** Its preview, once issued. */
  plan: Plan | null;
  /** Why its preview was refused, in the backend's words, once refused. */
  planError: string | null;
}

/** Why a ticked tool is not part of the batch (spec §5.3). */
export type Exclusion =
  /** X1: its preview was refused, in the backend's words. */
  | { kind: "refused"; raw: string }
  /** X2: it cannot be cancelled once it starts. */
  | { kind: "noCancel" }
  /** X3: it deletes something for good. */
  | { kind: "permanent" }
  /** X4: a cask's own steps, whose deletions cannot be known in advance. */
  | { kind: "unseen" }
  /** X5: the program these included tools (ids) need to be uninstalled. */
  | { kind: "host"; by: string[] }
  /**
   * X6: still used by software not ticked -- named with any ticked one left
   * out itself, every one that stays -- as the list shows them, or as
   * Homebrew gave them.
   */
  | { kind: "stillNeeded"; names: string[] }
  /** X6: still used by ticked tools (ids) that are left out themselves. */
  | { kind: "neededByExcluded"; ids: string[] }
  /** With `DEPENDENTS_FIRST` off: it could go only after these ticked tools (ids). */
  | { kind: "afterOthers"; ids: string[] }
  /** X7: an order its dependencies allow cannot be found. */
  | { kind: "cycle" };

/** One included tool, and the included tools (ids) it must run after. */
export interface IncludedItem {
  candidate: BatchCandidate;
  after: string[];
}

export interface ExcludedItem {
  candidate: BatchCandidate;
  reason: Exclusion;
}

export interface Classification {
  /** In the order they run (`runOrder`). */
  included: IncludedItem[];
  /** In the order they were ticked from the list. */
  excluded: ExcludedItem[];
}

/**
 * The cask sentences whose uninstall runs steps of its own Banager cannot
 * read the effect of (`skipsTrash`'s note in src/lib/warnings.ts): a
 * program the cask names, Ruby around the uninstall, or a record it could
 * not read.
 */
const UNSEEN_SCOPES: ReadonlySet<UninstallScope> = new Set<UninstallScope>([
  "HomebrewCask",
  "HomebrewCaskStepsUnseen",
  "HomebrewCaskStepsOnlyUnseen",
]);

/** A plan's `UninstallScope`, or null for one with none. */
function scopeOf(warnings: readonly Warning[]): UninstallScope | null {
  for (const warning of warnings) {
    if (typeof warning !== "string" && "UninstallScope" in warning) return warning.UninstallScope.what;
  }
  return null;
}

/**
 * The sources whose tools run through a program another source may have
 * installed -- npm's `npm`, pipx's `pipx`, uv's `uv`, cargo's `cargo`,
 * Ollama's `ollama` -- and which fail without it (spec §5.3, X5). Not a
 * tool's own installer: its launcher's name is the tool's, which another
 * copy of the tool provides too (npm's `claude`), and the other copy is a
 * twin, not its host.
 */
const HOSTED_SOURCES: ReadonlySet<string> = new Set(["npm", "pipx", "uv", "cargo", "ollama"]);

/** The program a source runs, by name: the last part of its `exe_path`. */
function programName(instance: ManagerInstance): string {
  return instance.exe_path.slice(instance.exe_path.lastIndexOf("/") + 1);
}

/** The last part of a Homebrew name: `claudebar` of `gautham-v/tap/claudebar`. */
function lastSegment(name: string): string {
  return name.slice(name.lastIndexOf("/") + 1);
}

/**
 * Whether Homebrew's name for a dependent (`brew uses --installed`, a
 * plan's `affected`) names `artifact`: the same name, or the same last
 * part, as brew's own `qualified_key` matches a tapped cask by its short
 * name (crates/banager-core/src/adapters/brew/mod.rs).
 */
function namesArtifact(affected: string, artifact: InstalledArtifact): boolean {
  return artifact.key.name === affected || lastSegment(artifact.key.name) === lastSegment(affected);
}

/** The first of X1-X4 a ticked tool meets, or null (spec §5.3). */
function ownExclusion(candidate: BatchCandidate): Exclusion | null {
  const { plan } = candidate;
  if (plan === null) return { kind: "refused", raw: candidate.planError ?? "" };
  if (SINGLE_ONLY.noCancel && plan.cancel_policy === "NoCancel") return { kind: "noCancel" };
  if (SINGLE_ONLY.permanent && plan.warnings.some(deletesForGood)) return { kind: "permanent" };
  const scope = scopeOf(plan.warnings);
  if (SINGLE_ONLY.unseen && scope !== null && UNSEEN_SCOPES.has(scope)) return { kind: "unseen" };
  return null;
}

/** Each of `values` once, in the order first seen. */
function unique<T>(values: readonly T[]): T[] {
  return [...new Set(values)];
}

/**
 * The members of every cycle in `edges` among `ids` -- each a strongly
 * connected group of more than one, or one that is its own edge (Tarjan's
 * algorithm). Homebrew's dependencies form no cycle; this is the guard
 * that a list `brew uses` could not have produced cannot loop the run
 * order (X7).
 */
function cycleMembers(ids: readonly string[], edges: ReadonlyMap<string, readonly string[]>): string[] {
  const inGraph = new Set(ids);
  let counter = 0;
  const index = new Map<string, number>();
  const low = new Map<string, number>();
  const stack: string[] = [];
  const onStack = new Set<string>();
  const members: string[] = [];
  const visit = (id: string) => {
    index.set(id, counter);
    low.set(id, counter);
    counter += 1;
    stack.push(id);
    onStack.add(id);
    for (const next of edges.get(id) ?? []) {
      if (!inGraph.has(next)) continue;
      if (!index.has(next)) {
        visit(next);
        low.set(id, Math.min(low.get(id)!, low.get(next)!));
      } else if (onStack.has(next)) {
        low.set(id, Math.min(low.get(id)!, index.get(next)!));
      }
    }
    if (low.get(id) === index.get(id)) {
      const group: string[] = [];
      for (;;) {
        const member = stack.pop()!;
        onStack.delete(member);
        group.push(member);
        if (member === id) break;
      }
      if (group.length > 1 || (edges.get(id) ?? []).includes(id)) members.push(...group);
    }
  };
  for (const id of ids) if (!index.has(id)) visit(id);
  return members;
}

/**
 * Which ticked tools one batch uninstalls, and why each other one is left
 * out (spec §5.3, X1 to X7), with the included ones in the order they run
 * (`runOrder`). `artifacts` is the snapshot's list, for what Homebrew
 * names as a tool's dependents (`affected`): a dependent ticked too is
 * uninstalled first, one not ticked keeps the tool. X5 and X6 are applied
 * until nothing changes, since leaving one tool out can leave out another:
 * Homebrew's `pipx`, the program of a ticked pipx tool, is left out, and
 * with it `python@3.13`, which that `pipx` still needs.
 *
 * `DependentsUnknown` is no reason: such a tool is included with its line,
 * and runs after the rest of its source, where Homebrew's own refusal
 * still protects whatever needs it.
 */
export function classify(candidates: readonly BatchCandidate[], artifacts: readonly InstalledArtifact[]): Classification {
  const reasons = new Map<string, Exclusion>();
  for (const candidate of candidates) {
    const reason = ownExclusion(candidate);
    if (reason !== null) reasons.set(candidate.id, reason);
  }
  const ticked = new Set(candidates.map((candidate) => candidate.id));
  // What Homebrew names as each tool's dependents, matched to the artifacts
  // of the same source -- one name can match a formula and a cask.
  const dependents = new Map<string, Array<{ name: string; matches: InstalledArtifact[] }>>();
  for (const candidate of candidates) {
    const affected = candidate.plan?.affected ?? [];
    dependents.set(
      candidate.id,
      affected.map((name) => ({
        name,
        matches: artifacts.filter(
          (artifact) =>
            artifact.key.instance_id === candidate.instance.id &&
            artifactKeyId(artifact.key) !== candidate.id &&
            namesArtifact(name, artifact),
        ),
      })),
    );
  }
  const included = () => candidates.filter((candidate) => !reasons.has(candidate.id));
  // The included tools each one runs after: its dependents, every one
  // ticked and included (it is covered, or it would be left out).
  const afterOf = (id: string): string[] =>
    unique((dependents.get(id) ?? []).flatMap(({ matches }) => matches.map((match) => artifactKeyId(match.key))));
  // Why a tool's dependents keep it, or null when every one is ticked and
  // included, to be uninstalled first. Every one that stays is named, ticked
  // or not, where one is not ticked: each still needs it.
  const dependentsVerdict = (id: string): Exclusion | null => {
    const named = dependents.get(id) ?? [];
    const staying: string[] = [];
    const leftOut: string[] = [];
    let notTicked = false;
    for (const { name, matches } of named) {
      if (matches.length === 0) {
        staying.push(name);
        notTicked = true;
      }
      for (const match of matches) {
        const matchId = artifactKeyId(match.key);
        if (!ticked.has(matchId)) notTicked = true;
        else if (reasons.has(matchId)) leftOut.push(matchId);
        else continue;
        staying.push(match.display_name || match.key.name);
      }
    }
    if (notTicked) return { kind: "stillNeeded", names: unique(staying) };
    if (leftOut.length > 0) return { kind: "neededByExcluded", ids: unique(leftOut) };
    if (named.length > 0 && !DEPENDENTS_FIRST) return { kind: "afterOthers", ids: afterOf(id) };
    return null;
  };

  for (let changed = true; changed; ) {
    changed = false;
    // X5: the program an included tool of another source runs through.
    if (SINGLE_ONLY.host) {
      const now = included();
      for (const candidate of now) {
        const provides = new Set(candidate.artifact.facts.commands.map((command) => command.name));
        const by = now.filter(
          (other) =>
            other.id !== candidate.id &&
            other.instance.id !== candidate.instance.id &&
            HOSTED_SOURCES.has(other.instance.adapter_id) &&
            provides.has(programName(other.instance)),
        );
        if (by.length > 0) {
          reasons.set(candidate.id, { kind: "host", by: by.map((other) => other.id) });
          changed = true;
        }
      }
      if (changed) continue;
    }
    // X6: a dependent Homebrew named that is not uninstalled first.
    for (const candidate of included()) {
      const verdict = dependentsVerdict(candidate.id);
      if (verdict === null) continue;
      reasons.set(candidate.id, verdict);
      changed = true;
    }
    if (changed) continue;
    // X7: an order the dependencies allow, or none.
    const ids = included().map((candidate) => candidate.id);
    const looped = cycleMembers(ids, new Map(ids.map((id) => [id, afterOf(id)])));
    for (const id of looped) reasons.set(id, { kind: "cycle" });
    changed = looped.length > 0;
  }
  // Said with every exclusion known: a dependent left out later in the
  // same pass is named too. (Exclusions only grow, so the kind holds.)
  for (const [id, reason] of reasons) {
    if (reason.kind === "stillNeeded" || reason.kind === "neededByExcluded") {
      reasons.set(id, dependentsVerdict(id) ?? reason);
    }
  }

  const kept = included();
  const after = new Map(kept.map((candidate) => [candidate.id, afterOf(candidate.id)]));
  const order = runOrder(kept, after, (candidate) =>
    (candidate.plan?.warnings ?? []).includes("DependentsUnknown"),
  );
  return {
    included: order.map((candidate) => ({ candidate, after: after.get(candidate.id) ?? [] })),
    excluded: candidates.flatMap((candidate) => {
      const reason = reasons.get(candidate.id);
      return reason === undefined ? [] : [{ candidate, reason }];
    }),
  };
}

/**
 * The order the included tools run in (spec §5.4), a stable topological
 * sort: each time, the first in list order whose dependents in the batch
 * have all been taken -- except that a tool whose dependents Homebrew
 * could not check (`unknown`) waits while another tool of its source
 * that could go is left, so that anything in the batch that might need it
 * goes first. With no tool that can go (a cycle `classify` has already
 * left out), the rest keep the list's order.
 */
export function runOrder<T extends { id: string; instance: ManagerInstance }>(
  items: readonly T[],
  after: ReadonlyMap<string, readonly string[]>,
  unknown: (item: T) => boolean,
): T[] {
  const taken = new Set<string>();
  const inBatch = new Set(items.map((item) => item.id));
  const order: T[] = [];
  const remaining = [...items];
  const ready = (item: T) => (after.get(item.id) ?? []).every((id) => !inBatch.has(id) || taken.has(id));
  while (remaining.length > 0) {
    const available = remaining.filter(ready);
    const next =
      available.find(
        (item) =>
          !unknown(item) ||
          !available.some(
            (other) => other !== item && other.instance.id === item.instance.id && !unknown(other),
          ),
      ) ??
      available[0] ??
      remaining[0];
    order.push(next);
    taken.add(next.id);
    remaining.splice(remaining.indexOf(next), 1);
  }
  return order;
}

// ------------------------------------------------- what it says, together

/** One command typing which runs another copy once its tool is uninstalled. */
export interface TakenOver {
  command: string;
  /** The copies that run it then: those that come after this one, not uninstalled. */
  by: InstalledArtifact[];
}

/**
 * What typing the included tools' commands runs once they are gone (spec
 * §5.7): a command an included tool's copy runs now (`Runs`) is taken
 * over where an artifact the batch leaves -- not included -- has it, with
 * this tool's copy the one in its way (`ShadowedBy` this key); otherwise
 * Terminal will not find it. Nothing for a command Banager said nothing
 * about (`state: null`), which is every command while its `PATH` is not
 * the login shell's.
 */
export function terminalCommands(
  included: readonly InstalledArtifact[],
  artifacts: readonly InstalledArtifact[],
): { lost: string[]; takenOver: Map<string, TakenOver[]> } {
  const going = new Set(included.map((artifact) => artifactKeyId(artifact.key)));
  const lost: string[] = [];
  const takenOver = new Map<string, TakenOver[]>();
  for (const artifact of included) {
    const id = artifactKeyId(artifact.key);
    for (const fact of artifact.facts.commands) {
      if (fact.state !== "Runs") continue;
      const by = artifacts.filter(
        (other) =>
          !going.has(artifactKeyId(other.key)) &&
          other.facts.commands.some(
            (command) =>
              command.name === fact.name &&
              command.state !== null &&
              typeof command.state !== "string" &&
              "ShadowedBy" in command.state &&
              command.state.ShadowedBy.by !== null &&
              artifactKeyId(command.state.ShadowedBy.by) === id,
          ),
      );
      if (by.length === 0) lost.push(fact.name);
      else takenOver.set(id, [...(takenOver.get(id) ?? []), { command: fact.name, by }]);
    }
  }
  return { lost: unique(lost).sort(), takenOver };
}

/**
 * What the included tools leave behind, each path once (spec §5.5, item
 * 6): their `WillKeep` and `KeepsData` lines, two copies of Claude Code
 * keeping one `~/.claude`. Where one tool's line has a size and another's
 * does not -- a `KeepsData` and a `WillKeep` for the same path -- the one
 * with the size is kept. `owners`: whose each path is, by the names the
 * list showed, in run order.
 */
export function mergeKept(items: ReadonlyArray<{ name: string; warnings: readonly Warning[] }>): {
  warnings: Warning[];
  owners: Map<string, string[]>;
} {
  const byPath = new Map<string, Warning>();
  const owners = new Map<string, string[]>();
  for (const { name, warnings } of items) {
    for (const warning of warnings) {
      if (typeof warning === "string") continue;
      const path = "WillKeep" in warning ? warning.WillKeep.path : "KeepsData" in warning ? warning.KeepsData.path : null;
      if (path === null) continue;
      const had = byPath.get(path);
      if (had === undefined || (typeof had !== "string" && "WillKeep" in had && "KeepsData" in warning)) {
        byPath.set(path, warning);
      }
      owners.set(path, unique([...(owners.get(path) ?? []), name]));
    }
  }
  return { warnings: [...byPath.values()], owners };
}

// ------------------------------------------------------------------ sizes

/**
 * What one tool takes, as the By Size column would say it (`sizeCellOf`):
 * the size its source reports (an Ollama model's), or a measured size of
 * the version listed; null for none, while it is measured, and for a
 * measured 0.
 */
export function itemSizeOf(sizes: Sizes | undefined, artifact: InstalledArtifact): Measured | null {
  if (artifact.size_bytes !== null) return { bytes: artifact.size_bytes, partial: false, at_least: false };
  const view = sizeViewOf(sizes, artifact);
  if (view === null || view.kind === "measuring" || !saysSize(view.measured)) return null;
  return view.measured;
}

/**
 * What `artifacts` take together, and how many of them have no size to
 * add (spec §5.6): the sum of what each takes, hedged by the weakest of
 * them -- 「…以上」 where one has no size yet or was cut short, 「…，部分无法
 * 读取」 where part of one could not be read, else 「约…」 -- or null when
 * none has a size. What they take, never what removing them frees.
 */
export function batchSizeOf(
  sizes: Sizes | undefined,
  artifacts: readonly InstalledArtifact[],
): { measured: Measured | null; unknown: number } {
  let bytes = 0;
  let known = 0;
  let atLeast = false;
  let partial = false;
  for (const artifact of artifacts) {
    const size = itemSizeOf(sizes, artifact);
    if (size === null) continue;
    known += 1;
    bytes += size.bytes;
    atLeast ||= size.at_least;
    partial ||= size.partial;
  }
  const unknown = artifacts.length - known;
  if (known === 0) return { measured: null, unknown };
  return { measured: { bytes, partial, at_least: atLeast || unknown > 0 }, unknown };
}

/** The why behind a batch's size, by the i18n key of each line its ⓘ says (spec §5.6). */
export type SizeCaveat =
  | "batchUninstall.takesUnknown"
  | "batchUninstall.takesTrash"
  | "batchUninstall.takesKept"
  | "batchUninstall.takesShared"
  | "sizes.programOnly";

/**
 * The lines behind the total's ⓘ that hold for the included tools, in
 * order: some sizes not known yet (`unknown`), what goes to the Trash
 * frees nothing until it is emptied, data that stays is not counted, a
 * part shared with other software stays (an Ollama model's layers, a uv
 * tool's cache, a formula's dependencies), and a program's files are all
 * that is counted of a Cargo crate or a tool with its own installer.
 */
export function sizeCaveats(
  items: ReadonlyArray<{ artifact: InstalledArtifact; instance: ManagerInstance; plan: Plan }>,
  unknown: number,
): SizeCaveat[] {
  const caveats: SizeCaveat[] = [];
  if (unknown > 0) caveats.push("batchUninstall.takesUnknown");
  if (items.some(({ plan }) => "TrashPaths" in plan.action)) caveats.push("batchUninstall.takesTrash");
  const keeps = (warning: Warning) => typeof warning !== "string" && ("WillKeep" in warning || "KeepsData" in warning);
  if (items.some(({ plan }) => plan.warnings.some(keeps))) caveats.push("batchUninstall.takesKept");
  const shared = ({ artifact, instance }: { artifact: InstalledArtifact; instance: ManagerInstance }) =>
    artifact.key.kind === "Model" ||
    instance.adapter_id === "uv" ||
    (instance.adapter_id === "brew" && artifact.key.kind === "Formula");
  if (items.some(shared)) caveats.push("batchUninstall.takesShared");
  if (items.some(({ instance }) => instance.adapter_id === "cargo" || instance.adapter_id.startsWith("standalone-"))) {
    caveats.push("sizes.programOnly");
  }
  return caveats;
}
