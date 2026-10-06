import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { keptDataOf } from "../lib/keptData";
import { saysSize, sizeText } from "../lib/sizes";
import { namesInSentence } from "../lib/sources";
import type { KeptWhat, Warning } from "../lib/types";
import { KEPT_DATA_KEYS, KEPT_WHAT_DETAIL_KEYS } from "../lib/warnings";
import { CopyButton } from "./CopyButton";
import { TextWithInfo } from "./InfoDetail";
import { SheetSection } from "./SheetParts";
import { SMALL_WRAPPING } from "./ui/group";

/**
 * What a path-list uninstall keeps that is the tool's own settings or data
 * (`Warning.WillKeep`) -- what the group's last line says can go to the
 * Trash in Finder. A dead link, a path not confirmed as the tool's, an
 * installer's staging folder and a Terminal settings file are not.
 */
const SETTINGS_OR_DATA: ReadonlySet<KeptWhat> = new Set<KeptWhat>(["Settings", "SettingsAndHistory", "ToolState"]);

/** What a path-list uninstall keeps is, said under its path (`Warning.WillKeep`). */
const WILL_KEEP_WHAT_KEYS: Record<KeptWhat, string> = {
  Settings: "clarity.willKeepWhat.Settings",
  SettingsAndHistory: "clarity.willKeepWhat.SettingsAndHistory",
  ToolState: "clarity.willKeepWhat.ToolState",
  ShellConfigLines: "clarity.willKeepWhat.ShellConfigLines",
  OutsideHome: "clarity.willKeepWhat.OutsideHome",
  NotOurs: "clarity.willKeepWhat.NotOurs",
  InstallerCache: "clarity.willKeepWhat.InstallerCache",
};

/** One line of the group: a path, its size where known, what it is, and its why where there is one. */
interface KeptLine {
  path: string;
  size: ReactNode | null;
  what: string;
  why: string | null;
  /** Under it: whose data inside it the size does not count, one line each (`KeepsData`'s `others`). */
  others: string[];
  /** Whose it is, where the group is about several tools (`ownersOf`), or null. */
  owners: string | null;
}

/**
 * The uninstall confirmation's 「卸载后会保留」 group (advantages round,
 * item 6), the one way any uninstall says what it leaves where it is: what
 * a tool's own installer's uninstall keeps (`WillKeep`: Claude Code's
 * `~/.claude`, a shell file it added lines to), then each folder or file a
 * tool keeps its own data in (`KeepsData`: `~/.codex`, Ollama's models) --
 * with about how much it takes where that is known, what it holds in a few
 * plain words, its why behind an ⓘ where it needs one, and Copy Path, which
 * copies the path as shown, `~` and all, and says 「已拷贝」 beside itself
 * (`CopyButton`, as the details' copy buttons do). These are hidden folders,
 * so Show in Finder would show nothing: under the list, one line says how
 * to reach one with the copied path -- Finder's Go to Folder, ⇧⌘G -- and,
 * where a path starts with `~`, that `~` is the home folder; and, where
 * the list holds the tool's settings or data, one more says that what is
 * not needed can be moved to the Trash in Finder, and to leave a Terminal
 * settings file where it is when one is listed too (the author's decision
 * U15 e: a sentence, no button -- in the Trash it can be dragged back).
 * There is no button, menu or command here that deletes or moves one.
 * Nothing when the plan names none. A folder
 * two tools share says, under it, which other tool's data it does not count
 * (`~/.gemini` without Antigravity CLI's `~/.gemini/antigravity-cli`).
 */
