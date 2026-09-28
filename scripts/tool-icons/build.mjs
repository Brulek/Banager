/**
 * `pnpm icons:build`: builds the logo pack Canager ships,
 * src/assets/tool-icons/, from scripts/tool-icons/mapping.json -- the only
 * input it trusts. Dev-time only: the app shows the pack committed to the
 * repository and fetches nothing to do so, and no test runs this.
 *
 * - A glyph, `si:<slug>` in the mapping, is Simple Icons' logo of that
 *   slug from the pinned devDependency `simple-icons` (CC0): its path,
 *   rounded to 2 decimals (./path.mjs), its brand colour and its title.
 *   Pack id `si-<slug>`.
 * - A raster, `gh:<login>`, is that GitHub account's avatar, downloaded at
 *   192 px and written by `sharp` as a 96 px WebP. Pack id
 *   `gh-<login, lower case>`, file raster/<id>.webp.
 *
 * It fails loudly, and writes nothing, on a slug Simple Icons does not
 * have, a download that does not arrive, or a mapping entry it cannot
 * read; it fails after writing when the pack is over its 5 MB budget, so
 * the files are there to look at. The only files it deletes are rasters
 * an earlier run wrote -- the ones the pack.json it replaces names -- that
 * the mapping no longer names, and only inside raster/.
 */
