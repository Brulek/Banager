import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { buildWorld } from "./mockData";
import { appIcon } from "./mockIcons";

const doc = readFileSync(
  path.join(path.dirname(fileURLToPath(import.meta.url)), "../../docs/ui-preview.md"),
  "utf8",
).replace(/\s+/g, " ");

/**
 * BNG-003: docs/ui-preview.md said the default pretend Mac had 26
 * formulae and 4 casks, and a Homebrew heading of 33, while the world had
 * 28, 7 and 35. The numbers the page states are read here against
 * `buildWorld("full")`, so the two cannot drift apart again.
 */
describe("what docs/ui-preview.md says the pretend Mac has", () => {
  const world = buildWorld("full");
  const brew = world.artifacts.filter((a) => a.key.instance_id === "brew:/opt/homebrew");
  const formulae = brew.filter((a) => a.key.kind === "Formula");
  const casks = brew.filter((a) => a.key.kind === "Cask");

  it("counts Homebrew's formulae, its folded components and its casks as the world has them", () => {
    const said = doc.match(/\*\*Homebrew\*\* \(`\/opt\/homebrew`\): (\d+) formulae, (\d+) of them folded away as dependencies, and (\d+) casks/);
    expect(said).not.toBeNull();
    expect(said!.slice(1).map(Number)).toEqual([
      formulae.length,
      formulae.filter((a) => a.reason === "Dependency").length,
      casks.length,
    ]);
  });

  it("names the casks that are apps, as many as get an icon", () => {
    const said = doc.match(/(\w+) of the casks are apps/);
    const words = ["Zero", "One", "Two", "Three", "Four", "Five", "Six", "Seven"];
    expect(said?.[1]).toBe(words[casks.filter((a) => appIcon(a) !== null).length]);
  });

  it("gives Homebrew's heading and the whole list the world's counts", () => {
    expect(doc).toContain(`「Homebrew · ${brew.length}个 ·`);
    expect(doc).toContain(`「${world.artifacts.length}个工具 ·`);
  });

  it("counts npm's, pipx's and uv's rows and updates as the world has them", () => {
    const of = (id: string) => world.artifacts.filter((a) => a.key.instance_id === id).length;
    const updates = (id: string) => world.updates.filter((u) => u.key.instance_id === id);
    const free = (id: string) => updates(id).filter((u) => u.blocked === null).length;
    const said = doc.match(
      /\*\*npm\*\*: (\d+) global packages, (\d+) updates\. \*\*pipx\*\*: (\d+) tools, (\d+) updates and a pinned one \(poetry\)\. \*\*uv\*\*: (\d+) tools/,
    );
    expect(said).not.toBeNull();
    expect(said!.slice(1).map(Number)).toEqual([
      of("npm:/opt/homebrew"),
      free("npm:/opt/homebrew"),
      of("pipx"),
      free("pipx"),
      of("uv"),
    ]);
    expect(updates("pipx").filter((u) => u.blocked === "Pinned").map((u) => u.key.name)).toEqual(["poetry"]);
    expect(updates("uv")).toHaveLength(1);
  });
});
