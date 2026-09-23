import { describe, expect, it } from "vitest";
import { readFileSync, readdirSync, statSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import en from "./en.json";
import zhCN from "./zh-CN.json";

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
 * Strips `//` and `/* *\/` comments out of TS/TSX source, so a key that
 * only survives in a stale comment ("see also `foo.bar`") cannot pass as
 * a live reference. String and template literals are copied through
 * untouched -- comment markers are only recognised outside of one -- which
 * is what keeps a `//` inside a URL or a backtick used for Markdown-style
 * code spans in a comment (`` `refresh()` `` and friends, all over this
 * codebase) from being misread. This is a single-pass scanner, not a
 * parser: it does not need to, because nothing that ships nests a
 * template literal inside another template literal's `${}`, and nothing
 * that ships puts `//` or `/*` inside a string (both verified by grepping
 * the tree when this was written).
 */
function stripComments(code: string): string {
  let out = "";
  let i = 0;
  const n = code.length;
  while (i < n) {
    const c = code[i];
    const c2 = code[i + 1];
    if (c === "/" && c2 === "/") {
      while (i < n && code[i] !== "\n") i += 1;
      continue;
    }
    if (c === "/" && c2 === "*") {
      i += 2;
      while (i < n && !(code[i] === "*" && code[i + 1] === "/")) i += 1;
      i = Math.min(i + 2, n);
      continue;
    }
    if (c === '"' || c === "'" || c === "`") {
      const quote = c;
      out += c;
      i += 1;
      while (i < n && code[i] !== quote) {
        if (code[i] === "\\" && i + 1 < n) {
          out += code[i] + code[i + 1];
          i += 2;
          continue;
        }
        out += code[i];
        i += 1;
      }
      if (i < n) {
        out += code[i];
        i += 1;
      }
      continue;
    }
    out += c;
    i += 1;
  }
  return out;
}

const rawSource = collectSourceFiles(SRC_DIR)
  .map((file) => readFileSync(file, "utf-8"))
  .join("\n");

const source = stripComments(rawSource);

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
 * other way, because `expr` in each call site is a Rust-enum-shaped value
 * or an array of route names, neither of which static analysis of this
 * file can enumerate on its own.
 */
const INTERPOLATED_SUBTREES: Record<string, readonly string[]> = {
  // src/components/Sidebar.tsx: `t(\`nav.${p}\`)` over `PAGES`.
  nav: ["installed", "updates", "settings"],
  // src/components/OperationBar.tsx: `t(\`operations.kind.${current.kind}\`)`
  // over `OpKind` (src/lib/types.ts).
  "operations.kind": ["Install", "Uninstall", "Upgrade"],
  // src/components/OperationBar.tsx: `t(\`operations.status.${current.status}\`)`
  // over `OpStatus` (src/lib/types.ts).
  "operations.status": ["Queued", "Running", "CancelRequested", "Cancelling", "Verifying", "Done"],
  // src/components/OperationBar.tsx and src/components/LogDrawer.tsx:
  // `t(\`operations.outcome.${outcomeKey(...)}\`)` over every string
  // `outcomeKey()` can return (src/lib/format.ts), which in turn mirrors
  // `Outcome`'s variant names, `Attention`'s and `Fault`'s
  // (src/lib/types.ts), plus `FailedSilent` for a tool that failed without
  // a word on stderr.
  "operations.outcome": [
    "Succeeded",
    "Cancelled",
    "Unconfirmed",
    "NeedsAttention.NotInstalledAfterInstall",
    "NeedsAttention.StillInstalledAfterUninstall",
    "NeedsAttention.GoneAfterUpgrade",
    "Failed",
    "FailedSilent",
    "CanagerFailed.Panicked",
    "CanagerFailed.SourceGone",
    "CanagerFailed.ProgramMissing",
    "CanagerFailed.SpawnFailed",
    "CanagerFailed.Unsupported",
    "CanagerFailed.Internal",
  ],
};

/**
 * Whether anything that ships still looks this key up.
 *
 * Most call sites spell the key out, so the first check finds them. Two
 * shapes compose one at runtime and both are in use here:
 *
 * - a static head and an interpolated tail -- `t(\`nav.${p}\`)`,
 *   `t(\`operations.kind.${current.kind}\`)`. Checked against
 *   `INTERPOLATED_SUBTREES` above: the head's call site must still exist
 *   *and* the tail must be one of the named, enumerated values -- not
 *   "any key under this head", which is as far as a head match alone can
 *   tell you.
 * - an interpolated head and a static tail -- `t(\`${prefix}.description\`)`
 *   over a lookup table (`READ_ONLY_NOTICE_KEYS`). Both halves have to be
 *   present: the head as a whole string literal, the tail spelled out
 *   after an interpolation.
 */
function isReferenced(key: string, haystack: string = source): boolean {
  if (occursAsToken(haystack, key)) return true;

  for (const [head, tails] of Object.entries(INTERPOLATED_SUBTREES)) {
    if (!key.startsWith(`${head}.`)) continue;
    const tail = key.slice(head.length + 1);
    if (tails.includes(tail) && haystack.includes(`${head}.\${`)) return true;
  }

  const segments = key.split(".");
  for (let i = 1; i < segments.length; i += 1) {
    const head = segments.slice(0, i).join(".");
    const tail = segments.slice(i).join(".");
    if (haystack.includes(`"${head}"`) && occursAsToken(haystack, `}.${tail}`, { before: false })) {
      return true;
    }
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
   * OperationBar.tsx interpolates `operations.kind.${current.kind}` over
   * `OpKind`, which has exactly three variants. Before this table
   * existed, the mere presence of `operations.kind.${` in the source was
   * read as clearing *every* key under `operations.kind.*` -- including
   * one no Rust variant can ever produce.
   */
  it("does not let an interpolated head claim tails the code cannot produce", () => {
    expect(isReferenced("operations.kind.Install")).toBe(true); // a real OpKind variant
    expect(isReferenced("operations.kind.Bogus")).toBe(false); // not a variant of OpKind
  });

  /**
   * Comments are stripped out of the haystack before anything is matched
   * against it, so a key that only survives as a mention -- "see also
   * ..." -- does not count as a use.
   */
  it("does not count a mention inside a comment as a use", () => {
    const commentOnly = "// still wired through updates.cannotCheckShort, see below\n";
    expect(commentOnly.includes("updates.cannotCheckShort")).toBe(true); // present in the raw text
    expect(isReferenced("updates.cannotCheckShort", stripComments(commentOnly))).toBe(false);
    expect(isReferenced("updates.cannotCheckShort")).toBe(true); // the real call site still counts
  });

  /** The base case the other three exist to protect: an unused key with no
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

describe("zh-CN uses full-width punctuation in Chinese prose", () => {
  /**
   * The Chinese translation drifted back toward untouched machine
   * translation over several review rounds: half-width ， ： （ ） creeping
   * in between Chinese characters instead of the full-width forms the rest
   * of the file already uses. This guards the fix so it can't come back
   * silently string by string.
   */
  it("has no half-width , : ( ) sitting between two CJK characters", () => {
    const offenders = flattenEntries(zhCN)
      .filter(([, value]) => ASCII_PUNCT_BETWEEN_CJK.test(value))
      .map(([key, value]) => `${key}: ${value}`);

    expect(offenders, `half-width punctuation inside Chinese prose:\n${offenders.join("\n")}`).toEqual([]);
  });
});
