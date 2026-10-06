import { describe, expect, it } from "vitest";
import { readFileSync, readdirSync, statSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import * as ts from "typescript";
import en from "./en.json";
import zhCN from "./zh-CN.json";
import zhHant from "./zh-Hant.json";

const __dirname = path.dirname(fileURLToPath(import.meta.url));

const PLURAL_SUFFIXES = ["_zero", "_one", "_two", "_few", "_many", "_other"];

function stripPluralSuffix(key: string): string {
  const suffix = PLURAL_SUFFIXES.find((s) => key.endsWith(s));
  return suffix ? key.slice(0, -suffix.length) : key;
}

function flattenKeys(value: unknown, prefix = ""): string[] {
  if (typeof value !== "object" || value === null) {
    return [prefix];
  }
  return Object.entries(value as Record<string, unknown>).flatMap(([key, child]) =>
    flattenKeys(child, prefix ? `${prefix}.${key}` : key),
  );
}

function normalizedKeySet(resource: unknown): Set<string> {
  return new Set(flattenKeys(resource).map(stripPluralSuffix));
}

describe("i18n key parity", () => {
  it("keeps every current Simplified Chinese key, plural suffix and marker in Traditional Chinese", () => {
    expect(flattenKeys(zhHant).sort()).toEqual(flattenKeys(zhCN).sort());
    const traditional = new Map(flattenEntries(zhHant));
    const markers = (text: string) => text.match(/\{\{[^}]+\}\}|<[^>]+>/g) ?? [];
    for (const [key, value] of flattenEntries(zhCN)) {
      const translated = traditional.get(key)!;
      expect(translated.trim(), key).not.toBe("");
      expect(markers(translated).sort(), key).toEqual(markers(value).sort());
    }
  });
  it("names each language in the language popup by its own name, in every language", () => {
    // Someone looking for their language finds it written as they write
    // it, whatever the window is in now (walk-5 W5-8).
    for (const locale of [en, zhCN, zhHant]) {
      const { english, chinese, traditionalChinese } = locale.settings.language;
      expect([english, chinese, traditionalChinese]).toEqual(["English", "简体中文", "繁體中文"]);
    }
    // And System Default as macOS's Language & Region says it in Taiwan.
    expect(zhHant.settings.language.system).toBe("系統預設值");
  });
  it("has the same set of keys in en.json and zh-CN.json", () => {
    const enKeys = normalizedKeySet(en);
    const zhKeys = normalizedKeySet(zhCN);

    const missingInZh = [...enKeys].filter((k) => !zhKeys.has(k)).sort();
    const missingInEn = [...zhKeys].filter((k) => !enKeys.has(k)).sort();

    expect(missingInZh, `zh-CN.json is missing: ${missingInZh.join(", ")}`).toEqual([]);
    expect(missingInEn, `en.json is missing: ${missingInEn.join(", ")}`).toEqual([]);
  });
});

// Every `.ts`/`.tsx` that ships, joined into one haystack. Tests are left
// out on purpose: a key that only a test still looks up is exactly the
// orphan this is hunting for.
const SRC_DIR = path.resolve(__dirname, "..");

function collectSourceFiles(dir: string): string[] {
  return readdirSync(dir).flatMap((entry) => {
    const full = path.join(dir, entry);
    if (statSync(full).isDirectory()) return collectSourceFiles(full);
    if (!/\.tsx?$/.test(full)) return [];
    if (/\.test\.tsx?$/.test(full)) return [];
    return [full];
  });
}

/**
 * Every leaf token of `sourceFile`, in source order -- every punctuation
 * mark, keyword, identifier and literal, not just the "significant" nodes
 * `ts.forEachChild` visits. `getChildren()` is backed by the real parser
 * output (it re-derives the token list the parser already produced), so a
 * `/like/this/` that the parser resolved as a single RegularExpressionLiteral
 * token comes back as one leaf, never as a run of characters a naive
 * scanner would re-interpret quote by quote. That is what a hand-rolled
 * scanner cannot do without reimplementing the parser's regex/divide
 * disambiguation.
 */
