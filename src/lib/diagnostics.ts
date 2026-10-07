/**
 * 「拷贝诊断信息」, "Copy Diagnostic Info": a short plain text, in the
 * window's language, that the user can paste to whoever helps them --
 * which Banager, which macOS on which chip, each source with its version,
 * where its program is, how it is doing, and the error details of one that
 * did not answer (`NoAnswer.diagnostic`); the folders searched for commands
 * (Banager's own `PATH`, the login shell's when it could be read);
 * the last check and whether it finished; how many tools Terminal cannot
 * find and how many tools are installed twice; and the disk they take, once
 * measured. Settings' button copies it -- Help's item takes the user there --
 * and adds the list of tools when its checkbox is on (a private tap's or scope's name
 * can say where someone works).
 *
 * Never put in it by Banager: an environment variable's value (a proxy
 * setting can hold a password) but the `PATH` folders, the home folder's
 * path (written as `~`, and any `/Users/<name>` left over is too), anything
 * from a shell file. A source's error details are its tool's own stderr,
 * masked by the runner (`runner::redact`): they can quote a setting, its
 * login masked, or a line of the tool's own settings file, and a token
 * there that is no URL login, or a login in a form the runner does not
 * read as one (docs/what-we-run.md, "What a tool prints about a login"),
 * is copied as written -- Settings' footnote and its ⓘ say so. Building
 * it asks nothing of the network: the snapshot, the sizes and
 * `get_system_facts` are all on this Mac.
 */
import { useCallback, useEffect, useRef } from "react";
import { useQuery, type UseQueryResult } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { create } from "zustand";
import { getSystemFacts } from "./api";
import { twinsByArtifact } from "./commands";
import { commandsKnown, toolsNotJudged } from "./commandsKnown";
import { updatesUnchecked } from "./uncheckedStandalone";
import { useSizes, useSnapshot } from "./queries";
import { sizeTotalsOf, sourceTotalText } from "./sizeTotals";
import { adapterLabel, failedSourceAdapters, instanceLabels, namesInSentence } from "./sources";
import { checkedInFull } from "./updateState";
import { NO_ANSWER_WORDS, saidNoAnswer } from "./noAnswer";
import type { InstalledArtifact, ManagerInstance, Sizes, Snapshot, SystemFacts, Unavailable } from "./types";

/** Whatever `useTranslation()`'s `t` needs to look a key up. */
export type Translate = (key: string, options?: Record<string, unknown>) => string;

/** Everything the text is made of; `diagnosticsText` reads nothing else. */
export interface DiagnosticsInput {
  /** When it is copied: the first line after the title. */
  now: Date;
  /** The app's name and version, as Settings' About shows them. */
  appName: string;
  appVersion: string;
  /** The window's language, named as Settings names it: 「简体中文」, "English". */
  languageName: string;
  /** `get_system_facts`'s answer; null when it could not be had. */
  facts: SystemFacts | null;
  snapshot: Snapshot | null;
  sizes: Sizes | null;
  /** Each source's tools as name and version, one a line. Off from Help's item. */
  includeTools: boolean;
}

/** A source's status in one word, by why it could not answer. A `Record`, so a new `Unavailable` without a word fails `tsc`. */
const UNAVAILABLE_WORDS: Record<Unavailable, string> = {
  NotRunning: "diagnostics.text.statusWord.NotRunning",
  NotResponding: "diagnostics.text.statusWord.NotResponding",
  RefusesAsRoot: "diagnostics.text.statusWord.RefusesAsRoot",
  HttpsHostRefused: "sourceNotice.httpsHostRefused.statusWord",
  NoPip: "sourceNotice.noPip.statusWord",
};

/** The app's name: a name, the same in every language (src-tauri/tauri.conf.json's `productName`). */
const APP_NAME = "Banager";

/** Two levels of indent: under a source, and under its tools. */
const INDENT = "  ";

/** `2026-10-01 14:03`, local time, the same in both languages: a helper reads it, and so may a search. */
export function diagnosticsTime(date: Date): string {
  const two = (n: number) => String(n).padStart(2, "0");
  return `${date.getFullYear()}-${two(date.getMonth() + 1)}-${two(date.getDate())} ${two(date.getHours())}:${two(date.getMinutes())}`;
}

