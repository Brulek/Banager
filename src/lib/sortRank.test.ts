import { describe, expect, it } from "vitest";
import { cachedRankedComparator, rankedComparator } from "./sortRank";

interface Row {
  name: string;
  n: number;
}

const collator = new Intl.Collator("en", { numeric: true, sensitivity: "base" });
const byName = (a: Row, b: Row) => collator.compare(a.name, b.name);

function rows(names: string[]): Row[] {
  return names.map((name, n) => ({ name, n }));
}

describe("rankedComparator", () => {
  it("sorts any part of the list as the comparison itself does", () => {
    const all = rows(["node@9", "Node@22", "git", "ffmpeg", "Git", "zsh", "écho", "echo", "a10", "a9"]);
    const ranked = rankedComparator(all, byName);
    for (const part of [all, all.slice(2), all.filter((_, i) => i % 2 === 0), [...all].reverse()]) {
      expect([...part].sort(ranked)).toEqual([...part].sort(byName));
    }
  });

  it("gives items the comparison calls equal one place, so a sort keeps the order it was handed them", () => {
    const all = rows(["git", "Git", "GIT", "abc"]);
    const ranked = rankedComparator(all, byName);
    expect(ranked(all[0], all[1])).toBe(0);
    expect(ranked(all[2], all[0])).toBe(0);
    const reversed = [all[2], all[1], all[0], all[3]];
    expect([...reversed].sort(ranked).map((r) => r.n)).toEqual([3, 2, 1, 0]);
  });

  it("compares an item the list does not have with the comparison", () => {
    const all = rows(["b", "c"]);
    const ranked = rankedComparator(all, byName);
    const stranger = { name: "a", n: 9 };
    expect([all[1], stranger, all[0]].sort(ranked).map((r) => r.name)).toEqual(["a", "b", "c"]);
  });

  it("is worked out once per list and key", () => {
    const all = rows(["b", "a"]);
    let calls = 0;
    const counting = (a: Row, b: Row) => {
      calls += 1;
      return byName(a, b);
    };
    const first = cachedRankedComparator(all, "en", counting);
    const made = calls;
    expect(cachedRankedComparator(all, "en", counting)).toBe(first);
    expect(calls).toBe(made);
    expect(cachedRankedComparator(all, "zh-CN", counting)).not.toBe(first);
    expect(cachedRankedComparator([...all], "zh-CN", counting)).not.toBe(first);
  });
});
