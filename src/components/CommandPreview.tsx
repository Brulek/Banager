import { useId, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import type { TFunction } from "i18next";
import { displayToken } from "../lib/format";
import { previewEnvValue } from "../lib/previewEnv";
import { useSettings } from "../lib/queries";
import type { PlanAction } from "../lib/types";
import { SMALL_WRAPPING } from "./ui/group";
import { DisclosureButton } from "./DisclosureButton";

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
 * top of Banager's own environment, as `NAME=value` in the plan's order --
 * so a setting that changes what the tool does, such as Homebrew's
 * `HOMEBREW_NO_AUTOREMOVE=1`, is on screen -- then the argv, each value and
 * token per `displayToken`.
 */
export function commandText(action: Extract<PlanAction, { Command: unknown }>): string {
  const { program, args, env } = action.Command;
  return [
    ...env.map(([name, value]) => `${name}=${displayToken(previewEnvValue(name, value))}`),
    ...[program, ...args].map(displayToken),
  ].join(" ");
}

/**
 * Every command a plan runs, each as `commandText` gives it, in the order
 * they run: one for a `Command`; for a `CommandThen`, a Homebrew update and
 * each follow-up that runs once it has succeeded -- `brew link --formula --force`
 * (y1-keg), `brew cleanup` (U9) -- under the same variables; none for a
 * `TrashPaths`.
 */
export function commandTexts(action: PlanAction): string[] {
  if ("Command" in action) return [commandText(action)];
  if ("CommandThen" in action) {
    const { program, args, env, then } = action.CommandThen;
    return [args, ...then].map((argv) => commandText({ Command: { program, args: argv, env } }));
  }
  return [];
}

/**
 * The sentence a `TrashPaths` plan has in place of a command: its items go
 * to the Trash, and can be dragged back out of it -- nothing more. Not
 * Finder's Put Back, which works as often as not and is promised nowhere
 * (crates/banager-core/src/trash/mod.rs; the copy table's T4).
 */
function trashText(t: TFunction, action: Extract<PlanAction, { TrashPaths: unknown }>): string {
  return t("uninstall.trashPreview", { count: action.TrashPaths.paths.length });
}

/**
 * What a confirmation's plans will do, exactly, one click away.
 *
 * For a `Command`: the variables it is given and its argv
 * (`commandText`) -- for a `CommandThen`, both of its commands, the second
 * under the first (`commandTexts`) -- behind a disclosure -- 「查看命令」/
 * "Show the command", a button that says whether it is open
 * (`aria-expanded`) -- and open from the start while Settings' "Show
 * technical details" is on. One token per `displayToken`, since a plain
 * `join(" ")` cannot tell `/Users/Alice Smith/bin/brew` apart from a
 * program called `/Users/Alice` with an argument `Smith/bin/brew`; set as
 * code, which wraps rather than scrolls, and selects (`select-text`), to
 * be copied into Terminal. A sheet about several updates
 * lists each command under its tool's name, behind one disclosure.
 *
 * For a `TrashPaths` plan there is no command to show: Banager moves the
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
  const commands: Array<{ id: string; name?: string; texts: string[] }> = [];
  for (const plan of plans) {
    const { action } = plan;
    if ("Command" in action || "CommandThen" in action) {
      commands.push({ id: plan.id, name: plan.name, texts: commandTexts(action) });
    } else if ("TrashPaths" in action) {
      trash.push(
        <p key={plan.id} className={`text-muted ${SMALL_WRAPPING}`}>
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
        <div className="mt-3">
          <DisclosureButton open={open} panelId={panelId} onToggle={() => setChosen(!open)}>
            {t("commandPreview.show", { count: commands.reduce((sum, command) => sum + command.texts.length, 0) })}
          </DisclosureButton>
          {open ? (
            <div id={panelId} className="mt-1 flex flex-col gap-2">
              {commands.map((command) => (
                <div key={command.id}>
                  {command.name !== undefined ? (
                    <p className="mb-1 text-small text-muted">{command.name}</p>
                  ) : null}
                  {/* A brew cleanup that follows an update (U9) under it, a step apart. */}
                  {command.texts.map((text, index) => (
                    <code
                      key={index}
                      className={`block select-text whitespace-pre-wrap break-words rounded-control bg-group px-2.5 py-2 font-mono text-small text-foreground${index > 0 ? " mt-1" : ""}`}
                    >
                      {text}
                    </code>
                  ))}
                </div>
              ))}
            </div>
          ) : null}
        </div>
      ) : null}
    </>
  );
}