/**
 * Every `/Users/<name>` left in `text` as `~`, but macOS's own shared
 * folder, and the data volume's spelling of it
 * (`/System/Volumes/Data/Users/<name>`) as `~` too, not as
 * `/System/Volumes/Data~`. Rust already writes each path it hands over
 * with the home folder as `~`; this is the last guard, for a path the
 * snapshot carries that the facts did not cover (a source found since
 * they were read).
 */
export function withoutHomePaths(text: string): string {
  return text.replace(/(?:\/System\/Volumes\/Data)?\/Users\/(?!Shared(?:\/|\s|$))[^/\s]+/g, "~");
}

/**
 * The sources the last check did not cover in full, by name, in the
 * order the snapshot lists them: each one a call failed for
 * (`failedSourceAdapters`), and each other one that did not answer or
 * whose updates went unchecked this time (`checkedInFull`) -- but not one
 * whose updates Banager never checks (`updatesUnchecked`), which the line
 * names apart.
 */
function notCheckedNames(t: Translate, instances: ManagerInstance[], errors: Snapshot["errors"]): string[] {
  const failed = failedSourceAdapters(errors, instances);
  const adapters: string[] = [];
  const add = (adapterId: string) => {
    if (!adapters.includes(adapterId)) adapters.push(adapterId);
  };
  for (const instance of instances) {
    if (failed.includes(instance.adapter_id) || (!updatesUnchecked(instance) && !checkedInFull(instance))) {
      add(instance.adapter_id);
    }
  }
  for (const adapterId of failed) add(adapterId);
  return adapters.map((adapterId) => adapterLabel(t, adapterId));
}

/**
 * Each thing that holds of one source's state, in a word -- 「没有响应」,
 * 「仅供查看」, 「未经测试的版本」 -- none for one that is 「正常」. The
 * status line here and Check Tool Setup's source lines
 * (src/lib/toolSetupCheck.ts) both say a source's state in these words.
 */
export function sourceStateWords(t: Translate, instance: ManagerInstance): string[] {
  const words: string[] = [];
  const why = saidNoAnswer(instance);
  if (why !== null) words.push(t(NO_ANSWER_WORDS[why.kind]));
  else if (instance.status.unavailable !== null) words.push(t(UNAVAILABLE_WORDS[instance.status.unavailable]));
  if (instance.read_only_reason !== null) words.push(t("diagnostics.text.statusWord.readOnly"));
  if (instance.unverified_version !== null) words.push(t("diagnostics.text.statusWord.unverified"));
  return words;
}

/** What one source's status line says: each thing that holds, or 「正常」. */
function statusOf(t: Translate, instance: ManagerInstance): string {
  const words = sourceStateWords(t, instance);
  if (words.length === 0) return t("diagnostics.text.statusWord.ok");
  return words.join(t("common.listSeparator"));
}

/**
 * A tool's version as the list shows it: an Ollama model's is the digest
 * of its manifest, 64 hex digits, cut to the 12 `ollama list` shows.
 */
function shownVersion(artifact: InstalledArtifact): string {
  return artifact.key.kind === "Model" && /^[0-9a-f]{64}$/.test(artifact.version)
    ? artifact.version.slice(0, 12)
    : artifact.version;
}

/** Whether any of `artifact`'s commands is one Terminal cannot find. */
function notFoundInTerminal(artifact: InstalledArtifact): boolean {
  return artifact.facts.commands.some(
    (command) => command.state !== null && typeof command.state === "object" && "NotOnPath" in command.state,
  );
}

/**
 * How many tools are installed more than once: one tool (a family, such as
 * Claude Code from npm and from its own installer) counts once however many
 * of its copies there are, not once a copy as `twinsByArtifact` keys them.
 */
export function toolsInstalledTwice(artifacts: readonly InstalledArtifact[]): number {
  const families = new Set<string>();
  for (const twins of twinsByArtifact(artifacts).values()) {
    // Every twin is of the artifact's own family (`twinsByArtifact` pairs within one).
    const family = twins[0]?.artifact.facts.family;
    if (family != null) families.add(family);
  }
  return families.size;
}

/**
 * The diagnostic text, line by line. Pure: the golden tests in
 * diagnostics.test.ts build it from a fixed snapshot in both languages.
 */