function forEachToken(sourceFile: ts.SourceFile, node: ts.Node, cb: (token: ts.Node) => void): void {
  const children = node.getChildren(sourceFile);
  if (children.length === 0) {
    cb(node);
    return;
  }
  for (const child of children) forEachToken(sourceFile, child, cb);
}

/**
 * Strips `//` and `/* *\/` comments out of one TS/TSX file's source, so a
 * key that only survives in a stale comment ("see also `foo.bar`") cannot
 * pass as a live reference. Built on the TypeScript compiler API
 * (`ts.createSourceFile` plus `ts.getLeadingCommentRanges` /
 * `ts.getTrailingCommentRanges` over every real token) instead of a
 * hand-rolled quote-parity scanner: the previous scanner tracked "inside a
 * string" as one open/close pair per quote character, so it could not see
 * a template literal nested inside another template literal's `${...}` --
 * exactly src/lib/format.ts:12, `` `'${token.replace(/'/g, `'\\''`)}'` ``,
 * whose inner `` `'\\''` `` opens and closes its own backtick pair before
 * the outer template does. The old scanner read that inner open-backtick
 * as closing the *outer* string, which inverted its notion of "inside a
 * string" for the rest of that file and -- because all files were joined
 * into one haystack before stripping -- for every file read after it too
 * (435 of 635 comments in shipping src survived "stripping", measured
 * against TypeScript's own comment ranges). Operating on the parser's
 * token stream sidesteps both that nesting and the regex-vs-divide
 * ambiguity a raw scanner would also hit, and running once per file (see
 * below) means one file's result can never leak into another's
 * regardless.
 */
function stripComments(code: string, fileName: string): string {
  const scriptKind = fileName.endsWith(".tsx") ? ts.ScriptKind.TSX : ts.ScriptKind.TS;
  const sourceFile = ts.createSourceFile(fileName, code, ts.ScriptTarget.Latest, true, scriptKind);
  const fullText = sourceFile.getFullText();

  const seenStart = new Set<number>();
  const commentRanges: ts.CommentRange[] = [];
  function collect(ranges: ts.CommentRange[] | undefined): void {
    if (!ranges) return;
    for (const range of ranges) {
      if (seenStart.has(range.pos)) continue;
      seenStart.add(range.pos);
      commentRanges.push(range);
    }
  }
  forEachToken(sourceFile, sourceFile, (token) => {
    collect(ts.getLeadingCommentRanges(fullText, token.getFullStart()));
    collect(ts.getTrailingCommentRanges(fullText, token.end));
  });
  commentRanges.sort((a, b) => a.pos - b.pos);

  let out = "";
  let last = 0;
  for (const { pos, end } of commentRanges) {
    out += fullText.slice(last, pos);
    out += " "; // keep tokens either side from fusing into one identifier
    last = end;
  }
  out += fullText.slice(last);
  return out;
}

// Each file is parsed and stripped on its own -- never as a shared
// haystack -- so a parse quirk in one file has no way to reach another's
// comments. Only after stripping are the per-file results joined.
const source = collectSourceFiles(SRC_DIR)
  .map((file) => stripComments(readFileSync(file, "utf-8"), file))
  .join("\n");

const IDENT_CHAR = /[A-Za-z0-9_]/;

/**
 * Whether `needle` sits in `haystack` as a delimited token, not merely as
 * a run of characters inside a longer name. By default this requires both
 * the character immediately before the match and the one immediately
 * after it to fall outside `[A-Za-z0-9_]`. That is what stops a key from
 * reading as "referenced" purely because it happens to be a literal
 * prefix (or, symmetrically, a suffix) of some longer string the source
 * really does contain -- `updates.cannotCheck` sitting inside
 * `updates.cannotCheckDetail`, or `warnings.thirdPartyRegist` sitting
 * inside the real key `warnings.thirdPartyRegistry`. Pass `before: false`
 * / `after: false` for a search pattern that is already self-delimiting
 * on that side (it ends in a quote, in `${`, ...).
 */
function occursAsToken(
  haystack: string,
  needle: string,
  { before = true, after = true }: { before?: boolean; after?: boolean } = {},
): boolean {
  let from = 0;
  for (;;) {
    const idx = haystack.indexOf(needle, from);
    if (idx === -1) return false;
    const beforeChar = idx > 0 ? haystack[idx - 1] : "";
    const afterChar = haystack[idx + needle.length] ?? "";
    const beforeOk = !before || !IDENT_CHAR.test(beforeChar);
    const afterOk = !after || !IDENT_CHAR.test(afterChar);
    if (beforeOk && afterOk) return true;
    from = idx + 1;
  }
}