import { mkdir, readFile, readdir, rm, stat, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import sharp from "sharp";
import * as simpleIcons from "simple-icons";
import { roundPath } from "./path.mjs";

const ROOT = fileURLToPath(new URL("../../", import.meta.url));
const MAPPING = join(ROOT, "scripts/tool-icons/mapping.json");
const PACK_DIR = join(ROOT, "src/assets/tool-icons");
const PACK = join(PACK_DIR, "pack.json");
const RASTER_DIR = join(PACK_DIR, "raster");

/**
 * The whole of src/assets/tool-icons/, 1000-based as Finder counts;
 * src/lib/toolIcons.test.ts holds the committed pack to it too.
 */
const BUDGET_BYTES = 5_000_000;
/** Decimals a glyph's path keeps. */
const PATH_DIGITS = 2;
/** The size the rounded path is compared at, and how much of one pixel's coverage it may change. */
const CHECK_PX = 64;
const CHECK_TOLERANCE = 64;
/** The size an avatar is asked for, and the size it is stored at. */
const AVATAR_PX = 192;
const RASTER_PX = 96;
const WEBP_QUALITY = 85;
const DOWNLOADS_AT_ONCE = 6;
const DOWNLOAD_ATTEMPTS = 3;

const SLUG = /^[a-z0-9_]+$/;
/** A GitHub login: letters, digits and single hyphens, which also makes `gh-<login>.webp` a safe file name. */
const LOGIN = /^[A-Za-z0-9](?:[A-Za-z0-9]|-(?=[A-Za-z0-9])){0,38}$/;
/** A raster this script writes, and so the only kind of file it may delete. */
const RASTER_FILE = /^gh-[a-z0-9-]+\.webp$/;

async function main() {
  const mapping = JSON.parse(await readFile(MAPPING, "utf8"));
  const { tools, sources, icons } = readMapping(mapping);
  const glyphs = await buildGlyphs(icons);
  const rasters = await buildRasters(icons);

  // Everything is in memory: nothing is written unless all of it arrived.
  const previous = await previousRasterFiles();
  await mkdir(RASTER_DIR, { recursive: true });
  for (const raster of rasters.values()) {
    await writeFile(join(RASTER_DIR, raster.file), raster.webp);
  }
  const current = new Set([...rasters.values()].map((raster) => raster.file));
  const deleted = [];
  for (const file of previous) {
    if (current.has(file)) continue;
    await rm(join(RASTER_DIR, file), { force: true });
    deleted.push(file);
  }
  const pack = {
    version: 1,
    generated: new Date().toISOString().slice(0, 10),
    glyphs: sorted(
      Object.fromEntries([...glyphs].map(([id, g]) => [id, { path: g.path, hex: g.hex, title: g.title }])),
    ),
    rasters: sorted(Object.fromEntries([...rasters].map(([id, r]) => [id, { file: r.file, title: r.title }]))),
    tools: sorted(tools),
    sources: sorted(sources),
  };
  await writeFile(PACK, `${JSON.stringify(pack, null, 2)}\n`);

  const unrounded = [...glyphs].filter(([, g]) => !g.rounded).map(([id]) => id);
  const packBytes = (await stat(PACK)).size;
  const rasterBytes = [...rasters.values()].reduce((sum, r) => sum + r.webp.length, 0);
  const total = await directoryBytes(PACK_DIR);
  console.log(
    `tool-icons: ${count(tools, "tool")} and ${count(sources, "source")}, ` +
      `drawn with ${count(glyphs, "glyph")} and ${count(rasters, "raster")}`,
  );
  if (unrounded.length > 0) {
    console.log(`  kept unrounded, rounding changed them at ${CHECK_PX} px: ${unrounded.join(", ")}`);
  }
  if (deleted.length > 0) {
    console.log(`  deleted, the mapping no longer names them: ${deleted.join(", ")}`);
  }
  console.log(
    `  src/assets/tool-icons: ${size(total)} of ${size(BUDGET_BYTES)} ` +
      `(pack.json ${size(packBytes)}, rasters ${size(rasterBytes)})`,
  );
  if (total > BUDGET_BYTES) {
    throw new Error(`the pack is ${size(total)}, over its ${size(BUDGET_BYTES)} budget`);
  }
}

/**
 * The mapping as the pack's `tools` and `sources` (key → icon id) and the
 * icons they name, by id. A tool entry is `{ icon, relation }`, with
 * `relation` "same" (the tool's own logo) or "maker" (its maker's); an
 * `icon` of null is a reviewed "no logo" and is left out. A source entry
 * is the icon itself, or null. Every problem is collected, then thrown at
 * once.
 */
function readMapping(mapping) {
  const problems = [];
  const icons = new Map();
  const tools = {};
  const sources = {};
  const isObject = (value) => typeof value === "object" && value !== null && !Array.isArray(value);
  if (!isObject(mapping) || !isObject(mapping.tools) || !isObject(mapping.sources)) {
    throw new Error(`${MAPPING} needs a "tools" and a "sources" object`);
  }
  const idOf = (ref, where) => {
    const match = typeof ref === "string" ? /^(si|gh):(.*)$/.exec(ref) : null;
    if (match === null) {
      problems.push(`${where}: icon must be "si:<slug>" or "gh:<login>", not ${JSON.stringify(ref)}`);
      return null;
    }
    const [, scheme, name] = match;
    if (scheme === "si" && !SLUG.test(name)) {
      problems.push(`${where}: "${name}" is not a Simple Icons slug`);
      return null;
    }
    if (scheme === "gh" && !LOGIN.test(name)) {
      problems.push(`${where}: "${name}" is not a GitHub login`);
      return null;
    }
    const id = scheme === "si" ? `si-${name}` : `gh-${name.toLowerCase()}`;
    if (!icons.has(id)) {
      icons.set(id, scheme === "si" ? { kind: "glyph", slug: name } : { kind: "raster", login: name });
    }
    return id;
  };
  for (const [key, entry] of Object.entries(mapping.tools)) {
    const where = `tools["${key}"]`;
    if (!isObject(entry)) {
      problems.push(`${where}: must be { "icon": ..., "relation": ... }`);
      continue;
    }
    if (entry.icon === null) continue;
    if (entry.relation !== "same" && entry.relation !== "maker") {
      problems.push(`${where}: relation must be "same" or "maker", not ${JSON.stringify(entry.relation)}`);
    }
    const id = idOf(entry.icon, where);
    if (id !== null) tools[key] = id;
  }
  for (const [adapterId, ref] of Object.entries(mapping.sources)) {
    if (ref === null) continue;
    const id = idOf(ref, `sources["${adapterId}"]`);
    if (id !== null) sources[adapterId] = id;
  }
  if (problems.length > 0) {
    throw new Error(`${MAPPING} has ${problems.length} problem(s):\n  ${problems.join("\n  ")}`);
  }
  return { tools, sources, icons };
}

/**
 * Every glyph the mapping names, by id: `{ path, hex, title, rounded }`.
 * The rounded path is drawn at 64 px next to the original; where any
 * pixel's coverage moves by more than a quarter the original is kept
 * (`rounded: false`). Of Simple Icons 16.32.0's 3461 logos only KNIME's
 * and THE FINALS' do, by one pixel each, on a sharp point.
 */
async function buildGlyphs(icons) {
  const bySlug = new Map(
    Object.values(simpleIcons)
      .filter((icon) => typeof icon === "object" && icon !== null && typeof icon.slug === "string")
      .map((icon) => [icon.slug, icon]),
  );
  const wanted = [...icons].filter(([, icon]) => icon.kind === "glyph");
  const unknown = wanted.filter(([, icon]) => !bySlug.has(icon.slug)).map(([, icon]) => icon.slug);
  if (unknown.length > 0) {
    throw new Error(`simple-icons has no icon with the slug: ${unknown.join(", ")}`);
  }
  const glyphs = new Map();
  for (const [id, { slug }] of wanted) {
    const icon = bySlug.get(slug);
    const rounded = roundPath(icon.path, PATH_DIGITS);
    const same = await drawsTheSame(icon.path, rounded);
    glyphs.set(id, { path: same ? rounded : icon.path, hex: icon.hex, title: icon.title, rounded: same });
  }
  return glyphs;
}

/** Whether path data `a` and `b` cover every pixel of a 64 px render within `CHECK_TOLERANCE` of 255. */
async function drawsTheSame(a, b) {
  const [left, right] = await Promise.all([coverage(a), coverage(b)]);
  return left.every((value, i) => Math.abs(value - right[i]) <= CHECK_TOLERANCE);
}

function coverage(d) {
  const svg =
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="${CHECK_PX}" height="${CHECK_PX}">` +
    `<path d="${d}"/></svg>`;
  return sharp(Buffer.from(svg)).ensureAlpha().extractChannel(3).raw().toBuffer();
}

/**
 * Every raster the mapping names, by id: `{ file, title, webp }`.
 * Downloads a few at a time; any that fails fails the build.
 */
async function buildRasters(icons) {
  const wanted = [...icons].filter(([, icon]) => icon.kind === "raster");
  const rasters = new Map();
  const failures = [];
  let next = 0;
  const worker = async () => {
    while (next < wanted.length) {
      const [id, { login }] = wanted[next++];
      try {
        const webp = await toWebp(await download(login));
        rasters.set(id, { file: `${id}.webp`, title: login, webp });
      } catch (error) {
        failures.push(`${login}: ${error.message}`);
      }
    }
  };
  await Promise.all(Array.from({ length: DOWNLOADS_AT_ONCE }, worker));
  if (failures.length > 0) {
    throw new Error(`${failures.length} avatar(s) did not download:\n  ${failures.join("\n  ")}`);
  }
  return rasters;
}

/**
 * `login`'s GitHub avatar, as GitHub sends it. Tries again after a
 * network error or a 5xx; a 404, no such account, is final.
 */
async function download(login) {
  const url = `https://github.com/${login}.png?size=${AVATAR_PX}`;
  let lastError = null;
  for (let attempt = 1; attempt <= DOWNLOAD_ATTEMPTS; attempt++) {
    try {
      const response = await fetch(url, {
        headers: { "user-agent": "canager-icons-build" },
        signal: AbortSignal.timeout(30_000),
      });
      if (response.status === 404) {
        throw Object.assign(new Error(`${url}: 404, no such GitHub account`), { final: true });
      }
      if (!response.ok) throw new Error(`${url}: HTTP ${response.status}`);
      const type = response.headers.get("content-type") ?? "";
      if (!type.startsWith("image/")) throw new Error(`${url}: not an image (${type || "no content type"})`);
      return Buffer.from(await response.arrayBuffer());
    } catch (error) {
      if (error.final) throw error;
      lastError = error;
      if (attempt < DOWNLOAD_ATTEMPTS) await new Promise((resolve) => setTimeout(resolve, 1000 * attempt));
    }
  }
  throw lastError;
}

/** `image` as a `RASTER_PX` square WebP, its transparency kept: the app puts it on a white tile. */
function toWebp(image) {
  return sharp(image)
    .resize(RASTER_PX, RASTER_PX, { fit: "contain", background: { r: 0, g: 0, b: 0, alpha: 0 } })
    .webp({ quality: WEBP_QUALITY, effort: 6 })
    .toBuffer();
}

/**
 * The raster files the current pack.json names: the ones an earlier run
 * wrote, and so the only ones it may delete. None when there is no pack
 * yet, or one it cannot read -- then nothing is deleted.
 */
async function previousRasterFiles() {
  let pack;
  try {
    pack = JSON.parse(await readFile(PACK, "utf8"));
  } catch (error) {
    if (error.code !== "ENOENT") {
      console.warn(`  could not read the old ${PACK}, so no raster is deleted: ${error.message}`);
    }
    return [];
  }
  const rasters = typeof pack?.rasters === "object" && pack.rasters !== null ? Object.values(pack.rasters) : [];
  return rasters.map((raster) => raster?.file).filter((file) => typeof file === "string" && RASTER_FILE.test(file));
}

/** Every file under `dir` but Finder's .DS_Store, which git ignores and the app never ships. */
async function directoryBytes(dir) {
  let total = 0;
  for (const entry of await readdir(dir, { withFileTypes: true })) {
    if (entry.name === ".DS_Store") continue;
    const path = join(dir, entry.name);
    total += entry.isDirectory() ? await directoryBytes(path) : (await stat(path)).size;
  }
  return total;
}

function sorted(object) {
  return Object.fromEntries(Object.entries(object).sort(([a], [b]) => (a < b ? -1 : 1)));
}

function count(collection, noun) {
  const n = collection instanceof Map ? collection.size : Object.keys(collection).length;
  return `${n} ${noun}${n === 1 ? "" : "s"}`;
}

/** Bytes as Finder counts them, 1000-based, like `formatBytes` in src/lib/format.ts. */
function size(bytes) {
  if (bytes >= 1_000_000) return `${(bytes / 1_000_000).toFixed(2)} MB`;
  if (bytes >= 1_000) return `${(bytes / 1_000).toFixed(1)} KB`;
  return `${bytes} B`;
}

main().catch((error) => {
  console.error(`icons:build failed: ${error.message}`);
  process.exitCode = 1;
});