export function diagnosticsText(t: Translate, input: DiagnosticsInput): string {
  const { facts, snapshot, sizes } = input;
  const lines: string[] = [];
  lines.push(t("diagnostics.text.title"));
  lines.push(t("diagnostics.text.time", { time: diagnosticsTime(input.now) }));
  lines.push(t("diagnostics.text.app", { app: input.appName, version: input.appVersion }));
  if (facts === null) {
    lines.push(t("diagnostics.text.noFacts"));
  } else {
    lines.push(
      facts.macos_version === null
        ? t("diagnostics.text.macosUnknown")
        : t("diagnostics.text.macos", { version: facts.macos_version }),
    );
    const chip =
      facts.chip ??
      (facts.arch === "aarch64"
        ? t("diagnostics.text.chipAppleSilicon")
        : facts.arch === "x86_64"
          ? t("diagnostics.text.chipIntel")
          : facts.arch);
    lines.push(t("diagnostics.text.chip", { chip }));
  }
  lines.push(t("diagnostics.text.language", { language: input.languageName }));

  const instances = snapshot?.instances ?? [];
  const artifacts = snapshot?.artifacts ?? [];
  const labels = instanceLabels(t, instances);
  const paths = new Map((facts?.sources ?? []).map((source) => [source.instance_id, source.exe_path]));
  lines.push("");
  lines.push(t("diagnostics.text.sources", { number: instances.length }));
  for (const instance of instances) {
    const version = instance.version ?? instance.unverified_version;
    const tools = artifacts
      .filter((artifact) => artifact.key.instance_id === instance.id)
      .sort((a, b) => (a.key.name < b.key.name ? -1 : a.key.name > b.key.name ? 1 : 0));
    lines.push(labels.get(instance.id) ?? instance.adapter_id);
    lines.push(
      INDENT +
        (version === null
          ? t("diagnostics.text.versionUnknown")
          : t("diagnostics.text.version", { version })),
    );
    lines.push(INDENT + t("diagnostics.text.path", { path: paths.get(instance.id) ?? instance.exe_path }));
    lines.push(INDENT + t("diagnostics.text.status", { status: statusOf(t, instance) }));
    const diagnostic = instance.status.no_answer?.diagnostic;
    // The source's own words, a step further in: at the line start they
    // would read as the next source's name.
    if (diagnostic) {
      lines.push(INDENT + t("sourceDiagnostic.text"));
      for (const line of diagnostic.split("\n")) lines.push(INDENT + INDENT + line);
    }
    if (instance.status.notes.length > 0) {
      lines.push(INDENT + t("diagnostics.text.notes", { number: instance.status.notes.length }));
    }
    lines.push(INDENT + t("diagnostics.text.tools", { number: tools.length }));
    if (input.includeTools) {
      for (const tool of tools) lines.push(`${INDENT}${INDENT}${tool.key.name} ${shownVersion(tool)}`);
    }
  }

  if (facts !== null) {
    lines.push("");
    lines.push(t("diagnostics.text.searchPath", { number: facts.path_dirs.length }));
    lines.push(INDENT + t(facts.login_path ? "diagnostics.text.loginRead" : "diagnostics.text.loginNotRead"));
    for (const dir of facts.path_dirs) lines.push(INDENT + dir);
  }

  lines.push("");
  const refreshedAt = snapshot?.refreshed_at ?? null;
  if (refreshedAt === null) {
    lines.push(t("diagnostics.text.neverChecked"));
  } else {
    lines.push(t("diagnostics.text.lastCheck", { time: diagnosticsTime(new Date(refreshedAt * 1000)) }));
    // The sources whose updates are never checked (Codex's own install,
    // `updatesUnchecked`): said, not passed over, whether or not the rest
    // of the check finished.
    const labels = instanceLabels(t, instances);
    const unchecked = instances
      .filter(updatesUnchecked)
      .map((instance) => labels.get(instance.id) ?? instance.adapter_id);
    // Complete only as the Updates page and the Overview say "up to date"
    // (`everySourceChecked`): no call failed, and every other source
    // answered and had its updates checked in full -- not a Homebrew still
    // rewriting its list, nor an Ollama that is not running. Those that
    // fell short are named, as a failed one is.
    const short = notCheckedNames(t, instances, snapshot?.errors ?? []);
    if ((snapshot?.errors.length ?? 0) > 0 || short.length > 0) {
      const check =
        short.length === 0
          ? t("diagnostics.text.incompletePlain")
          : t("diagnostics.text.incomplete", { sources: namesInSentence(t, short) });
      lines.push(
        unchecked.length === 0
          ? check
          : t("clarity.incompleteExcept", { check, sources: namesInSentence(t, unchecked) }),
      );
    } else {
      lines.push(
        unchecked.length === 0
          ? t("diagnostics.text.complete")
          : t("clarity.completeExcept", { sources: namesInSentence(t, unchecked) }),
      );
    }
  }
  // 0 only when the check looked (`commandsKnown`): a round with no
  // verdicts, or no commands at all, says it did not. Where it looked at
  // some tools and not at others (`toolsNotJudged`), how many it could not
  // check, on a line of its own, so that a pasted 0 is not read as all.
  const judged = commandsKnown(artifacts, false, "verdicts") === "known";
  lines.push(
    judged
      ? t("diagnostics.text.notFound", { number: artifacts.filter(notFoundInTerminal).length })
      : t("commandsKnown.notFoundUnknown"),
  );
  const notJudged = judged ? toolsNotJudged(artifacts) : 0;
  if (notJudged > 0) lines.push(t("setupCheckCoverage.diagnosticsNotChecked", { number: notJudged }));
  lines.push(
    commandsKnown(artifacts, false, "names") === "known"
      ? t("diagnostics.text.twins", { number: toolsInstalledTwice(artifacts) })
      : t("commandsKnown.twinsUnknown"),
  );
  // The toolbar's total and hedge (`sizeTotalsOf`): measured for this
  // snapshot's round, and 「…以上」 when some tool has no size in it -- a
  // pip package, a cask with no app -- as well as when the budget ran out.
  const total = sizeTotalsOf(sizes ?? undefined, snapshot ?? undefined).all;
  if (total !== null) {
    lines.push(t("diagnostics.text.diskTotal", { size: sourceTotalText(t, total) }));
  }
  // Plain spaces: the window keeps 「About 1.2 GB」 on one line with a
  // no-break space, which a pasted text has no use for.
  return withoutHomePaths(lines.join("\n")).replace(/\u00a0/g, " ") + "\n";
}

