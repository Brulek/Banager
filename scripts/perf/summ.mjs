// node scripts/perf/summ.mjs results.json [results2.json ...]
// What bench-huge.mjs measured, per step and build (its `--bases` label): the
// median of each number over the runs, the range in brackets, and how many
// runs. Reads the result files only.
import { readFileSync } from "node:fs";
const rows = process.argv.slice(2).flatMap((f) => JSON.parse(readFileSync(f, "utf8")));
const FIELDS = ["interactionMs", "maxTask", "tasksOver50", "tasksOver16", "busyMs", "readyAfterMs", "allRowsMs", "allStartedMs", "opLists", "opClones"];
const median = (v) => {
  const s = [...v].sort((a, b) => a - b);
  const m = Math.floor(s.length / 2);
  return s.length % 2 ? s[m] : (s[m - 1] + s[m]) / 2;
};
const by = new Map();
for (const r of rows) {
  const k = `${r.step}\t${r.base}`;
  if (!by.has(k)) by.set(k, []);
  by.get(k).push(r);
}
for (const [k, rs] of [...by.entries()].sort()) {
  const cells = [];
  for (const f of FIELDS) {
    const v = rs.map((r) => r[f]).filter((x) => typeof x === "number");
    if (!v.length) continue;
    const lo = Math.min(...v);
    const hi = Math.max(...v);
    cells.push(`${f}=${median(v)} [${lo}–${hi}]`);
  }
  console.log(`${k}\tn=${rs.length}\t${cells.join("  ")}`);
}
