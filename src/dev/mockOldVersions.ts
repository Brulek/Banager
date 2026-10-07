import type { InstalledArtifact, OpRequest, Plan, Warning } from "../lib/types";
import type { LogLine } from "./mockPlans";
import { sameKey, type World } from "./mockData";

/**
 * The preview's stand-in for what `BrewAdapter::plan` builds from the
 * Cellar since the author's decision U9 (r6;
 * crates/banager-core/src/adapters/brew/mod.rs, `brew::kegs`): a Homebrew
 * formula's update is followed by `brew cleanup <name>`, its preview first
 * naming every version installed now; an uninstall of a formula with more
 * than one version passes `--force` and names each. The pretend Cellar of a
 * formula is its row's version and `other_versions` (mockData.ts), oldest
 * first.
 */
function versionsOf(artifact: InstalledArtifact): string[] {
  return [...(artifact.facts.homebrew?.other_versions ?? []), artifact.version].sort((a, b) =>
    a.localeCompare(b, "en", { numeric: true }),
  );
}

/** The row `request` is about, when it is a Homebrew formula. */
function formulaOf(world: World, request: OpRequest): InstalledArtifact | undefined {
  if (request.artifact_kind !== "Formula" || !request.instance_id.startsWith("brew:")) return undefined;
  return world.artifacts.find((artifact) =>
    sameKey(artifact.key, { instance_id: request.instance_id, kind: request.artifact_kind, name: request.name }),
  );
}

/** `plan`, with U9's cleanup after an update and `--force` on an uninstall of more than one version. */
export function withMockOldVersions(plan: Plan, world: World, request: OpRequest): Plan {
  const artifact = formulaOf(world, request);
  if (artifact === undefined || !("Command" in plan.action)) return plan;
  const versions = versionsOf(artifact);
  const { program, args, env } = plan.action.Command;
  if (request.kind === "Upgrade") {
    const cleans: Warning = { HomebrewCleansUpOldVersions: { versions } };
    return {
      ...plan,
      action: { CommandThen: { program, args, env, then: [["cleanup", request.name]] } },
      warnings: [cleans, ...plan.warnings],
    };
  }
  if (request.kind === "Uninstall" && versions.length > 1) {
    const every: Warning = { HomebrewRemovesEveryVersion: { versions } };
    const scope = plan.warnings.findIndex((warning) => typeof warning !== "string" && "UninstallScope" in warning);
    const warnings = [...plan.warnings];
    warnings.splice(scope + 1, 0, every);
    return {
      ...plan,
      action: { Command: { program, args: [...args.slice(0, -1), "--force", ...args.slice(-1)], env } },
      warnings,
    };
  }
  return plan;
}

/** Whether `plan` is an update a `brew cleanup` follows. */
function cleansUp(plan: Plan): boolean {
  return "CommandThen" in plan.action && plan.action.CommandThen.then.some((argv) => argv[0] === "cleanup");
}

/** What `brew cleanup <name>` prints after an update a cleanup follows, Banager's line first. */
export function mockCleanupLines(plan: Plan, world: World): LogLine[] {
  if (!cleansUp(plan)) return [];
  const artifact = formulaOf(world, plan.request);
  const old = artifact === undefined ? [] : versionsOf(artifact);
  return [
    { note: { CleaningUpOldVersions: { name: plan.request.name } } },
    ...old.map(
      (version): LogLine => ({
        stream: "Stdout",
        line: `Removing: /opt/homebrew/Cellar/${plan.request.name.split("/").pop()}/${version}... (1,287 files, 61.4MB)`,
      }),
    ),
  ];
}

/** After an update a cleanup followed: the row keeps no other version. */
export function applyMockCleanup(plan: Plan, world: World): void {
  if (!cleansUp(plan)) return;
  const artifact = formulaOf(world, plan.request);
  const homebrew = artifact?.facts.homebrew;
  if (artifact === undefined || homebrew === null || homebrew === undefined) return;
  // A new facts object: another row may share the one it had.
  artifact.facts = { ...artifact.facts, homebrew: { ...homebrew, other_versions: [] } };
}
