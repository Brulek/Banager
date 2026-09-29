import { useId, useRef } from "react";
import { useTranslation } from "react-i18next";
import { useToolIcons } from "../lib/toolIconsContext";
import { Dialog } from "./ui/Dialog";
import { BUTTON } from "./ui/controls";

export interface IconCreditsDrawerProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

/**
 * Where the logos built into Canager come from, opened from Settings'
 * 「关于」 group, as a dialog in the manner of the others (spec R11: it
 * answers a press, so it stays a dialog): that each logo is its owner's
 * and shown only to tell the tools apart; that most are Simple Icons',
 * which is CC0; every logo in the pack under a license of its own, as
 * "<title> — <license>" with the license's URL and Simple Icons' source
 * for it; and that the others are GitHub avatars. The list is the pack's
 * (`ToolIcons.credits`), not one kept by hand, so it follows whatever the
 * mapping names. The URLs are text to copy: nothing in the window opens a
 * link. Done, the default button, closes it.
 */
export function IconCreditsDrawer({ open, onOpenChange }: IconCreditsDrawerProps) {
  const { t } = useTranslation();
  const { credits } = useToolIcons();
  const listTitleId = useId();
  const doneRef = useRef<HTMLButtonElement>(null);
  return (
    <Dialog
      open={open}
      onOpenChange={onOpenChange}
      title={t("settings.iconCredits.title")}
      // A list, as a dialog about several tools is: its sources' addresses
      // wrap less at its width than at an alert's.
      width="several"
      description={t("settings.iconCredits.owners")}
      initialFocus={doneRef}
      footer={
        <button ref={doneRef} type="button" onClick={() => onOpenChange(false)} className={BUTTON.large.default}>
          {t("common.done")}
        </button>
      }
    >
      {/* Its own Tab stop: nothing in it takes the focus, and the keyboard
          can scroll only what has it. The dialog's body scrolls it. */}
      <div
        role="region"
        aria-label={t("settings.iconCredits.title")}
        tabIndex={0}
        className="-mx-1 mt-3 rounded-control px-1 text-body text-foreground"
      >
        <p>{t("settings.iconCredits.simpleIcons")}</p>
        {credits.length > 0 ? (
          <>
            <p id={listTitleId} className="mt-3">
              {t("settings.iconCredits.ownLicense")}
            </p>
            <ul aria-labelledby={listTitleId} className="mt-2 flex flex-col gap-3">
              {credits.map((credit) => (
                <li key={credit.id}>
                  <p className="font-semibold">
                    {t("settings.iconCredits.entry", { title: credit.title, license: credit.license.type })}
                  </p>
                  <dl className="mt-0.5 grid grid-cols-[auto_1fr] gap-x-3 text-small">
                    <dt className="text-muted">{t("settings.iconCredits.license")}</dt>
                    <dd className="min-w-0 break-all text-foreground">{credit.license.url}</dd>
                    <dt className="text-muted">{t("settings.iconCredits.source")}</dt>
                    <dd className="min-w-0 break-all text-foreground">{credit.source}</dd>
                  </dl>
                </li>
              ))}
            </ul>
          </>
        ) : null}
        <p className="mt-3">{t("settings.iconCredits.avatars")}</p>
      </div>
    </Dialog>
  );
}
