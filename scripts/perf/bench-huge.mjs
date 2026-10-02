// Times the browser preview (docs/ui-preview.md, "Large list") in headless
// Chrome through the DevTools protocol: one scenario over Installed, Updates
// and the Overview of `?state=huge` (or `--state many`), the same script for
// every build so builds are compared like for like, their runs interleaved
// (A B, B A, ...) to share the machine's noise. It runs only this Mac's
// Google Chrome and node's own modules, against a preview served locally;
// the preview's backend is the mock (src/dev/), so nothing here touches
// Homebrew or any other tool. The numbers under "About 5,000 tools" in
// docs/ui-preview.md come from it (track p3 used an earlier version of it).
//
// Build and serve each build to compare (from the repository's root):
//   pnpm exec vite build --mode mock --outDir target/mock-a
//   pnpm exec vite preview --mode mock --outDir target/mock-a --port 1546
// then
//   node scripts/perf/bench-huge.mjs --bases a=http://localhost:1546/,b=http://localhost:1547/ \
//     --runs 8 --width 1280 --height 800 [--throttle 4] [--only update-all-sheet,update-all-submit] \
//     [--profile <step>] [--out target/perf/results.json] [--port 9861]
//   node scripts/perf/summ.mjs target/perf/results.json
// Results, CPU profiles and screenshots of a failed step go to target/perf/
// (or `--out`'s folder); Chrome's profile to the system's temporary folder.
//
// What it measures per step: Event Timing interactions, the main thread's
// tasks from a trace (longest, how many over 50 and 16 ms, busy time),
// frames while scrolling, and for Update All when its Update comes on, when
// every tool of its list is drawn, and how many `list_operations` the
// preview answered while all its updates started (counted as the preview's
// backend copies operation 1 for each answer: `opLists`; `opClones` all the
// operations copied). `--only` still runs the other steps, unmeasured, so
// the page is where the measured ones expect it. A step set by a popup is
// timed from the change to the next frame (`toFrameMs`).
import { spawn } from "node:child_process";
import { mkdirSync, rmSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const argv = process.argv.slice(2);
const args = {};
for (let i = 0; i < argv.length; i += 1) {
  if (!argv[i].startsWith("--")) continue;
  const next = argv[i + 1];
  args[argv[i].slice(2)] = next === undefined || next.startsWith("--") ? "1" : next;
}
const HERE = path.dirname(fileURLToPath(import.meta.url));
const PORT = Number(args.port ?? 9861); // outside cdp.mjs's random 9400-9800
const DEVTOOLS = `http://127.0.0.1:${PORT}`;
const BASES = (args.bases ?? "polish3=http://localhost:1460/").split(",").map((s) => {
  const [label, ...rest] = s.split("=");
  return { label, url: rest.join("=") };
});
const RUNS = Number(args.runs ?? 3);
const WIDTH = Number(args.width ?? 960);
const HEIGHT = Number(args.height ?? 640);
const THROTTLE = Number(args.throttle ?? 1);
const SPEED = Number(args.speed ?? 6000);
const LANG = args.lang ?? "zh-CN";
const PROFILE = new Set((args.profile ?? "").split(",").filter(Boolean));
const ONLY = new Set((args.only ?? "").split(",").filter(Boolean));
const STATE = args.state ?? "huge";
const OUT = path.resolve(args.out ?? path.join(HERE, "..", "..", "target", "perf", `results-${STATE}-${WIDTH}x${HEIGHT}-t${THROTTLE}.json`));
const OUT_DIR = path.dirname(OUT);
mkdirSync(OUT_DIR, { recursive: true });
const CH = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

class CDP {
  constructor(url) {
    this.ws = new WebSocket(url);
    this.nextId = 1;
    this.pending = new Map();
    this.listeners = new Map();
    this.ws.onmessage = (msg) => {
      const data = JSON.parse(msg.data);
      if (data.id !== undefined) {
        const p = this.pending.get(data.id);
        this.pending.delete(data.id);
        if (data.error) p.reject(new Error(`${p.method}: ${data.error.message}`));
        else p.resolve(data.result);
      } else if (data.method) {
        for (const fn of this.listeners.get(data.method) ?? []) fn(data.params);
      }
    };
  }
  open() {
    return new Promise((resolve, reject) => {
      this.ws.onopen = resolve;
      this.ws.onerror = reject;
    });
  }
  send(method, params = {}) {
    const id = this.nextId++;
    this.ws.send(JSON.stringify({ id, method, params }));
    return new Promise((resolve, reject) => this.pending.set(id, { resolve, reject, method }));
  }
  on(method, fn) {
    if (!this.listeners.has(method)) this.listeners.set(method, []);
    this.listeners.get(method).push(fn);
  }
  once(method) {
    return new Promise((resolve) => {
      const fn = (p) => {
        this.listeners.set(method, (this.listeners.get(method) ?? []).filter((f) => f !== fn));
        resolve(p);
      };
      this.on(method, fn);
    });
  }
  close() {
    this.ws.close();
  }
}

// Installed in every document before any of its scripts.
const INSTRUMENT = `
(() => {
  const perf = (window.__perf = { events: [], marks: {}, frames: null, opLists: 0, opClones: 0 });
  const stringify = JSON.stringify;
  JSON.stringify = function (value, ...rest) {
    if (value !== null && typeof value === "object" && "argv_preview" in value && "cancel_policy" in value) {
      perf.opClones += 1;
      if (value.id === 1) perf.opLists += 1;
    }
    return stringify.call(this, value, ...rest);
  };
  try {
    new PerformanceObserver((list) => {
      for (const e of list.getEntries()) perf.events.push({ name: e.name, start: e.startTime, dur: e.duration, ps: e.processingStart, pe: e.processingEnd, id: e.interactionId });
    }).observe({ type: "event", buffered: true, durationThreshold: 16 });
  } catch (e) { console.warn("event observer", e); }
  window.__startFrames = () => { perf.frames = []; const tick = (t) => { if (perf.frames === null) return; perf.frames.push(t); requestAnimationFrame(tick); }; requestAnimationFrame(tick); return true; };
  window.__stopFrames = () => { const f = perf.frames || []; perf.frames = null; const gaps = []; for (let i = 1; i < f.length; i++) gaps.push(f[i] - f[i - 1]); return gaps; };
})();
`;

let chrome;
async function launchChrome() {
  const ud = path.join(os.tmpdir(), `banager-bench-chrome-${PORT}`);
  rmSync(ud, { recursive: true, force: true });
  mkdirSync(ud, { recursive: true });
  chrome = spawn(
    CH,
    [
      "--headless=new",
      `--remote-debugging-port=${PORT}`,
      `--user-data-dir=${ud}`,
      "--disable-background-timer-throttling",
      "--disable-renderer-backgrounding",
      "--disable-backgrounding-occluded-windows",
      "--no-first-run",
      "--hide-scrollbars",
      `--window-size=${WIDTH},${HEIGHT}`,
      "about:blank",
    ],
    { stdio: "ignore" },
  );
  for (let i = 0; i < 100; i += 1) {
    try {
      await fetch(`${DEVTOOLS}/json/version`);
      return ud;
    } catch {
      await sleep(100);
    }
  }
  throw new Error("Chrome did not start");
}

async function newPage() {
  const res = await fetch(`${DEVTOOLS}/json/new?about:blank`, { method: "PUT" });
  const target = await res.json();
  const cdp = new CDP(target.webSocketDebuggerUrl);
  await cdp.open();
  cdp.targetId = target.id;
  await cdp.send("Page.enable");
  await cdp.send("Runtime.enable");
  await cdp.send("Emulation.setDeviceMetricsOverride", { width: WIDTH, height: HEIGHT, deviceScaleFactor: 2, mobile: false });
  await cdp.send("Emulation.setFocusEmulationEnabled", { enabled: true });
  await cdp.send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-color-scheme", value: "light" }] });
  if (THROTTLE > 1) await cdp.send("Emulation.setCPUThrottlingRate", { rate: THROTTLE });
  await cdp.send("Page.addScriptToEvaluateOnNewDocument", { source: INSTRUMENT });
  cdp.on("Runtime.exceptionThrown", (p) =>
    console.error("page exception:", p.exceptionDetails?.exception?.description ?? p.exceptionDetails?.text),
  );
  return cdp;
}

async function closePage(cdp) {
  cdp.close();
  await fetch(`${DEVTOOLS}/json/close/${cdp.targetId}`);
}

async function evaluate(cdp, expression) {
  const r = await cdp.send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true });
  if (r.exceptionDetails)
    throw new Error(`evaluate failed: ${r.exceptionDetails.exception?.description ?? r.exceptionDetails.text}\n${expression}`);
  return r.result.value;
}

