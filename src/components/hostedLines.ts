import type { useTranslation } from "react-i18next";
import { hostedTools } from "../lib/batchUninstall";
import { namesInSentence } from "../lib/sources";
import type { WarningLine } from "../lib/warnings";
import type { InstalledArtifact, ManagerInstance } from "../lib/types";

type Translate = ReturnType<typeof useTranslation>["t"];

/** At most three names, then how many in all: 「aider-chat、httpie、black等5个」. */
function fewNames(t: Translate, names: readonly string[]): string {
  if (names.length <= 3) return namesInSentence(t, [...names]);
  return t("commands.names", {
    names: names.slice(0, 3).join(t("common.listSeparator")),
    count: names.length,
    rest: names.length - 3,
  });
}

/**
 * What uninstalling `artifact` does to the tools other sources installed
 * through it (`hostedTools`), a caution each source: Homebrew's `pipx`
 * gone, the pipx tools stay with nothing in Banager able to update or
 * uninstall them; a Homebrew `node` gone, npm's tools may stop working.
 * Said in the single uninstall's alert and under the tool in a batch's
 * sheet alike. Empty for anything no other source runs through.
 */
export function hostedLines(
  t: Translate,
  artifact: InstalledArtifact | undefined,
  instances: readonly ManagerInstance[],
  artifacts: readonly InstalledArtifact[],
): WarningLine[] {
  if (artifact === undefined) return [];
  return hostedTools(artifact, instances, artifacts).map(({ manages, runs, program, runsOn, tools }) => {
    const names = fewNames(
      t,
      tools.map((tool) => tool.display_name || tool.key.name),
    );
    const key = manages && runs ? "reviewFixes.hostedBoth" : manages ? "reviewFixes.hostedManages" : "reviewFixes.hostedRuns";
    return {
      text: t(key, { names, program, count: tools.length, runsOn }),
      detail: null,
      caution: true,
    };
  });
}


/**
 * What a batch's Homebrew formula that runs after `hosts` -- its ticked
 * dependents, the source programs among them -- may take from those
 * sources' tools: `python@3.13` after Homebrew's `pipx`, whose tools'
 * environments were most likely made with the Python pipx ran on. Banager
 * cannot read which Python each one has, so it says "may". One caution a
 * source, under the formula.
 */
export function hostedThroughLines(
  t: Translate,
  hosts: ReadonlyArray<InstalledArtifact>,
  instances: readonly ManagerInstance[],
  artifacts: readonly InstalledArtifact[],
): WarningLine[] {
  return hosts.flatMap((host) =>
    hostedTools(host, instances, artifacts)
      .filter(({ manages }) => manages)
      .map(({ program, tools }) => ({
        text: t("reviewFixes.hostedThrough", {
          names: fewNames(
            t,
            tools.map((tool) => tool.display_name || tool.key.name),
          ),
          program,
          count: tools.length,
        }),
        detail: null,
        caution: true,
      })),
  );
}
