/**
 * 「拷贝诊断信息」, "Copy Diagnostic Info": a short plain text, in the
 * window's language, that the user can paste to whoever helps them --
 * which Banager, which macOS on which chip, each source with its version,
 * where its program is, how it is doing; the folders searched for commands
 * (Banager's own `PATH`, the login shell's when it could be read);
 * the last check and whether it finished; how many tools Terminal cannot
 * find and how many tools are installed twice; and the disk they take, once
 * measured. Help's item copies it without the list of tools; Settings'
 * button adds it when its checkbox is on (a private tap's or scope's name
 * can say where someone works).
 *
 * Never in it: an environment variable's value (a proxy setting can hold
 * a password) but the `PATH` folders, the home folder's path (written as
 * `~`, and any `/Users/<name>` left over is too), anything from a shell
 * file. Building it asks nothing of the network: the snapshot, the sizes
 * and `get_system_facts` are all on this Mac.
 */
import { useCallback, useEffect, useRef } from "react";
import { useQuery, type UseQueryResult } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { create } from "zustand";
import { getSystemFacts } from "./api";
import { SHOWN_FOR_MS, type CopyStatus } from "./clipboard";
import { twinsByArtifact } from "./commands";
import { useSizes, useSnapshot } from "./queries";
import { sizeText } from "./sizes";
import { failedSourceNames, instanceLabels, namesInSentence } from "./sources";
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
 * folder. Rust already writes each path it hands over with the home folder
 * as `~`; this is the last guard, for a path the snapshot carries that the
 * facts did not cover (a source found since they were read).
 */
export function withoutHomePaths(text: string): string {
  return text.replace(/\/Users\/(?!Shared(?:\/|\s|$))[^/\s]+/g, "~");
}

/** What one source's status line says: each thing that holds, or 「正常」. */
function statusOf(t: Translate, instance: ManagerInstance): string {
  const words: string[] = [];
  if (instance.status.unavailable !== null) words.push(t(UNAVAILABLE_WORDS[instance.status.unavailable]));
  if (instance.read_only_reason !== null) words.push(t("diagnostics.text.statusWord.readOnly"));
  if (instance.unverified_version !== null) words.push(t("diagnostics.text.statusWord.unverified"));
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
function toolsInstalledTwice(artifacts: readonly InstalledArtifact[]): number {
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
    if (snapshot?.stale) {
      const names = failedSourceNames(t, snapshot.errors, instances);
      lines.push(
        names.length === 0
          ? t("diagnostics.text.incompletePlain")
          : t("diagnostics.text.incomplete", { sources: namesInSentence(t, names) }),
      );
    } else {
      lines.push(t("diagnostics.text.complete"));
    }
  }
  lines.push(t("diagnostics.text.notFound", { number: artifacts.filter(notFoundInTerminal).length }));
  lines.push(t("diagnostics.text.twins", { number: toolsInstalledTwice(artifacts) }));
  if (sizes !== null && sizes.done && sizes.total !== null) {
    lines.push(t("diagnostics.text.diskTotal", { size: sizeText(t, sizes.total) }));
  }
  // Plain spaces: the window keeps 「About 1.2 GB」 on one line with a
  // no-break space, which a pasted text has no use for.
  return withoutHomePaths(lines.join("\n")).replace(/\u00a0/g, " ") + "\n";
}

/**
 * `get_system_facts`'s answer, asked for again when the snapshot's
 * generation moves (a source found or gone), the last answer kept
 * meanwhile. Mounted by `useMenuCommands` too, so that Help's item has it
 * at hand: the clipboard is written in the same turn as the click, and an
 * answer awaited first could lose it. A command that answers nothing
 * reads as null.
 */
export function useSystemFacts(): UseQueryResult<SystemFacts | null> {
  const { data: snapshot } = useSnapshot();
  return useQuery({
    queryKey: ["systemFacts", snapshot?.generation ?? 0] as const,
    queryFn: async () => (await getSystemFacts()) ?? null,
    placeholderData: (previous) => previous,
    staleTime: Infinity,
  });
}

/**
 * What the last copy did, wherever it was asked for: Settings shows it
 * beside its button. `reveal`: Help's item asked, and Settings is to bring
 * that button into view once it is on screen (`DiagnosticsRows`).
 */
export const useDiagnosticsStatus = create<{ status: CopyStatus; reveal: boolean }>(() => ({
  status: null,
  reveal: false,
}));

let statusTimer: number | undefined;

function sayStatus(status: "copied" | "failed"): void {
  useDiagnosticsStatus.setState({ status });
  window.clearTimeout(statusTimer);
  statusTimer = window.setTimeout(() => useDiagnosticsStatus.setState({ status: null }), SHOWN_FOR_MS);
}

/**
 * What copies the text: Settings' button, with its checkbox's answer, and
 * Help's item, always without the tools. The same function from one render
 * to the next; it builds the text from what the window holds at the click,
 * writes it at once, and says 「已拷贝」 or 「无法拷贝」 for a moment
 * (`useDiagnosticsStatus`), as the other Copy buttons do.
 */
export function useCopyDiagnostics(): (includeTools: boolean) => void {
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
        i18n.resolvedLanguage === "zh-CN" ? "settings.language.chinese" : "settings.language.english",
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
  return useCallback((includeTools: boolean) => {
    const text = latest.current(includeTools);
    if (navigator.clipboard === undefined) {
      sayStatus("failed");
      return;
    }
    navigator.clipboard.writeText(text).then(
      () => sayStatus("copied"),
      () => sayStatus("failed"),
    );
  }, []);
}
