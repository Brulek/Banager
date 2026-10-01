import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { keptDataOf } from "../lib/keptData";
import { saysSize, sizeText } from "../lib/sizes";
import type { KeptWhat, Warning } from "../lib/types";
import { KEPT_DATA_KEYS, KEPT_WHAT_DETAIL_KEYS } from "../lib/warnings";
import { CopyButton } from "./CopyButton";
import { TextWithInfo } from "./InfoDetail";
import { SheetSection } from "./SheetParts";
import { SMALL_WRAPPING } from "./ui/group";

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
 * so Show in Finder would show nothing; and there is no button, menu or
 * command here that deletes one. Nothing when the plan names none. A folder
 * two tools share says, under it, which other tool's data it does not count
 * (`~/.gemini` without Antigravity CLI's `~/.gemini/antigravity-cli`).
 */
export function KeptDataGroup({ warnings }: { warnings: readonly Warning[] }) {
  const { t } = useTranslation();
  const lines: KeptLine[] = [];
  for (const warning of warnings) {
    if (typeof warning !== "string" && "WillKeep" in warning) {
      const { path, what } = warning.WillKeep;
      const why = KEPT_WHAT_DETAIL_KEYS[what];
      lines.push({
        path,
        size: null,
        what: t(WILL_KEEP_WHAT_KEYS[what]),
        why: why === null ? null : t(why),
        others: [],
      });
    }
  }
  for (const item of keptDataOf(warnings)) {
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
    lines.push({ path: item.path, size, what: t(KEPT_DATA_KEYS[item.what]), why, others });
  }
  if (lines.length === 0) return null;
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
            </div>
            <CopyButton
              text={line.path}
              label={t("keepsData.copyPath")}
              ariaLabel={t("keepsData.copyPathLabel", { path: line.path })}
            />
          </li>
        ))}
      </ul>
    </SheetSection>
  );
}