/**
 * `t(\`head.${expr}\`)` call sites, and, named, every tail value `expr`
 * can actually produce there. A bare head match used to claim the whole
 * subtree under it -- every key `head.*` counted as "referenced" whether
 * or not `expr` can ever hold that value. This table is the fix: a key
 * under one of these heads only counts as referenced when its tail is
 * literally enumerated below, against the runtime type or list that
 * drives the interpolation. Each entry names its source so the two can be
 * checked against each other by hand; there is no way to check it any
 * other way, because `expr` in each call site is a Rust-enum-shaped value,
 * which static analysis of this file cannot enumerate on its own. (The
 * sidebar's `t(\`nav.${p}\`)` over its list of pages used to be one; it
 * spells each page's key out now, in `PAGE_LABEL_KEYS`. So do the
 * operation's words for each kind and status, which the operation bar used
 * to compose: `OP_RUNNING_KEYS`, `OP_STATUS_KEYS` and the rest in
 * src/lib/operations.ts.)
 */
const INTERPOLATED_SUBTREES: Record<string, readonly string[]> = {
  // `outcomeSentence` in src/lib/operations.ts, for the operation bar and
  // the log drawer: `t(\`operations.outcome.${outcomeKey(...)}\`)` over
  // every string `outcomeKey()` can return (src/lib/format.ts), which in turn mirrors
  // `Outcome`'s variant names, `Attention`'s and `Fault`'s
  // (src/lib/types.ts), plus `FailedSilent` for a tool that failed without
  // a word on stderr.
  // A success is said by kind, 「已更新」 (`OP_SUCCEEDED_KEYS`), not here.
  "operations.outcome": [
    "Cancelled",
    "Unconfirmed",
    "NeedsAttention.NotInstalledAfterInstall",
    "NeedsAttention.StillInstalledAfterUninstall",
    "NeedsAttention.GoneAfterUpgrade",
    "NeedsAttention.UnchangedAfterUpgrade",
    "NeedsAttention.BackAfterUninstall",
    "Failed",
    "FailedSilent",
    "BanagerFailed.Panicked",
    "BanagerFailed.ProgramMissing",
    "BanagerFailed.SpawnFailed",
    "BanagerFailed.HomebrewStillUpdating",
    "BanagerFailed.PathChanged",
    "BanagerFailed.FormulaChanged",
    "BanagerFailed.Internal",
  ],
};

/**
 * Whether anything that ships still looks this key up.
 *
 * Most call sites spell the key out, so the first check finds them. One
 * shape composes a key at runtime: a static head and an interpolated tail
 * -- `t(\`operations.kind.${current.kind}\`)`. Checked against
 * `INTERPOLATED_SUBTREES` above: the head's call site must still exist
 * *and* the tail must be one of the named, enumerated values -- not "any
 * key under this head", which is as far as a head match alone can tell
 * you. Nothing composes a key's *head* any more: the read-only sentences
 * used to be `${prefix}.description` over a table of prefixes, and are
 * named whole now (`READ_ONLY_DETAIL_KEYS` in src/lib/sources.ts). A key
 * built that way again would read as an orphan here -- the safe way for
 * this to be wrong.
 */
function isReferenced(key: string, haystack: string = source): boolean {
  if (occursAsToken(haystack, key)) return true;

  for (const [head, tails] of Object.entries(INTERPOLATED_SUBTREES)) {
    if (!key.startsWith(`${head}.`)) continue;
    const tail = key.slice(head.length + 1);
    if (tails.includes(tail) && haystack.includes(`${head}.\${`)) return true;
  }
  return false;
}

describe("i18n keys are reachable", () => {
  /**
   * This project's recurring failure mode is a field, variant, badge or
   * key that is defined, mirrored and never rendered. The parity test
   * above cannot catch one: a key nothing reads stays green as long as
   * both locales carry it. Seven review rounds found several of these by
   * hand; this finds the next one.
   */
  it("has no key in en.json that nothing renders", () => {
    const orphans = [...normalizedKeySet(en)].filter((key) => !isReferenced(key)).sort();

    expect(orphans, `nothing looks these up: ${orphans.join(", ")}`).toEqual([]);
  });
});

