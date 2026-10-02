import type { CommandFact, InstalledArtifact } from "./types";

/**
 * Why a tool is in the Installed page's search results: by its words --
 * the name its row shows or the package's own, anywhere in either
 * ("visual-studio-code" finds "Microsoft Visual Studio Code"), or a word
 * of the line its row shows under the name, in the window's language or
 * the other one ("编程", "video" and "JSON" find what says so) -- or,
 * where nothing the row shows matches, by a command it puts on the Mac
 * (`ArtifactFacts.commands`), the word a user types in Terminal and may
 * know better than the package's name: "rg" finds ripgrep, "pip3.13"
 * python@3.13, "agy" Antigravity CLI. A command matches by its start
 * only, so "g" does not find ripgrep through "rg"; the command it matched
 * by is the one the row names (「命令：rg」), so that a row found by a
 * word it does not show says which.
 */
export type SearchMatch = { by: "text" } | { by: "command"; command: string };

const BY_TEXT: SearchMatch = { by: "text" };

/**
 * A tool's words the search looks through, besides its commands:
 * lowercased once for a list (`searchTextOf`), not once a keystroke a row,
 * so a search over thousands of tools stays quick.
 */
export interface SearchText {
  /** The name its row shows. */
  name: string;
  /** The line its row shows under the name, in the window's language. */
  line: string;
  /** The package's own name, which the row may not show. */
  packageName: string;
  /** The line the row would show in the other language, or "" for none. */
  otherLine: string;
}

/**
 * Space beside a Chinese character: the gap the row draws there
 * (`text-autospace`), or `autospace`'s narrow one (U+2006, which `\s`
 * matches) where the web view cannot draw it, or one a user types after
 * reading the row -- 「AI 编程」. Neither the copy nor the tables write
 * one there, so a search sees none either side.
 */
const HAN_GAP = /\s+(?=\p{Script=Han})|(?<=\p{Script=Han})\s+/gu;

/** `text`, lowercased, with no space beside a Chinese character (`HAN_GAP`). */
function folded(text: string | null): string {
  return (text ?? "").toLowerCase().replace(HAN_GAP, "");
}

/**
 * The search field's `query` as the search reads it: trimmed and folded
 * as the words it looks through are, so 「AI 编程」 finds what 「ai编程」 does.
 * The Installed page and its subtitle's count both read it so.
 */
export function searchNeedle(query: string): string {
  return folded(query.trim());
}

/**
 * The words `artifact`'s search looks through: `line`, the one its row
 * shows under its name (`toolDescription`), and `otherLine`, the one it
 * would show in the other language, or null where there is no other.
 */
export function searchTextOf(
  artifact: Pick<InstalledArtifact, "display_name" | "key">,
  line: string | null,
  otherLine: string | null,
): SearchText {
  const shownLine = folded(line);
  const other = folded(otherLine);
  return {
    name: folded(artifact.display_name),
    line: shownLine,
    packageName: folded(artifact.key.name),
    otherLine: other === shownLine ? "" : other,
  };
}

const LATIN = /[a-z0-9]/;

/**
 * Whether `needle` begins a word of `text`: in Chinese anywhere, which has
 * no spaces to start a word after; in Latin where no letter or digit is
 * before it -- "video" in "audio and video", "JSON" in 「命令行JSON处理工具」,
 * but not "rg" in "large", which would bury ripgrep's row among every
 * line that says "large" or "merge".
 */
function beginsAWord(text: string, needle: string): boolean {
  if (!LATIN.test(needle[0])) return text.includes(needle);
  for (let at = text.indexOf(needle); at !== -1; at = text.indexOf(needle, at + 1)) {
    if (at === 0 || !LATIN.test(text[at - 1])) return true;
  }
  return false;
}

/**
 * How `artifact` matches `needle` -- the search field's text as
 * `searchNeedle` reads it -- or null where it does not, through `text`,
 * its words (`searchTextOf`; its names alone where not given, made only
 * then). An empty needle matches every tool. A command is named only
 * where nothing the row shows matches: "agy" is Antigravity CLI's package
 * name as well as its command, and its row shows neither.
 */
export function searchMatch(
  artifact: Pick<InstalledArtifact, "display_name" | "key" | "facts">,
  needle: string,
  given?: SearchText,
): SearchMatch | null {
  if (needle === "") return BY_TEXT;
  const text = given ?? searchTextOf(artifact, null, null);
  if (text.name.includes(needle) || beginsAWord(text.line, needle)) return BY_TEXT;
  const command = commandMatching(artifact.facts?.commands ?? [], needle);
  if (command !== null) return { by: "command", command };
  return text.packageName.includes(needle) || beginsAWord(text.otherLine, needle) ? BY_TEXT : null;
}

/**
 * The command `needle` names, case aside: one that is exactly it, else the
 * first (by name, as the facts are sorted) that starts with it; null for
 * none. As the facts spell it, for the row's hint.
 */
export function commandMatching(commands: readonly CommandFact[], needle: string): string | null {
  if (needle === "") return null;
  let prefix: string | null = null;
  for (const { name } of commands) {
    const lower = name.toLowerCase();
    if (lower === needle) return name;
    if (prefix === null && lower.startsWith(needle)) prefix = name;
  }
  return prefix;
}
