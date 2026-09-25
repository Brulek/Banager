import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import type { TFunction } from "i18next";
import { displayToken } from "../lib/format";
import type { PlanAction } from "../lib/types";

export interface CommandPreviewProps {
  action: PlanAction;
}

/**
 * What a Plan will do, exactly. For a `Command`: the argv under "This
 * will run:", one token per `displayToken`, since a plain `join(" ")`
 * cannot tell `/Users/Alice Smith/bin/brew` apart from a program called
 * `/Users/Alice` with an argument `Smith/bin/brew`; both destructive paths
 * (updates and uninstall) rely on this component as the operator's only
 * view of what is about to run. For a `TrashPaths` plan there is no
 * command to show: the honest preview is one sentence -- Canager moves the
 * listed items to the Trash itself, no command runs, nothing is deleted --
 * under a label of its own (`commandPreview.trashLabel`), never "This will
 * run:", and the items are the dialog's `WillTrash` warnings above it
 * (spec §6.2). Not in a <code> block: there is nothing to paste into a
 * terminal.
 */
export function CommandPreview({ action }: CommandPreviewProps) {
  const { t } = useTranslation();
  const { label, body } = preview(t, action);
  return (
    <div>
      <p className="text-xs font-medium uppercase text-[var(--color-muted)]">{label}</p>
      {body}
    </div>
  );
}

/** The label and the body for each arm; a third arm fails `tsc` here. */
function preview(t: TFunction, action: PlanAction): { label: string; body: ReactNode } {
  if ("Command" in action) {
    return {
      label: t("commandPreview.label"),
      body: (
        <code className="mt-1 block overflow-x-auto rounded-md bg-[var(--color-hover)] px-3 py-2 text-xs text-[var(--color-foreground)]">
          {[action.Command.program, ...action.Command.args].map(displayToken).join(" ")}
        </code>
      ),
    };
  }
  if ("TrashPaths" in action) {
    return {
      label: t("commandPreview.trashLabel"),
      body: (
        <p className="mt-1 rounded-md bg-[var(--color-hover)] px-3 py-2 text-sm text-[var(--color-foreground)]">
          {t("uninstall.trashPreview", { count: action.TrashPaths.paths.length })}
        </p>
      ),
    };
  }
  const unhandled: never = action;
  return unhandled;
}