export function KeptDataGroup({
  warnings,
  ownersOf,
}: {
  warnings: readonly Warning[];
  /**
   * Whose each path is, by the names the list shows, where the group is
   * about several tools' uninstalls at once (`BatchUninstallSheet`): said
   * under the path, 「来自Claude Code和Codex」.
   */
  ownersOf?: (path: string) => string[];
}) {
  const { t } = useTranslation();
  const ownersText = (path: string): string | null => {
    const names = ownersOf?.(path) ?? [];
    return names.length === 0 ? null : t("batchUninstall.keptBy", { names: namesInSentence(t, names) });
  };
  const lines: KeptLine[] = [];
  // Whether the list holds the tool's settings or data, and a Terminal
  // settings file: what the last line says (`keptTrash`).
  let settingsOrData = false;
  let shellFile = false;
  for (const warning of warnings) {
    if (typeof warning !== "string" && "WillKeep" in warning) {
      const { path, what } = warning.WillKeep;
      settingsOrData ||= SETTINGS_OR_DATA.has(what);
      shellFile ||= what === "ShellConfigLines";
      const why = KEPT_WHAT_DETAIL_KEYS[what];
      lines.push({
        path,
        size: null,
        what: t(WILL_KEEP_WHAT_KEYS[what]),
        why: why === null ? null : t(why),
        others: [],
        owners: ownersText(path),
      });
    }
  }
  for (const item of keptDataOf(warnings)) {
    settingsOrData = true;
    // What the size does not count: another copy's program inside the
    // folder (Codex's own install in ~/.codex).
    const leftOut =
      item.leftOut.length === 0 ? null : item.leftOut.map((path) => t("clarity.keptLeftOut", { path })).join(" ");
    // A measured 0 says no size (`saysSize`), as one not known.
    const measured = item.size !== null && saysSize(item.size) ? item.size : null;
    const size =
      measured === null ? null : leftOut === null ? (
        sizeText(t, measured)
      ) : (
        <TextWithInfo text={sizeText(t, measured)} label={t("clarity.keptLeftOutLabel", { path: item.path })}>
          {leftOut}
        </TextWithInfo>
      );
    // With no size to hang it on, it goes behind the ⓘ of what it holds.
    const why = measured === null ? leftOut : null;
    // Another tool's data inside it, which the size leaves out: said under it.
    const others = item.others.map((other) => t("keepsData.notCounting", { tool: other.tool, path: other.path }));
    lines.push({ path: item.path, size, what: t(KEPT_DATA_KEYS[item.what]), why, others, owners: ownersText(item.path) });
  }
  if (lines.length === 0) return null;
  const home = lines.some((line) => line.path.startsWith("~"));
  return (
    <SheetSection title={t("keepsData.title")}>
      <ul className="flex flex-col gap-1.5">
        {lines.map((line) => (
          <li key={line.path} data-kept-data="" className="flex items-start gap-2">
            <div className="min-w-0 flex-1">
              <p className={`break-words text-foreground ${SMALL_WRAPPING}`}>
                <span data-kept-path="">{line.path}</span>
                {line.size !== null ? (
                  <span data-kept-size="" className="text-muted">
                    {" · "}
                    {line.size}
                  </span>
                ) : null}
              </p>
              <p data-kept-what="" className={`break-words text-muted ${SMALL_WRAPPING}`}>
                {line.why === null ? (
                  line.what
                ) : (
                  <TextWithInfo text={line.what} label={t("common.detailsLabel", { title: line.path })}>
                    {line.why}
                  </TextWithInfo>
                )}
              </p>
              {line.others.map((other) => (
                <p key={other} data-kept-others="" className={`break-words text-muted ${SMALL_WRAPPING}`}>
                  {other}
                </p>
              ))}
              {line.owners !== null ? (
                <p data-kept-owners="" className={`break-words text-muted ${SMALL_WRAPPING}`}>
                  {line.owners}
                </p>
              ) : null}
            </div>
            <CopyButton
              text={line.path}
              label={t("keepsData.copyPath")}
              ariaLabel={t("keepsData.copyPathLabel", { path: line.path })}
            />
          </li>
        ))}
      </ul>
      <p data-kept-find="" className={`mt-1.5 break-words text-muted ${SMALL_WRAPPING}`}>
        {t(home ? "keepsData.findInFinderHome" : "keepsData.findInFinder")}
      </p>
      {settingsOrData ? (
        <p data-kept-trash="" className={`mt-1.5 break-words text-muted ${SMALL_WRAPPING}`}>
          {t(shellFile ? "keptTrash.dataButShellFile" : "keptTrash.data")}
        </p>
      ) : null}
    </SheetSection>
  );
}
