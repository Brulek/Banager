/**
 * The Overview's headline where a source was not checked this time
 * (decision I22, 2026-10-06): the source named, not 「来源」 --
 * 「uv这次没检查，其余都是最新的」 -- from what `updatesSummary` found
 * (`NotChecked`, src/lib/updateState.ts).
 */
import { adapterIdOf, adapterLabel, instanceLabels, namesInSentence } from "./sources";
import type { NotChecked } from "./updateState";
import type { ManagerInstance } from "./types";

/** Whatever `useTranslation()`'s `t` needs here; the same convention as `Translate` in src/lib/sources.ts. */
type Translate = (key: string, options?: Record<string, string | number>) => string;

/**
 * The names of the sources `notChecked` goes by, each once, in its order:
 * a listed source as the sidebar names it (`instanceLabels`: 「Homebrew
 * （Intel）」 where there are two and only that one is named), a kind every
 * source of which is named by the kind's name alone (「Homebrew」, as
 * 「部分检查未完成」 names it), and one only an error names by the name of
 * its kind (`adapterLabel`).
 */
export function notCheckedNames(t: Translate, notChecked: NotChecked, instances: readonly ManagerInstance[]): string[] {
  const labels = instanceLabels(t, instances);
  const named = new Set(notChecked.ids);
  const wholeKind = (adapterId: string) =>
    instances.filter((instance) => instance.adapter_id === adapterId).every((instance) => named.has(instance.id));
  const names: string[] = [];
  for (const id of notChecked.ids) {
    const instance = instances.find((each) => each.id === id);
    const name =
      instance === undefined || wholeKind(instance.adapter_id)
        ? adapterLabel(t, instance?.adapter_id ?? adapterIdOf(id))
        : (labels.get(id) ?? adapterLabel(t, instance.adapter_id));
    if (!names.includes(name)) names.push(name);
  }
  return names;
}

/**
 * The headline: 「uv这次没检查」 for a source that did not answer, or whose
 * list was still downloading; 「这次没检查完」 where one answered and was
 * checked in part (`partly`: a step that failed, a list that could not be
 * downloaded); and after it, only where some other source was checked in
 * full (`rest`) -- never of nothing -- 「，其余都是最新的」 where those
 * others list nothing and are all ones Banager checks (`everythingElse`),
 * else 「，其余能在这里更新的都已是最新」: no more than the all good says
 * of the same rows (「能在这里更新的都已是最新」).
 */
export function notCheckedHeadline(
  t: Translate,
  notChecked: NotChecked,
  everythingElse: boolean,
  instances: readonly ManagerInstance[],
): string {
  const names = notCheckedNames(t, notChecked, instances);
  const values = { sources: namesInSentence(t, names), count: names.length };
  if (notChecked.rest && everythingElse) {
    return notChecked.partly
      ? t("overviewAllGood.notFinishedRest", values)
      : t("overviewAllGood.notCheckedRest", values);
  }
  if (notChecked.rest) {
    return notChecked.partly
      ? t("overviewAllGood.notFinishedRestHere", values)
      : t("overviewAllGood.notCheckedRestHere", values);
  }
  return notChecked.partly ? t("overviewAllGood.notFinished", values) : t("overviewAllGood.notChecked", values);
}