async function waitFor(cdp, expression, timeout = 20000, label = expression) {
  const start = Date.now();
  while (Date.now() - start < timeout) {
    if (await evaluate(cdp, expression)) return Date.now() - start;
    await sleep(20);
  }
  throw new Error(`timed out waiting for ${label}`);
}

const NAV = (texts) => `(() => {
  const want = ${JSON.stringify(texts)};
  return [...document.querySelectorAll("nav button")].find((b) => [...b.querySelectorAll("span")].some((s) => want.includes(s.textContent.trim())) || want.includes(b.textContent.trim()));
})()`;
const BUTTON_STARTS = (texts) => `(() => {
  const want = ${JSON.stringify(texts)};
  return [...document.querySelectorAll("button")].find((b) => { const t = b.textContent.trim().replace(/\\s+/g, " "); return want.some((w) => t.startsWith(w)); });
})()`;

async function click(cdp, elementExpr) {
  const r = await evaluate(
    cdp,
    `(() => { const el = ${elementExpr}; if (!el) return null; const r = el.getBoundingClientRect(); return { x: r.left + r.width / 2, y: r.top + r.height / 2 }; })()`,
  );
  if (!r) {
    const info = await evaluate(cdp, `JSON.stringify({ slots: document.querySelectorAll("[data-list-slot]").length, rows: document.querySelectorAll("[data-tool-row]").length, st: document.querySelector("[data-list]")?.scrollTop, first: document.querySelector("[data-list-slot]")?.dataset.index, text: document.body.innerText.slice(0, 200) })`);
    const shot = await cdp.send("Page.captureScreenshot", { format: "png" });
    writeFileSync(path.join(OUT_DIR, `fail-${current.label}-r${current.run}.png`), Buffer.from(shot.data, "base64"));
    throw new Error(`no element for ${elementExpr}: ${info}`);
  }
  await cdp.send("Input.dispatchMouseEvent", { type: "mouseMoved", x: r.x, y: r.y });
  await cdp.send("Input.dispatchMouseEvent", { type: "mousePressed", x: r.x, y: r.y, button: "left", clickCount: 1 });
  await cdp.send("Input.dispatchMouseEvent", { type: "mouseReleased", x: r.x, y: r.y, button: "left", clickCount: 1 });
}

