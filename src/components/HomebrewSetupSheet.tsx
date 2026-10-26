import { useEffect, useId, useRef, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { lookForHomebrew, openTerminal } from "../lib/api";
import { useSystemFacts } from "../lib/diagnostics";
import { HOMEBREW_INSTALL_COMMAND, LOOK_EVERY_MS, olderThanHomebrewSupports } from "../lib/homebrewSetup";
import { refreshIntoCache } from "../lib/events";
import { useSettings } from "../lib/queries";
import { DisclosureButton } from "./DisclosureButton";
import { CheckIcon } from "./icons";
import { Refusal, SheetPending, SheetText } from "./SheetParts";
import { SourceAvatar } from "./SourceAvatar";
import { Dialog } from "./ui/Dialog";
import { BUTTON } from "./ui/controls";

/** The steps the sheet lists, in order: what the person does in Terminal. */
const STEP_KEYS = [
  "discover.homebrew.steps.open",
  "discover.homebrew.steps.paste",
  "discover.homebrew.steps.password",
  "discover.homebrew.steps.return",
  "discover.homebrew.steps.done",
] as const;

/**
 * What Homebrew's installer does, in five plain lines (phase 5 proposal
 * §3.1, read from Homebrew/install's install.sh, docs/what-we-run.md
 * "Homebrew itself"): behind What It Does.
 */
const WHAT_KEYS = [
  "discover.homebrew.what.script",
  "discover.homebrew.what.admin",
  "discover.homebrew.what.tools",
  "discover.homebrew.what.place",
  "discover.homebrew.what.update",
] as const;

/** How many looks in a row must find Homebrew there and not updating. */
const LOOKS_TO_BE_SURE = 2;

export interface HomebrewSetupSheetProps {
  open: boolean;
  onClose: () => void;
}

/**
 * Install on Discover's Homebrew row (phase 5 step C): Homebrew is
 * installed in Terminal, and Banager neither runs its installer nor sees
 * the password. The sheet says what to do there, step by step; Open
 * Terminal puts Homebrew's own command on the clipboard
 * (`HOMEBREW_INSTALL_COMMAND`) and opens Terminal (`open_terminal`),
 * typing nothing into it. What the command does, and the command itself,
 * are a click away -- open from the start where Settings' Show technical
 * details is on. On a macOS older than Homebrew supports, it says so.
 *
 * While it is open it looks for Homebrew every two seconds
 * (`look_for_homebrew`, which runs nothing): Homebrew is installed once two
 * looks in a row find `/opt/homebrew/bin/brew` there and no `brew update`
 * running -- the installer's last step. Then it says so, and checks again,
 * which finds Homebrew and lights up the rows that install with it; closed
 * before then, the next check finds it all the same.
 */
export function HomebrewSetupSheet({ open, onClose }: HomebrewSetupSheetProps) {
  const { t } = useTranslation();
  const { data: settings } = useSettings();
  const { data: facts } = useSystemFacts();
  const queryClient = useQueryClient();
  const openRef = useRef<HTMLButtonElement>(null);
  const textId = useId();
  const whatId = useId();
  const commandId = useId();
  const technical = settings?.show_technical_details ?? false;
  const [whatOpen, setWhatOpen] = useState<boolean | null>(null);
  const [commandOpen, setCommandOpen] = useState<boolean | null>(null);
  const [opened, setOpened] = useState(false);
  const [copyFailed, setCopyFailed] = useState(false);
  const [openFailed, setOpenFailed] = useState(false);
  // Looks in a row that found Homebrew there and not updating.
  const [sure, setSure] = useState(0);
  const older = olderThanHomebrewSupports(facts?.macos_version);

  const look = useQuery({
    queryKey: ["lookForHomebrew"],
    queryFn: lookForHomebrew,
    enabled: open && sure < LOOKS_TO_BE_SURE,
    refetchInterval: open && sure < LOOKS_TO_BE_SURE ? LOOK_EVERY_MS : false,
    // On the interval alone, Terminal in front or not: a look the window's
    // coming back adds could land just after the last, and two looks are
    // to be two seconds apart.
    refetchIntervalInBackground: true,
    refetchOnWindowFocus: false,
    gcTime: 0,
  });
  // When the last look that counted was answered, so that a second one
  // counts only half a look's interval after it or later: two answers
  // close together (the window's coming back, then the interval) are one
  // look, and the interval's own drift does not lose one.
  const counted = useRef(0);
  useEffect(() => {
    // A look answered after the sheet closed is no look of this sheet's.
    if (look.data === undefined || !open) return;
    const at = look.dataUpdatedAt;
    if (!(look.data.there && !look.data.updating)) {
      setSure(0);
    } else if (at - counted.current >= LOOK_EVERY_MS / 2) {
      counted.current = at;
      setSure((was) => was + 1);
    }
    // Each look, once: `dataUpdatedAt` moves with every answer, the same or not.
  }, [look.dataUpdatedAt]);
  const installed = sure >= LOOKS_TO_BE_SURE;

  // Installed: check again, which finds it and lights the rows up. And
  // Continue, which takes Open Terminal's place, takes the focus it had:
  // a keyboard's Return or VoiceOver is not left on the sheet itself (from
  // the independent review of the branch).
  useEffect(() => {
    if (!installed) return;
    // Through the shared refresh, as Open Ollama's check is: one asked for
    // while another runs is run after it, not dropped -- a refresh that
    // began before Homebrew was there would not find it (from the review of
    // the uncovered areas).
    void refreshIntoCache(queryClient, "homebrew installed").catch(() => {});
    openRef.current?.focus();
  }, [installed]);

  // Each time it opens, from the start: as it opens, so that nothing a
  // look answered while it was closed counts.
  useEffect(() => {
    setWhatOpen(null);
    setCommandOpen(null);
    setOpened(false);
    setCopyFailed(false);
    setOpenFailed(false);
    setSure(0);
    counted.current = 0;
  }, [open]);

  async function openTerminalWithCommand() {
    setCopyFailed(false);
    setOpenFailed(false);
    let copied = false;
    try {
      if (navigator.clipboard !== undefined) {
        await navigator.clipboard.writeText(HOMEBREW_INSTALL_COMMAND);
        copied = true;
      }
    } catch {
      copied = false;
    }
    // Not copied: the command is shown, to copy by hand.
    if (!copied) {
      setCopyFailed(true);
      setCommandOpen(true);
    }
    try {
      await openTerminal();
    } catch {
      setOpenFailed(true);
    }
    setOpened(true);
  }

  const showWhat = whatOpen ?? technical;
  const showCommand = commandOpen ?? technical;
  const updating = !installed && look.data?.there === true && look.data.updating;

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!next) onClose();
      }}
      title={t("discover.homebrew.title")}
      // Five steps, each a sentence or two: the alert's width would wrap
      // nearly every one.
      width="several"
      icon={<SourceAvatar adapterId="brew" label={t("adapters.brew")} size="lg" />}
      describedBy={installed ? undefined : textId}
      initialFocus={openRef}
      footer={
        installed ? (
          <button ref={openRef} type="button" onClick={onClose} className={BUTTON.large.default}>
            {t("discover.homebrew.continue")}
          </button>
        ) : (
          <>
            <button type="button" onClick={onClose} className={BUTTON.large.grey}>
              {t("common.cancel")}
            </button>
            <button ref={openRef} type="button" onClick={() => void openTerminalWithCommand()} className={BUTTON.large.default}>
              {t("discover.homebrew.openTerminal")}
            </button>
          </>
        )
      }
    >
      {/* `status`, there and empty from the time the sheet opens: one put
          in the page with its words may never be read. */}
      <p role="status" className={installed ? "flex items-center gap-1.5 text-body text-foreground" : "sr-only"}>
        {installed ? (
          <>
            <CheckIcon size={14} className="shrink-0 text-success" />
            {t("discover.homebrew.installed")}
          </>
        ) : null}
      </p>
      {installed ? null : (
        <>
          <SheetText id={textId}>{t("discover.homebrew.text")}</SheetText>
          {older === null ? null : (
            <SheetText className="mt-2">{t("discover.homebrew.olderMacOS", { version: facts?.macos_version ?? "" })}</SheetText>
          )}
          <ol className="mt-3 flex list-decimal flex-col gap-1.5 pl-5 text-body text-foreground">
            {STEP_KEYS.map((key) => (
              <li key={key} className="break-words">
                {t(key)}
              </li>
            ))}
          </ol>

          <div className="mt-3">
            <DisclosureButton open={showWhat} panelId={whatId} onToggle={() => setWhatOpen(!showWhat)}>
              {t("discover.homebrew.whatTitle")}
            </DisclosureButton>
            {showWhat ? (
              <ul id={whatId} className="mt-1 flex list-disc flex-col gap-1 pl-5 text-small text-muted">
                {WHAT_KEYS.map((key) => (
                  <li key={key} className="break-words">
                    {t(key)}
                  </li>
                ))}
              </ul>
            ) : null}
          </div>
          <div className="mt-1">
            <DisclosureButton open={showCommand} panelId={commandId} onToggle={() => setCommandOpen(!showCommand)}>
              {t("commandPreview.show", { count: 1 })}
            </DisclosureButton>
            {showCommand ? (
              <code
                id={commandId}
                aria-label={t("discover.homebrew.commandLabel")}
                className="mt-1 block select-all whitespace-pre-wrap break-all rounded-control bg-group px-2.5 py-2 font-mono text-small text-foreground"
              >
                {HOMEBREW_INSTALL_COMMAND}
              </code>
            ) : null}
          </div>

          {copyFailed ? <Refusal className="mt-3" text={t("discover.homebrew.copyFailed")} detail={null} detailTitle="" /> : null}
          {openFailed ? <Refusal className="mt-3" text={t("discover.homebrew.openFailed")} detail={null} detailTitle="" /> : null}
          {opened || updating ? (
            <>
              <SheetPending text={updating ? t("discover.homebrew.updating") : t("discover.homebrew.waiting")} />
              {/* What to do where the installer stopped: this sheet cannot
                  see Terminal, and would wait for ever (from the review of
                  the uncovered areas). */}
              <SheetText className="mt-2">{t("discover.homebrew.ifItStops")}</SheetText>
            </>
          ) : null}
        </>
      )}
    </Dialog>
  );
}
