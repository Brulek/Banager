/**
 * What `?state=huge` installs on top of `?state=many` (docs/ui-preview.md,
 * `addHuge` in ./mockData.ts): about 4,200 more tools, for a Mac whose
 * owner has used Homebrew for years and has several thousand formulae and
 * casks. No list is written out here: each name is made from one of
 * ./mockManyNames.ts's real ones, the way sources name relatives of a tool
 * -- a versioned formula (`hugo@2`), a `-cli` or `-utils` beside it, a
 * library (`libuv`), a cask's `@beta`, an npm `create-` package, a
 * `cargo-` subcommand -- so the rows read like a real list without a new
 * data file. Which relatives a name gets comes from its own seeded stream
 * (`seededStream` in ./mockData.ts, passed in), so the list is the same on
 * every run. None of these is a name the logo pack or the description
 * tables know, so the rows show the program tile with the source's mark
 * on its corner, and the source's line. Dev-only.
 */
import { MANY_CARGO, MANY_CASKS, MANY_DEPENDENCIES, MANY_FORMULAE, MANY_NPM, MANY_PIPX, MANY_UV } from "./mockManyNames";

export interface HugeNames {
  formulae: string[];
  dependencies: string[];
  casks: Array<{ token: string; name: string }>;
  npm: string[];
  pipx: string[];
  uv: string[];
  cargo: string[];
}

/** Relatives a formula may have, as Homebrew names them. */
const FORMULA_RELATIVES = [
  (n: string) => `${n}@1`,
  (n: string) => `${n}@2`,
  (n: string) => `${n}@3`,
  (n: string) => `${n}-cli`,
  (n: string) => `${n}-lsp`,
  (n: string) => `${n}-utils`,
  (n: string) => `${n}-docs`,
  (n: string) => `${n}-tools`,
  (n: string) => `${n}-completion`,
  (n: string) => `${n}-server`,
];

/** Libraries Homebrew installs for the others. */
const DEPENDENCY_RELATIVES = [
  (n: string) => `${n}@1`,
  (n: string) => `${n}@2`,
  (n: string) => `${n}-static`,
  (n: string) => `${n}-dev`,
  (n: string) => `${n}-data`,
  (n: string) => `${n}-headers`,
];

const CASK_RELATIVES = [
  { token: (t: string) => `${t}@beta`, name: (n: string) => `${n} Beta` },
  { token: (t: string) => `${t}@nightly`, name: (n: string) => `${n} Nightly` },
  { token: (t: string) => `${t}@preview`, name: (n: string) => `${n} Preview` },
  { token: (t: string) => `${t}-helper`, name: (n: string) => `${n} Helper` },
  { token: (t: string) => `${t}-cli`, name: (n: string) => `${n} CLI` },
];

const NPM_RELATIVES = [
  (n: string) => `${n}-cli`,
  (n: string) => `create-${n}`,
  (n: string) => `${n}-js`,
  (n: string) => `${n}-plugin`,
];

const PYPI_RELATIVES = [
  (n: string) => `python-${n}`,
  (n: string) => `${n}-py`,
  (n: string) => `mkdocs-${n}`,
  (n: string) => `pytest-${n}`,
];

const CARGO_RELATIVES = [(n: string) => `cargo-${n}`, (n: string) => `${n}-rs`, (n: string) => `${n}-cli`];

/** `make(name)`, inside an npm scope's: `@google/gemini-cli` -> `@google/create-gemini-cli`. */
function scoped(name: string, make: (name: string) => string): string {
  const slash = name.indexOf("/");
  return name.startsWith("@") && slash > 0 ? `${name.slice(0, slash + 1)}${make(name.slice(slash + 1))}` : make(name);
}

/**
 * Each of `bases`' relatives that `stream(base)` keeps -- each one with
 * chance `share` -- and that no list in `taken` has yet, in `bases`' order.
 */
function relatives(
  bases: readonly string[],
  makers: ReadonlyArray<(name: string) => string>,
  share: number,
  stream: (seed: string) => () => number,
  seed: string,
  taken: Set<string>,
): string[] {
  const out: string[] = [];
  for (const base of bases) {
    const next = stream(`huge|${seed}|${base}`);
    for (const make of makers) {
      if (next() >= share) continue;
      const name = scoped(base, make);
      if (taken.has(name)) continue;
      taken.add(name);
      out.push(name);
    }
  }
  return out;
}

/**
 * The names `?state=huge` adds, from `stream` (`seededStream`): none of
 * them one of `existing`, the names the Mac already has.
 */
export function hugeNames(stream: (seed: string) => () => number, existing: ReadonlySet<string>): HugeNames {
  const brew = new Set<string>([...existing, ...MANY_FORMULAE, ...MANY_DEPENDENCIES, ...MANY_CASKS.map((c) => c.token)]);
  const formulae = relatives(MANY_FORMULAE, FORMULA_RELATIVES, 0.5, stream, "formula", brew);
  const libraries = relatives(
    MANY_FORMULAE,
    [(n) => `lib${n.replace(/^lib/, "")}`],
    0.25,
    stream,
    "library",
    brew,
  );
  const dependencies = [
    ...relatives(MANY_DEPENDENCIES, DEPENDENCY_RELATIVES, 0.6, stream, "dependency", brew),
    ...libraries,
  ];
  const casks: HugeNames["casks"] = [];
  for (const cask of MANY_CASKS) {
    const next = stream(`huge|cask|${cask.token}`);
    for (const relative of CASK_RELATIVES) {
      if (next() >= 0.75) continue;
      const token = relative.token(cask.token);
      if (brew.has(token)) continue;
      brew.add(token);
      casks.push({ token, name: relative.name(cask.name) });
    }
  }
  const npm = relatives([...MANY_NPM, ...MANY_FORMULAE], NPM_RELATIVES, 0.18, stream, "npm", new Set([...existing, ...MANY_NPM]));
  const pipx = relatives(
    [...MANY_PIPX, ...MANY_FORMULAE],
    PYPI_RELATIVES.slice(0, 2),
    0.13,
    stream,
    "pipx",
    new Set([...existing, ...MANY_PIPX]),
  );
  const uv = relatives([...MANY_UV, ...MANY_FORMULAE], PYPI_RELATIVES.slice(2), 0.13, stream, "uv", new Set([...existing, ...MANY_UV]));
  const cargo = relatives(
    [...MANY_CARGO, ...MANY_FORMULAE],
    CARGO_RELATIVES,
    0.12,
    stream,
    "cargo",
    new Set([...existing, ...MANY_CARGO]),
  );
  return { formulae, dependencies, casks, npm, pipx, uv, cargo };
}