async function key(cdp, keyName, { text, code, vk, modifiers = 0, commands } = {}) {
  await cdp.send("Input.dispatchKeyEvent", {
    type: "keyDown",
    key: keyName,
    code,
    windowsVirtualKeyCode: vk,
    modifiers,
    ...(text ? { text, unmodifiedText: text } : {}),
    ...(commands ? { commands } : {}),
  });
  await cdp.send("Input.dispatchKeyEvent", { type: "keyUp", key: keyName, code, windowsVirtualKeyCode: vk, modifiers });
}
const typeChar = (cdp, ch) => key(cdp, ch, { text: ch, code: `Key${ch.toUpperCase()}`, vk: ch.toUpperCase().charCodeAt(0) });

// ------------------------------------------------------------------ tracing

async function readStream(cdp, handle) {
  const chunks = [];
  for (;;) {
    const r = await cdp.send("IO.read", { handle, size: 4 << 20 });
    chunks.push(r.base64Encoded ? Buffer.from(r.data, "base64").toString("utf-8") : r.data);
    if (r.eof) break;
  }
  await cdp.send("IO.close", { handle });
  return chunks.join("");
}

function mainThreadTasks(events, host) {
  let pid = null;
  for (const e of events) {
    if (e.name === "TracingStartedInBrowser" && e.args?.data?.frames) {
      const frame = e.args.data.frames.find((f) => (f.url ?? "").includes(host));
      if (frame) pid = frame.processId;
    }
    if (pid === null && e.name === "CommitLoad" && (e.args?.data?.url ?? "").includes(host)) pid = e.pid;
  }
  const threads = events.filter((e) => e.ph === "M" && e.name === "thread_name" && e.args?.name === "CrRendererMain");
  const main = threads.find((t) => pid === null || t.pid === pid) ?? threads[0];
  if (!main) return { error: "no renderer main thread" };
  const tasks = events
    .filter((e) => e.ph === "X" && e.pid === main.pid && e.tid === main.tid && /RunTask$/.test(e.name))
    .sort((a, b) => a.ts - b.ts);
  const top = [];
  for (const t of tasks) {
    const last = top[top.length - 1];
    if (last && t.ts < last.ts + last.dur) continue;
    top.push(t);
  }
  return { tasks: top.map((t) => ({ ts: t.ts / 1000, dur: t.dur / 1000 })) };
}

