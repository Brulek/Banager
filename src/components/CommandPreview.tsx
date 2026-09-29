import { useId, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import type { TFunction } from "i18next";
import { displayToken } from "../lib/format";
import { useSettings } from "../lib/queries";
import type { PlanAction } from "../lib/types";
import { ChevronIcon } from "./icons";
import { LINK } from "./ui/controls";

/** One plan a confirmation is about, for its preview. */
export interface PlanPreview {
  /** A stable key: the plan's id. */
  id: string;
  /** What it is for, over its command where a sheet lists several. */
  name?: string;
  action: PlanAction;
}

export interface CommandPreviewProps {
  plans: PlanPreview[];
}

/**
 * The command as Terminal would take it: the variables the plan sets on
 * top of Canager's own environment, as `NAME=value` in the plan's order --
 * so a setting that changes what the tool does, such as Homebrew's
 * `HOMEBREW_NO_AUTOREMOVE=1`, is on screen -- then the argv, each value and
 * token per `displayToken`.
 */
function commandText(action: Extract<PlanAction, { Command: unknown }>): string {
  const { program, args, env } = action.Command;
  return [
    ...env.map(([name, value]) => `${name}=${displayToken(value)}`),
    ...[program, ...args].map(displayToken),
  ].join(" ");
}

/**
 * The sentence a `TrashPaths` plan has in place of a command: its items go
 * to the Trash, and can be dragged back out of it -- nothing more. Not
 * Finder's Put Back, which works as often as not and is promised nowhere
 * (crates/canager-core/src/trash/mod.rs; the copy table's T4).
 */
function trashText(t: TFunction, action: Extract<PlanAction, { TrashPaths: unknown }>): string {
  return t("uninstall.trashPreview", { count: action.TrashPaths.paths.length });
}

/**
 * What a confirmation's plans will do, exactly, one click away.
 *
 * For a `Command`: the variables it is given and its argv
 * (`commandText`), behind a disclosure -- 「查看命令」/
 * "Show the command", a button that says whether it is open
 * (`aria-expanded`) -- and open from the start while Settings' "Show
 * technical details" is on. One token per `displayToken`, since a plain
 * `join(" ")` cannot tell `/Users/Alice Smith/bin/brew` apart from a
 * program called `/Users/Alice` with an argument `Smith/bin/brew`; set as
 * code, which wraps rather than scrolls, and selects (`select-text`), to
 * be copied into Terminal. A sheet about several updates
 * lists each command under its tool's name, behind one disclosure.
 *
 * For a `TrashPaths` plan there is no command to show: Canager moves the
 * items itself. It says so in one sentence, in the open, under the list of
 * what moves -- a button promising a command would promise one that does
 * not exist.
 */
export function CommandPreview({ plans }: CommandPreviewProps) {
  const { t } = useTranslation();
  const { data: settings } = useSettings();
  // Until pressed, the disclosure follows the setting.
  const [chosen, setChosen] = useState<boolean | null>(null);
  const open = chosen ?? settings?.show_technical_details ?? false;
  const panelId = useId();

  const trash: ReactNode[] = [];
  const commands: Array<{ id: string; name?: string; text: string }> = [];
  for (const plan of plans) {
    const { action } = plan;
    if ("Command" in action) {
      commands.push({ id: plan.id, name: plan.name, text: commandText(action) });
    } else if ("TrashPaths" in action) {
      trash.push(
        <p key={plan.id} className="text-small text-muted">
          {trashText(t, action)}
        </p>,
      );
    } else {
      const unhandled: never = action;
      void unhandled;
    }
  }

  return (
    <>
      {trash}
      {commands.length > 0 ? (
        <div className="mt-5">
          <button
            type="button"
            aria-expanded={open}
            aria-controls={open ? panelId : undefined}
            onClick={() => setChosen(!open)}
            className={`-ml-1 inline-flex items-center gap-1 px-1 py-0.5 text-small ${LINK}`}
          >
            <ChevronIcon size={14} className={`shrink-0 transition-transform ${open ? "rotate-90" : ""}`} />
            {t("commandPreview.show", { count: commands.length })}
          </button>
          {open ? (
            <div id={panelId} className="mt-2 flex flex-col gap-2.5">
              {commands.map((command) => (
                <div key={command.id}>
                  {command.name !== undefined ? (
                    <p className="mb-1 text-small font-medium text-muted">{command.name}</p>
                  ) : null}
                  <code className="block select-text whitespace-pre-wrap break-words rounded-button bg-[var(--color-hover)] px-3 py-2 font-mono text-small text-foreground">
                    {command.text}
                  </code>
                </div>
              ))}
            </div>
          ) : null}
        </div>
      ) : null}
    </>
  );
}