/**
 * `get_system_facts`'s answer, asked for again when the snapshot's
 * generation moves (a source found or gone) or a new round lands, the
 * last answer kept meanwhile. The round counts too because
 * `path_folders` is each round's own, also when the round found nothing
 * new and kept the generation. Mounted by `useMenuCommands` too, so that Help's item has it
 * at hand: the clipboard is written in the same turn as the click, and an
 * answer awaited first could lose it. A command that answers nothing
 * reads as null.
 */
export function useSystemFacts(): UseQueryResult<SystemFacts | null> {
  const { data: snapshot } = useSnapshot();
  return useQuery({
    queryKey: ["systemFacts", snapshot?.generation ?? 0, snapshot?.round ?? 0] as const,
    queryFn: async () => (await getSystemFacts()) ?? null,
    placeholderData: (previous) => previous,
    staleTime: Infinity,
  });
}

/**
 * Help's 「拷贝诊断信息…」 asked for Settings' button: Settings is to bring
 * it into view and give it the focus once it is on screen
 * (`DiagnosticsRows`). The item itself copies nothing: a webview may not
 * take a menu item's event as the click a clipboard write needs, so the
 * copy is always the button's.
 */
export const useDiagnosticsReveal = create<{ reveal: boolean }>(() => ({ reveal: false }));

/**
 * What builds the text: Settings' button, with its checkbox's answer, at
 * the click (`CopyButton`). The same function from one render to the
 * next; it builds the text from what the window holds when it is called.
 */
export function useDiagnosticsText(): (includeTools: boolean) => string {
  const { t, i18n } = useTranslation();
  const { data: snapshot } = useSnapshot();
  const { data: sizes } = useSizes();
  const { data: facts } = useSystemFacts();
  const build = (includeTools: boolean) =>
    diagnosticsText(t, {
      now: new Date(),
      appName: APP_NAME,
      appVersion: __APP_VERSION__,
      languageName: t(
        i18n.resolvedLanguage === "zh-Hant"
          ? "settings.language.traditionalChinese"
          : i18n.resolvedLanguage === "zh-CN" ? "settings.language.chinese" : "settings.language.english",
      ),
      facts: facts ?? null,
      snapshot: snapshot ?? null,
      sizes: sizes ?? null,
      includeTools,
    });
  const latest = useRef(build);
  useEffect(() => {
    latest.current = build;
  });
  return useCallback((includeTools: boolean) => latest.current(includeTools), []);
}