function taskStats(tasks) {
  const d = tasks.map((t) => t.dur);
  const round = (x) => Math.round(x * 10) / 10;
  return {
    maxTask: round(Math.max(0, ...d)),
    tasksOver50: d.filter((x) => x > 50).length,
    tasksOver16: d.filter((x) => x > 16).length,
    busyMs: Math.round(d.filter((x) => x > 1).reduce((a, b) => a + b, 0)),
  };
}

let current = null; // { label, run, host, results }

async function step(cdp, name, fn, { after = 500 } = {}) {
  // --only: the other steps still run, unmeasured, so the page is where the
  // measured ones expect it; a scroll, which only measures, is left out.
  if (ONLY.size && !ONLY.has(name)) {
    if (!name.startsWith("scroll-")) await fn();
    await sleep(after);
    return;
  }
  const t0 = await evaluate(cdp, `(() => { window.__perf.events.length = 0; window.__perf.opLists = 0; window.__perf.opClones = 0; if (window.__invokes) window.__invokes.length = 0; return performance.now(); })()`);
  await cdp.send("Tracing.start", {
    transferMode: "ReturnAsStream",
    traceConfig: {
      recordMode: "recordAsMuchAsPossible",
      includedCategories: ["toplevel", "devtools.timeline", "disabled-by-default-devtools.timeline", "__metadata"],
    },
  });
  const profiling = PROFILE.has(name);
  if (profiling) {
    await cdp.send("Profiler.enable");
    await cdp.send("Profiler.setSamplingInterval", { interval: 100 });
    await cdp.send("Profiler.start");
  }
  const extra = (await fn()) ?? {};
  await sleep(after);
  if (profiling) {
    const { profile } = await cdp.send("Profiler.stop");
    writeFileSync(path.join(OUT_DIR, `profile-${current.label}-${name}-r${current.run}.cpuprofile`), JSON.stringify(profile));
  }
  const complete = cdp.once("Tracing.tracingComplete");
  await cdp.send("Tracing.end");
  const { stream } = await complete;
  const trace = JSON.parse(await readStream(cdp, stream));
  const events = Array.isArray(trace) ? trace : trace.traceEvents;
  const { tasks, error } = mainThreadTasks(events, current.host);
  if (profiling && tasks) {
    // What the long tasks spent their time on, by trace event name (not exclusive).
    const long = tasks.filter((t) => t.dur > 30);
    const main = events.find((e) => e.ph === "M" && e.name === "thread_name" && e.args?.name === "CrRendererMain");
    const sums = {};
    for (const e of events) {
      if (e.ph !== "X" || !main || e.pid !== main.pid || e.tid !== main.tid) continue;
      const ts = e.ts / 1000;
      if (!long.some((t) => ts >= t.ts && ts < t.ts + t.dur)) continue;
      sums[e.name] = (sums[e.name] ?? 0) + e.dur / 1000;
    }
    const top = Object.entries(sums).sort((a, b) => b[1] - a[1]).slice(0, 14).map(([k, v]) => `${k}:${v.toFixed(1)}`);
    // How many elements each style recalculation over 5 ms touched (p5).
    const recalcs = events
      .filter((e) => e.ph === "X" && main && e.pid === main.pid && e.tid === main.tid && e.name === "UpdateLayoutTree" && e.dur > 5000)
      .map((e) => `${(e.dur / 1000).toFixed(0)}ms/${e.args?.elementCount ?? "?"}`);
    console.log(`  style recalcs over 5 ms (${name}): ${recalcs.join(" ")}`);
    console.log(`  breakdown ${name}: long=${long.map((t) => t.dur.toFixed(0)).join(",")} ${top.join(" ")}`);
  }
  const perfEvents = await evaluate(cdp, `JSON.parse(JSON.stringify(window.__perf.events))`);
  const interactions = new Map();
  for (const e of perfEvents) {
    if (!e.id || e.start < t0) continue;
    const cur = interactions.get(e.id);
    if (!cur || e.dur > cur.dur) interactions.set(e.id, e);
  }
  const durs = [...interactions.values()].map((e) => Math.round(e.dur));
  const invoked = await evaluate(cdp, `(() => { const c = {}; for (const n of window.__invokes ?? []) c[n] = (c[n] ?? 0) + 1; return Object.entries(c).map(([k, v]) => k + ":" + v).join(" "); })()`);
  const ops = await evaluate(cdp, `({ opLists: window.__perf.opLists, opClones: window.__perf.opClones })`);
  const result = {
    base: current.label,
    run: current.run,
    step: name,
    interactionMs: durs.length ? Math.max(...durs) : "<16",
    interactions: [...interactions.values()].map((e) => `${e.name}:${Math.round(e.dur)}`).join(" "),
    ...(invoked ? { invoked } : {}),
    ...(ops.opClones > 0 ? ops : {}),
    ...(error ? { error } : taskStats(tasks)),
    ...extra,
  };
  current.results.push(result);
  console.log(JSON.stringify(result));
  return result;
}

