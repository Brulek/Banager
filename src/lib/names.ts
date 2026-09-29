/**
 * How the lists tell apart two tools of the same name from different
 * sources -- a `black` from Homebrew and one from pipx (spec R3).
 */

/** A name as the lists compare it: case aside, so `Black` and `black` are the same name. */
export function nameKey(name: string): string {
  return name.toLocaleLowerCase();
}

/**
 * The names (`nameKey`) that `rows` -- the rows a list shows -- have under
 * more than one source. Each such row says its source's name after the
 * tool's (`ToolRow`'s `showSource`); every other row leaves it to the mark
 * on its avatar. The same source listing a name twice is not two sources.
 */
export function namesUnderSeveralSources(rows: Iterable<{ name: string; instanceId: string }>): Set<string> {
  const sources = new Map<string, Set<string>>();
  for (const { name, instanceId } of rows) {
    const key = nameKey(name);
    const seen = sources.get(key);
    if (seen === undefined) sources.set(key, new Set([instanceId]));
    else seen.add(instanceId);
  }
  return new Set([...sources].filter(([, ids]) => ids.size > 1).map(([key]) => key));
}