describe("the reachability guard itself", () => {
  /**
   * `warnings.thirdPartyRegistry` is real (src/lib/warnings.ts) and sits,
   * spelled out, in the tree's actual source. `warnings.thirdPartyRegist`
   * -- one letter short of it -- is not a key anywhere. A plain
   * `haystack.includes(key)` cannot tell those apart: the shorter string
   * is a literal prefix of the longer one, so it reads as present too.
   * That is the shape of bug this branch introduced twice on its own
   * keys (`updates.cannotCheck` inside `cannotCheckDetail` /
   * `cannotCheckShort`; `sourceNotice.unreachable.description` inside
   * `descriptionWithRows`) -- a genuine orphan that happens to be a
   * prefix of something real would have stayed invisible.
   */
  it("does not let a key's own prefix stand in for it", () => {
    expect(source.includes("warnings.thirdPartyRegist")).toBe(true); // the naive check's blind spot
    expect(occursAsToken(source, "warnings.thirdPartyRegist")).toBe(false); // not a real reference
    expect(isReferenced("warnings.thirdPartyRegist")).toBe(false);
    expect(isReferenced("warnings.thirdPartyRegistry")).toBe(true); // the real key still passes
  });

  /**
   * `outcomeSentence` interpolates `operations.outcome.${outcomeKey(...)}`
   * over what `outcomeKey` can return. Before this table existed, the
   * mere presence of `operations.outcome.${` in the source was read as
   * clearing *every* key under `operations.outcome.*` -- including one no
   * Rust variant can ever produce. (A next step, such as
   * `operations.outcome.UnconfirmedDetail`, is not a tail the table lists:
   * it counts only because `outcomeDetailKey` spells it out.)
   */
  it("does not let an interpolated head claim tails the code cannot produce", () => {
    expect(isReferenced("operations.outcome.Cancelled")).toBe(true); // a real Outcome variant
    expect(isReferenced("operations.outcome.Bogus")).toBe(false); // not an Outcome variant
    expect(isReferenced("operations.outcome.UnconfirmedDetail")).toBe(true); // spelled out
    expect(isReferenced("operations.outcome.CancelledDetail")).toBe(false); // spelled nowhere
    // The kind is spelled out now, and only real kinds are.
    expect(isReferenced("operations.running.Install")).toBe(true);
    expect(isReferenced("operations.running.Bogus")).toBe(false);
  });

  /**
   * Comments are stripped out of the haystack before anything is matched
   * against it, so a key that only survives as a mention -- "see also
   * ..." -- does not count as a use.
   */
  it("does not count a mention inside a comment as a use", () => {
    const commentOnly = "// still wired through updates.cannotCheckShort, see below\n";
    expect(commentOnly.includes("updates.cannotCheckShort")).toBe(true); // present in the raw text
    expect(isReferenced("updates.cannotCheckShort", stripComments(commentOnly, "self-test.ts"))).toBe(false);
    expect(isReferenced("updates.cannotCheckShort")).toBe(true); // the real call site still counts
  });

  /**
   * `stripComments` used to track "inside a string" as one open/close pair
   * per quote character, so it could not see a template literal nested
   * inside another template literal's `${...}` -- exactly
   * src/lib/format.ts:12, `` `'${token.replace(/'/g, `'\\''`)}'` ``, whose
   * inner `` `'\\''` `` opens and closes its own backtick pair before the
   * outer template does. Reading that inner open-backtick as the outer
   * string's close flipped the scanner's notion of "inside a string" for
   * the rest of the file, so a comment placed after it survived
   * "stripping" and a key mentioned only in it read as referenced. (The
   * regex literal `/'/g` earlier on the same line is not the cause: the old
   * scanner mishandles this line just as badly with the regex changed to
   * `/x/g` -- and conversely, dropping down to one backslash before the
   * inner template's closing quotes, instead of the two `token.replace`
   * actually writes, makes the old scanner strip the comment correctly,
   * i.e. that shape does not reproduce the bug at all.) Unlike the
   * single-line haystack the old version of this self-test used -- which
   * cannot exercise a scanner desync that only shows up *after* the
   * nesting flips parity -- this one runs a whole multi-statement file
   * through `stripComments` and checks the comment that comes after the
   * nested template literal.
   */
  it("does not desync on a template literal nested inside another template's ${}", () => {
    const file = [
      "export function quote(token: string): string {",
      "  return `'${token.replace(/'/g, `'\\\\''`)}'`;",
      "}",
      "// still wired through updates.cannotCheckShort, see below",
      'const other = t("some.other.key");',
    ].join("\n");
    const stripped = stripComments(file, "nested-template.ts");
    expect(stripped).toContain("token.replace"); // real code, kept
    expect(stripped).toContain('t("some.other.key")'); // real code after the comment, kept
    expect(stripped).not.toContain("updates.cannotCheckShort"); // the comment, gone
    expect(isReferenced("updates.cannotCheckShort", stripped)).toBe(false);
  });

  /**
   * The read-only sentences used to be looked up as `${prefix}.description`,
   * and the guard for that shape once accepted any string literal anywhere
   * as a stand-in for `prefix` -- so the page names `"updates"`,
   * `"settings"`, `"installed"` (string literals elsewhere, for `nav.*`)
   * paired with an unrelated `}.title`/`}.description` and cleared keys
   * nothing defines or reads. The sentences are named whole now; none of
   * the five phantoms passes, and the two real sentences pass by name.
   */
  it("does not let an unrelated call site's `.title` or `.description` stand in for a key", () => {
    for (const phantom of [
      "updates.description",
      "updates.title",
      "settings.description",
      "installed.title",
      "installed.description",
      "sourceNotice.pipReadOnly.title",
    ]) {
      expect(isReferenced(phantom)).toBe(false);
    }
    expect(isReferenced("sourceNotice.pipReadOnly.description")).toBe(true);
    expect(isReferenced("sourceNotice.prefixNotWritable.description")).toBe(true);
  });

  /** The base case the other five exist to protect: an unused key with no
   * prefix trick, no interpolated head and no comment involved is still
   * caught. */
  it("still finds a genuine orphan", () => {
    const haystack = 'const other = t("some.other.key");';
    expect(isReferenced("totally.unused.key", haystack)).toBe(false);
  });
});