// -------------------------------------------------------------- the scenario

const SLOTS = `document.querySelectorAll("[data-list-slot]").length`;
const HAS_ROWS = `(${SLOTS} > 5)`;
// The list's scroller: the first slot's nearest ancestor that scrolls.
const LIST = `(() => { let el = document.querySelector("[data-list-slot]"); while (el && el !== document.body) { const o = getComputedStyle(el).overflowY; if ((o === "auto" || o === "scroll") && el.scrollHeight > el.clientHeight) return el; el = el.parentElement; } return null; })()`;
const DIALOG = `document.querySelector('[role="dialog"], dialog[open]')`;

async function scrollToEnd(cdp) {
  const box = await evaluate(
    cdp,
    `(() => { const l = ${LIST}; l.scrollTop = 0; const r = l.getBoundingClientRect(); return { x: r.left + r.width / 2, y: r.top + r.height / 2, scrollHeight: l.scrollHeight, clientHeight: l.clientHeight }; })()`,
  );
  await sleep(400);
  await evaluate(cdp, `window.__startFrames()`);
  const t0 = Date.now();
  await cdp.send("Input.synthesizeScrollGesture", {
    x: box.x,
    y: box.y,
    yDistance: -(box.scrollHeight - box.clientHeight + 200),
    speed: SPEED,
    gestureSourceType: "mouse",
    preventFling: true,
  });
  const scrollMs = Date.now() - t0;
  const gaps = await evaluate(cdp, `window.__stopFrames()`);
  const atEnd = await evaluate(cdp, `(() => { const l = ${LIST}; if (!l) return "no-list slots=" + document.querySelectorAll("[data-list-slot]").length + " rows=" + document.querySelectorAll("[data-tool-row]").length + " body=" + document.body.innerText.slice(0, 80).replace(/\\s+/g, " "); return Math.round(l.scrollTop + l.clientHeight) >= l.scrollHeight - 2; })()`);
  const sorted = [...gaps].sort((a, b) => a - b);
  return {
    scrollMs,
    atEnd,
    scrollHeight: box.scrollHeight,
    frames: gaps.length,
    maxFrameMs: Math.round(sorted[sorted.length - 1] ?? 0),
    p95FrameMs: Math.round(sorted[Math.floor(sorted.length * 0.95)] ?? 0),
    framesOver50: gaps.filter((g) => g > 50).length,
  };
}

