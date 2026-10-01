import { useRef, useState, type ComponentType } from "react";
import { useTranslation } from "react-i18next";
import { useQueryClient } from "@tanstack/react-query";
import { queryKeys } from "../lib/queryKeys";
import { useSaveSettings, useSettings } from "../lib/queries";
import type { Settings } from "../lib/types";
import { useWelcomeAgain } from "../lib/welcome";
import { CheckCircleIcon, InstalledIcon, SettingsIcon } from "./icons";
import { Dialog } from "./ui/Dialog";
import { BUTTON } from "./ui/controls";

/** The app's name, which is not translated. */
const APP_NAME = "Banager";

/**
 * The sheet's three points, each a symbol, a bold title and one line. Every
 * line is a promise docs/what-we-run.md makes: everything in one list; an
 * update or uninstall previewed, run only once confirmed and checked again
 * after it ("When commands run", "What Banager never does"); no shell
 * startup file edited, no host but the registries it reads versions from,
 * and no account ("Network", "What Banager never does").
 */
const POINTS: { icon: ComponentType<{ size?: number; className?: string }>; title: string; text: string }[] = [
  { icon: InstalledIcon, title: "welcome.listTitle", text: "welcome.listText" },
  { icon: CheckCircleIcon, title: "welcome.confirmTitle", text: "welcome.confirmText" },
  { icon: SettingsIcon, title: "welcome.settingsTitle", text: "welcome.settingsText" },
];

/**
 * Whether these settings ask for the welcome sheet: only when they say,
 * explicitly, that it has not been shown. Rust always sends `welcome_seen`;
 * the settings a test or the preview builds by hand without it never show
 * the sheet.
 */
export function welcomeDue(settings: Settings | undefined): boolean {
  return settings?.welcome_seen === false;
}

/**
 * The sheet Banager shows the first time it opens, in the manner of
 * macOS's welcome and What's New sheets: 「欢迎使用Banager」, three short
 * points, and one button, 「开始使用」, which has the focus, so Return
 * presses it.
 *
 * It decides once, as the settings first arrive (`welcomeDue`), and from
 * then on only closing it changes anything: by its button, Escape or a
 * click beside it, each of which saves `welcome_seen` over the settings as
 * they are by then, so it does not show again. Should that save fail, the
 * sheet stays closed and shows once more at the next launch. Rust keeps
 * `welcome_seen` true once it is (`Settings::keep_welcome_seen`), so a page
 * that saves settings it read before the sheet closed cannot bring it back.
 *
 * Help's 「欢迎使用Banager」 shows it again at any time (`openWelcomeSheet`,
 * src/lib/welcome.ts). Closing it then saves nothing, as the settings say
 * it was seen already; should they not -- the item chosen while the first
 * launch's sheet is up, or after its save failed -- closing saves
 * `welcome_seen` as above.
 *
 * It holds nothing up: the first check starts behind it as at any launch
 * (`useStartupRefresh` in `App`). Mounted once, by `App`.
 */
export function WelcomeSheet() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const { data: settings } = useSettings();
  const save = useSaveSettings();
  const startButton = useRef<HTMLButtonElement>(null);
  // null until the settings have arrived and it has decided.
  const [open, setOpen] = useState<boolean | null>(null);
  if (open === null && settings !== undefined) setOpen(welcomeDue(settings));
  const again = useWelcomeAgain((s) => s.open);
  const shown = open === true || again;

  const close = () => {
    if (!shown) return;
    // From null too: settings that arrive after the menu's sheet was
    // closed must not bring it back.
    setOpen(false);
    useWelcomeAgain.setState({ open: false });
    const latest = queryClient.getQueryData<Settings>(queryKeys.settings) ?? settings;
    if (latest === undefined || latest.welcome_seen !== false) return;
    save.mutate(
      { ...latest, welcome_seen: true },
      {
        onError: (e: unknown) => {
          console.error("saving welcome_seen failed", e);
        },
      },
    );
  };

  return (
    <Dialog
      open={shown}
      onOpenChange={(next) => {
        if (!next) close();
      }}
      title={t("welcome.title", { name: APP_NAME })}
      width="several"
      initialFocus={startButton}
      stackedFooter
      footer={
        <button ref={startButton} type="button" onClick={close} className={BUTTON.large.default}>
          {t("welcome.start")}
        </button>
      }
    >
      <ul data-welcome-points="" className="flex flex-col gap-4 pb-1 pt-3">
        {POINTS.map(({ icon: Icon, title, text }) => (
          <li key={title} className="flex items-start gap-3">
            <Icon size={28} className="shrink-0 text-accent" />
            <div className="min-w-0">
              <p className="break-words text-title text-foreground">{t(title)}</p>
              <p className="mt-0.5 break-words text-body-long text-muted">{t(text)}</p>
            </div>
          </li>
        ))}
      </ul>
    </Dialog>
  );
}
