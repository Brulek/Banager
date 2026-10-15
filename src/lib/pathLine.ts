/**
 * The line that puts a folder Terminal does not search at the front of the
 * search path, for the person to add to a shell startup file themselves
 * (the author's decision U15 a, 2026-10-06): `export PATH="$HOME/.grok/bin:$PATH"`.
 * Banager only shows and copies it -- it never edits a shell file
 * (docs/what-we-run.md, "What Banager never does").
 *
 * `dir` is the folder as the details show it (`CommandState.NotOnPath.dir`):
 * `~`-relative, or absolute. `~` becomes `$HOME`, which the shell expands
 * inside double quotes where it would leave a `~` alone; every character
 * the shell reads inside double quotes (`\`, `"`, `$`, `` ` ``) is escaped,
 * so the folder is taken exactly as written. No line, `null`, for a folder
 * the search path cannot hold -- one with a `:`, which separates its
 * folders, or a line break -- or one that is neither absolute nor under the
 * home folder (`~other/…` included): there is nothing true to give.
 */
export function exportPathLine(dir: string): string | null {
  if (dir.includes(":") || dir.includes("\n") || dir.includes("\r")) return null;
  let folder: string;
  if (dir === "~") folder = "$HOME";
  else if (dir.startsWith("~/")) folder = `$HOME/${escaped(dir.slice(2))}`;
  else if (dir.startsWith("/")) folder = escaped(dir);
  else return null;
  return `export PATH="${folder}:$PATH"`;
}

/** `text` with each character the shell reads inside double quotes escaped. */
function escaped(text: string): string {
  return text.replace(/[\\"$`]/g, (character) => `\\${character}`);
}