async function scrollTop0(cdp, where) {
  const ok = await evaluate(cdp, `(() => { const l = ${LIST}; if (!l) return false; l.scrollTop = 0; return true; })()`);
  if (ok) return;
  const info = await evaluate(cdp, `JSON.stringify({ slots: document.querySelectorAll("[data-list-slot]").length, rows: document.querySelectorAll("[data-tool-row]").length, url: location.href, text: document.body.innerText.slice(0, 300), focus: document.activeElement && document.activeElement.tagName })`);
  const shot = await cdp.send("Page.captureScreenshot", { format: "png" });
  writeFileSync(path.join(OUT_DIR, `fail-${current.label}-r${current.run}-${where}.png`), Buffer.from(shot.data, "base64"));
  throw new Error(`no list at ${where}: ${info}`);
}

// Runs `action` (a function's source) in the page and resolves with the
// time to the frame after it: rAF, then a task after that frame's paint.
const TIMED = (action) => `new Promise((resolve) => {
  const t0 = performance.now();
  (${action})();
  requestAnimationFrame(() => setTimeout(() => resolve(Math.round(performance.now() - t0)), 0));
})`;
const SELECT = (label, value) =>
  TIMED(`() => { const s = document.querySelector('select[aria-label="${label}"]'); if (!s) throw new Error("no select ${label}"); s.value = "${value}"; s.dispatchEvent(new Event("change", { bubbles: true })); }`);
const ROWS = `document.querySelectorAll("[data-tool-row]").length`;

async function chooseStep(cdp, name, label, value, after = 500) {
  await step(
    cdp,
    name,
    async () => {
      const toFrameMs = await evaluate(cdp, SELECT(label, value));
      return { toFrameMs, rows: await evaluate(cdp, ROWS), slots: await evaluate(cdp, SLOTS) };
    },
    { after },
  );
}

async function closeDialog(cdp) {
  await key(cdp, "Escape", { code: "Escape", vk: 27 });
  await sleep(500);
  if (await evaluate(cdp, `${DIALOG} !== null`)) {
    await key(cdp, "Escape", { code: "Escape", vk: 27 });
    await sleep(500);
  }
}

