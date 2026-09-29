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
 * Where a logo came from, as a person reads it: the site's name alone --
 * `www.` dropped -- and on GitHub the organization's too
 * ("github.com/dotnet"), never a whole address, which at this width
 * breaks in the middle of a word. What is not an address is left as it is.
 */
export function creditSource(url: string): string {
  let parsed: URL;
  try {
    parsed = new URL(url);
  } catch {
    return url;
  }
  const host = parsed.hostname.replace(/^www\./, "");
  const owner = parsed.pathname.split("/").filter((part) => part !== "")[0];
  return host === "github.com" && owner !== undefined ? `${host}/${owner}` : host;
}

/**
 * Where the logos built into Canager come from, opened from Settings'
 * 「关于」 group, as a dialog in the manner of the others (spec R11: it
 * answers a press, so it stays a dialog): that each logo is its owner's
 * and shown only to tell the tools apart; that most are Simple Icons',
 * which is CC0; every logo in the pack under a license of its own, by its
 * title, with the license's name and the site Simple Icons took it from
 * (`creditSource`), each with its whole address in its tooltip; and that
 * the others are GitHub avatars. The list is the pack's
 * (`ToolIcons.credits`), not one kept by hand, so it follows whatever the
 * mapping names. Nothing in the window opens a link. The dialog's body
 * scrolls, its bottom edge fading while more is below (`Dialog`). Done,
 * the default button, closes it.
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
                  <p className="font-semibold">{credit.title}</p>
                  <dl className="mt-0.5 grid grid-cols-[auto_1fr] gap-x-3 text-small">
                    <dt className="text-muted">{t("settings.iconCredits.license")}</dt>
                    <dd title={credit.license.url} className="min-w-0 break-words text-foreground">
                      {credit.license.type}
                    </dd>
                    <dt className="text-muted">{t("settings.iconCredits.source")}</dt>
                    <dd title={credit.source} className="min-w-0 break-words text-foreground">
                      {creditSource(credit.source)}
                    </dd>
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
