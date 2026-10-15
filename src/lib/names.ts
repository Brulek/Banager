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

/**
 * An Ollama model pulled by a path -- from another registry,
 * `modelscope.cn/Qwen/Qwen2.5-Coder-7B-Instruct-GGUF:Q4_K_M` or `hf.co/…`,
 * or from a user's namespace on Ollama's, `someone/model:tag` -- in two
 * parts: `name`, its last segment, the model and its tag, which is what
 * tells two such models apart and what a row shows as its name; and
 * `from`, the rest of the path, where it came from. Null for a model named
 * without one (`llama3.2:3b`), and for anything but a model: an npm scope
 * (`@angular/cli`) or a Homebrew tap is part of what the package is called.
 */
export function modelPath(key: { kind: string }, name: string): { name: string; from: string } | null {
  if (key.kind !== "Model") return null;
  const cut = name.lastIndexOf("/");
  if (cut <= 0 || cut === name.length - 1) return null;
  return { name: name.slice(cut + 1), from: name.slice(0, cut) };
}

/**
 * The name a list shows a row by, and so sorts it by: a model's last path
 * segment (`modelPath`) -- its Qwen under Q, not under the registry's m --
 * and otherwise the name itself.
 */
export function listedName(key: { kind: string }, name: string): string {
  return modelPath(key, name)?.name ?? name;
}