async function scenario(base) {
  const q = (page) => `${base.url}?state=${STATE}&page=${page}&lang=${LANG}`;
  const cdp = await newPage();
  await cdp.send("Page.navigate", { url: q("overview") });
  await waitFor(cdp, `${NAV(["已安装", "Installed"])} !== undefined`, 30000);
  await sleep(STATE === "huge" ? 6000 : 3500); // the startup refresh (900 ms), the icons, the tables, the sizes.

  await step(cdp, "nav-installed", async () => {
    await click(cdp, NAV(["已安装", "Installed"]));
    return { seenAfterMs: await waitFor(cdp, HAS_ROWS) };
  });
  await sleep(1500);

  await click(cdp, `document.querySelector('input[type="search"]')`);
  await sleep(300);
  await step(cdp, "search-p", async () => {
    await typeChar(cdp, "p");
  });
  await step(cdp, "search-py", async () => {
    await typeChar(cdp, "y");
  });
  const matched = await evaluate(cdp, SLOTS);
  await step(cdp, "search-clear", async () => {
    await key(cdp, "a", { code: "KeyA", vk: 65, modifiers: 4, commands: ["selectAll"] });
    await key(cdp, "Backspace", { code: "Backspace", vk: 8 });
  });
  const query = await evaluate(cdp, `document.querySelector('input[type="search"]').value`);
  if (query !== "") throw new Error(`search not cleared: ${JSON.stringify(query)}`);
  const last = current.results.filter((r) => r.step === "search-py").at(-1);
  if (last) last.slotsForPy = matched;
  await sleep(600);
  await evaluate(cdp, `document.activeElement && document.activeElement.blur(), true`);

  await step(cdp, "scroll-installed", () => scrollToEnd(cdp), { after: 300 });

  await scrollTop0(cdp, "before-open-inspector");
  await sleep(600);
  await step(cdp, "open-inspector", async () => {
    await click(cdp, `document.querySelectorAll("[data-tool-row]")[2]`);
    await sleep(700);
    return { inspector: await evaluate(cdp, `!!document.querySelector("[data-inspector-content], [data-inspector-scroll]")`) };
  });
  await sleep(500);
  await step(cdp, "scroll-installed-inspector", () => scrollToEnd(cdp), { after: 300 });
  await scrollTop0(cdp, "after-inspector");
  await sleep(500);

  // The 「显示」 popup's choices, then back to every tool.
  await chooseStep(cdp, "show-ai", "显示", "ai");
  await chooseStep(cdp, "show-notOnPath", "显示", "notOnPath");
  await chooseStep(cdp, "show-twins", "显示", "twins");
  await chooseStep(cdp, "show-all", "显示", "all");
  // The sorts, then back to by name.
  await chooseStep(cdp, "sort-size", "排序方式", "size");
  await step(cdp, "scroll-installed-size", () => scrollToEnd(cdp), { after: 300 });
  await scrollTop0(cdp, "after-size");
  await chooseStep(cdp, "sort-date", "排序方式", "date");
  await chooseStep(cdp, "sort-name", "排序方式", "name");
  await sleep(500);

  // Tick 20 rows, each a click of its own; the step's interaction is the slowest.
  await step(
    cdp,
    "tick-20",
    async () => {
      let ticked = 0;
      for (let tries = 0; ticked < 20 && tries < 80; tries += 1) {
        const box = `[...document.querySelectorAll("[data-tool-row] input[type=checkbox]")].find((b) => !b.checked && !b.disabled && (() => { const r = b.getBoundingClientRect(); const l = ${LIST}.getBoundingClientRect(); return r.top > l.top + 4 && r.bottom < l.bottom - 4; })())`;
        if (await evaluate(cdp, `!!(${box})`)) {
          await click(cdp, box);
          ticked += 1;
          await sleep(120);
        } else {
          await evaluate(cdp, `(() => { const l = ${LIST}; l.scrollTop += 240; return true; })()`);
          await sleep(200);
        }
      }
      return { ticked };
    },
    { after: 500 },
  );
  await step(
    cdp,
    "batch-sheet",
    async () => {
      await click(cdp, `document.querySelector("[data-uninstall-selected]")`);
      const tOpen = await waitFor(cdp, `${DIALOG} !== null`);
      return { dialogSeenMs: tOpen, title: (await evaluate(cdp, `${DIALOG}.querySelector("h2")?.textContent`)) ?? null };
    },
    { after: 1500 },
  );
  await closeDialog(cdp);

  await step(cdp, "nav-updates", async () => {
    await click(cdp, NAV(["更新", "Updates"]));
    return { seenAfterMs: await waitFor(cdp, HAS_ROWS) };
  });
  await sleep(1500);
  await step(cdp, "scroll-updates", () => scrollToEnd(cdp), { after: 300 });

  await scrollTop0(cdp, "before-update-all");
  await sleep(600);
  // The dialog's Update: 「更新」 for one, 「更新这753个」 / "Update 753 Tools" for several (walk-3 W3-18).
  const DIALOG_UPDATE = `(() => { const d = ${DIALOG}; if (!d) return null; return [...d.querySelectorAll("button")].find((b) => { const t = b.textContent.trim().replace(/\\s+/g, " "); return ["更新", "Update"].includes(t) || /^更新这\\d+个$/.test(t) || /^Update \\d+ Tools?$/.test(t); }); })()`;
  await step(
    cdp,
    "update-all-sheet",
    async () => {
      await click(cdp, BUTTON_STARTS(["全部更新", "Update All", "Update all"]));
      const tOpen = await waitFor(cdp, `${DIALOG} !== null`);
      const t0 = Date.now() - tOpen;
      const tReady = await waitFor(cdp, `(() => { const b = ${DIALOG_UPDATE}; return b && !b.disabled; })()`, 60000);
      const rowsAtReady = await evaluate(cdp, `${DIALOG}.querySelectorAll("[data-sheet-tool]").length`);
      const total = await evaluate(cdp, `Number((${DIALOG_UPDATE}.textContent.match(/\\d+/) || ["0"])[0])`);
      await waitFor(cdp, `${DIALOG}.querySelectorAll("[data-sheet-tool]").length >= ${total}`, 60000);
      const allRowsMs = Date.now() - t0;
      return { dialogSeenMs: tOpen, readyAfterMs: tReady, rowsAtReady, total, allRowsMs, title: (await evaluate(cdp, `${DIALOG}.querySelector("h2")?.textContent`)) ?? null };
    },
    { after: 700 },
  );
  await closeDialog(cdp);

  await step(cdp, "nav-overview", async () => {
    await click(cdp, NAV(["概览", "Overview"]));
    return { seenAfterMs: await waitFor(cdp, `!!document.querySelector("[data-overview-tool-setup] button")`) };
  });
  await sleep(800);
  await step(
    cdp,
    "setup-sheet",
    async () => {
      await click(cdp, `document.querySelector("[data-overview-tool-setup] button")`);
      const tOpen = await waitFor(cdp, `${DIALOG} !== null`);
      return { dialogSeenMs: tOpen, title: (await evaluate(cdp, `${DIALOG}.querySelector("h2")?.textContent`)) ?? null };
    },
    { after: 1500 },
  );
  await closeDialog(cdp);

  // Last, as it changes the Mac: Update All's sheet again, then Update, until every update has started.
  if (!ONLY.size || ONLY.has("update-all-submit")) {
    await click(cdp, NAV(["更新", "Updates"]));
    await waitFor(cdp, HAS_ROWS);
    await sleep(1000);
    await click(cdp, BUTTON_STARTS(["全部更新", "Update All", "Update all"]));
    await waitFor(cdp, `(() => { const b = ${DIALOG_UPDATE}; return b && !b.disabled; })()`, 60000);
    await sleep(800);
    await step(
      cdp,
      "update-all-submit",
      async () => {
        const t0 = Date.now();
        await click(cdp, DIALOG_UPDATE);
        await waitFor(cdp, `${DIALOG} === null`, 120000);
        return { allStartedMs: Date.now() - t0 };
      },
      { after: 300 },
    );
  }

  await closePage(cdp);
}

const all = [];
async function main() {
  const ud = await launchChrome();
  try {
    for (let run = 1; run <= RUNS; run += 1) {
      for (const base of run % 2 === 0 ? [...BASES].reverse() : BASES) {
        current = { label: base.label, run, host: new URL(base.url).host, results: all };
        console.error(`--- run ${run} ${base.label} ${base.url} ${WIDTH}x${HEIGHT} throttle ${THROTTLE}`);
        await scenario(base);
      }
    }
  } finally {
    writeFileSync(OUT, JSON.stringify(all, null, 2));
    chrome.kill("SIGTERM");
    await sleep(500);
    try { process.kill(chrome.pid, "SIGKILL"); } catch {}
    rmSync(ud, { recursive: true, force: true });
  }
}
main().catch((e) => {
  console.error(e);
  process.exit(1);
});