function flattenEntries(value: unknown, prefix = ""): [string, string][] {
  if (typeof value === "string") {
    return [[prefix, value]];
  }
  if (typeof value !== "object" || value === null) {
    return [];
  }
  return Object.entries(value as Record<string, unknown>).flatMap(([key, child]) =>
    flattenEntries(child, prefix ? `${prefix}.${key}` : key),
  );
}

const CJK = "\\u4e00-\\u9fff";
// A half-width comma, colon or parenthesis with CJK text pressed against
// both sides is the most recognisable machine-translation tell in
// Simplified Chinese -- mainland punctuation (GB/T 15834) uses the
// full-width forms ， ： （ ） there instead. This does not flag ASCII next
// to an interpolation, a Latin word or a domain name (crates.io,
// nodejs.org): those are legitimate half-width uses this file also
// contains, and only a CJK character on *both* sides marks Chinese prose.
const ASCII_PUNCT_BETWEEN_CJK = new RegExp(`[${CJK}][,:()][${CJK}]`);

describe.each([["zh-CN", zhCN], ["zh-Hant", zhHant]])("%s uses full-width punctuation in Chinese prose", (_name, locale) => {
  /**
   * The Chinese translation drifted back toward untouched machine
   * translation over several review rounds: half-width ， ： （ ） creeping
   * in between Chinese characters instead of the full-width forms the rest
   * of the file already uses. This guards the fix so it can't come back
   * silently string by string.
   */
  it("has no half-width , : ( ) sitting between two CJK characters", () => {
    const offenders = flattenEntries(locale)
      .filter(([, value]) => ASCII_PUNCT_BETWEEN_CJK.test(value))
      .map(([key, value]) => `${key}: ${value}`);

    expect(offenders, `half-width punctuation inside Chinese prose:\n${offenders.join("\n")}`).toEqual([]);
  });
});
