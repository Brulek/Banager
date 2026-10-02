import { useMemo } from "react";
import { useTranslation } from "react-i18next";
import { artifactKeyId } from "../store/ui";
import { searchTextOf, type SearchText } from "./searchMatch";
import { instanceLabels, toolDescription } from "./sources";
import { otherLanguage, useOtherLanguageDescription, useTranslatedDescription } from "./toolDescriptions";
import type { Snapshot } from "./types";

/**
 * Each tool's words the Installed page's search looks through
 * (`searchTextOf`), by `artifactKeyId`: its names, the line its row shows
 * under the name (`toolDescription`, as the row draws it), and the line
 * it would show in the other language. Made once for a list -- again
 * only when the list, the language or a table of lines changes -- and only
 * while `searching`, so typing in the field never redoes it, and a list
 * never searched never makes it; `null` until then.
 */
export function useSearchTexts(
  snapshot: Pick<Snapshot, "artifacts" | "instances"> | undefined,
  searching: boolean,
): ReadonlyMap<string, SearchText> | null {
  const { t, i18n } = useTranslation();
  const line = useTranslatedDescription(searching);
  const otherLine = useOtherLanguageDescription(searching);
  const other = otherLanguage(i18n.resolvedLanguage);
  return useMemo(() => {
    if (!searching || snapshot === undefined) return null;
    const otherT = other === null ? null : i18n.getFixedT(other);
    const labels = instanceLabels(t, snapshot.instances);
    const otherLabels = otherT === null ? null : instanceLabels(otherT, snapshot.instances);
    const adapters = new Map(snapshot.instances.map((instance) => [instance.id, instance.adapter_id]));
    const texts = new Map<string, SearchText>();
    for (const artifact of snapshot.artifacts) {
      const instanceId = artifact.key.instance_id;
      // Without its source in the snapshot, the id starts with its adapter's.
      const adapterId = adapters.get(instanceId) ?? instanceId.split(":")[0];
      const tool = { description: artifact.description, kind: artifact.key.kind, path: artifact.path };
      const shown = toolDescription(
        t,
        { ...tool, translated: line(artifact.key, adapterId) },
        adapterId,
        labels.get(instanceId) ?? adapterId,
      );
      const otherShown =
        otherT === null
          ? null
          : toolDescription(
              otherT,
              { ...tool, translated: otherLine(artifact.key, adapterId) },
              adapterId,
              otherLabels?.get(instanceId) ?? adapterId,
            );
      texts.set(artifactKeyId(artifact.key), searchTextOf(artifact, shown, otherShown));
    }
    return texts;
  }, [searching, snapshot, t, i18n, other, line, otherLine]);
}
