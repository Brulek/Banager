import { useTranslation } from "react-i18next";
import { copyStatusText, useCopyCommand } from "../lib/clipboard";
import { outcomeCause } from "../lib/failureCause";
import type { OpSummary } from "../lib/types";
import { commandText } from "./CommandPreview";
import { displayToken } from "../lib/format";
import { previewEnvValue } from "../lib/previewEnv";
import { adapterIdOf } from "../lib/sources";
import { BUTTON } from "./ui/controls";

type TerminalPreview = Pick<OpSummary, "argv_preview" | "env_preview">;

/**
 * `SUDO_ASKPASS` is the one variable a plan can carry that is left out of
 * the command for Terminal: it names a program that asks for the password
 * in a window of its own (Homebrew then runs `sudo -A`), and the way on
 * from here is sudo asking in Terminal itself, as the sentence above the
 * command says. Every other variable stays, `HOMEBREW_NO_AUTOREMOVE=1` and
 * `HOMEBREW_NO_INSTALL_CLEANUP=1` first among them: without them Homebrew
 * in Terminal would remove and clean up more than the confirmation said.
 */
const LEFT_OUT = new Set(["SUDO_ASKPASS"]);

/**
 * The command a failed operation ran, as Terminal would take it -- the
 * plan's variables, then its argv, each token per `displayToken`, exactly
 * as the confirmation showed it (`commandText`) -- or null for one that
 * ran no command.
 */
export function terminalCommand(op: TerminalPreview): string | null {
  const [program, ...args] = op.argv_preview;
  if (program === undefined) return null;
  const env = (op.env_preview ?? []).filter(([name]) => !LEFT_OUT.has(name));
  return commandText({ Command: { program, args, env } });
}

/**
 * The same command in two parts, to be shown as two lines: the settings
 * it runs with (`HOMEBREW_NO_AUTO_UPDATE=1 …`), technical and muted, and
 * the command itself, each a list of tokens (`displayToken`) a line may
 * break between and never inside -- 「upgrade --」 / 「cask …」 reads as
 * something else. Copy still copies `terminalCommand`, one line.
 */
export function terminalCommandParts(op: TerminalPreview): { env: string[]; command: string[] } | null {
  const [program, ...args] = op.argv_preview;
  if (program === undefined) return null;
  const env = (op.env_preview ?? []).filter(([name]) => !LEFT_OUT.has(name));
  return {
    env: env.map(([name, value]) => `${name}=${displayToken(previewEnvValue(name, value))}`),
    command: [program, ...args].map(displayToken),
  };
}

/** `tokens` set with a space between each two, a line breaking only there. */
function unbrokenTokens(tokens: string[]) {
  return tokens.flatMap((token, index) => [
    ...(index === 0 ? [] : [" "]),
    <span key={index} className="whitespace-nowrap">
      {token}
    </span>,
  ]);
}

/**
 * Whether `op` ran Homebrew: the one source whose steps run `sudo` (a
 * cask's installer or uninstaller), and the one whose command this hands
 * over -- the `brew` argv with Homebrew's variables. Another source whose
 * output carried sudo's lines would hand over a command of its own that
 * the words here were not written for; its log keeps the cause and the
 * next step, without a command.
 */
function fromHomebrew(op: OpSummary): boolean {
  const program = op.argv_preview[0] ?? "";
  return adapterIdOf(op.instance_id) === "brew" && program.split("/").pop() === "brew";
}

/**
 * Whether `PasswordCommand` shows a command for `op`: a Homebrew
 * operation (`fromHomebrew`) that ran a command and ended because sudo
 * wanted the Mac's password (`needsPassword`, `passwordNotAccepted`).
 */
export function showsPasswordCommand(op: OpSummary): boolean {
  if (op.status !== "Done" || !fromHomebrew(op) || op.argv_preview.length === 0) return false;
  const cause = outcomeCause(op.outcome);
  return cause === "needsPassword" || cause === "passwordNotAccepted";
}

/**
 * Under a log's next step, where an operation failed because `sudo`
 * wanted the Mac's password and had no way to ask for it
 * (`needsPassword`, src/lib/failureCause.ts), or asked in a password
 * window that got none or a wrong one (`passwordNotAccepted`): the way on.
 * Banager runs every command without a terminal and has no password
 * window of its own, so the same command, run in Terminal, is where sudo
 * can always ask -- said in one
 * sentence with the one thing that stops people there (nothing shows as
 * the password is typed), then the command itself, set as code that
 * selects whole, Copy Command beside a word on whether it worked, and
 * what to do after: come back and check again.
 *
 * Nothing here runs anything: the command is only text to copy.
 * Renders nothing for any other ending, for an operation that ran no
 * command, or for one that did not run Homebrew (`fromHomebrew`).
 */
export function PasswordCommand({ op }: { op: OpSummary }) {
  if (!showsPasswordCommand(op)) return null;
  return <PasswordInstructions preview={op} />;
}

/** Shared by a finished operation and a newly validated history preview. */
export function PasswordInstructions({ preview: op }: { preview: TerminalPreview }) {
  const { t } = useTranslation();
  const { status, copy } = useCopyCommand();
  const command = terminalCommand(op);
  const parts = terminalCommandParts(op);
  if (command === null || parts === null) return null;
  const copyWords = copyStatusText(t, status);
  return (
    <div className="mb-3 flex flex-col gap-2">
      <p className="break-words text-body text-foreground">{t("needsPassword.intro")}</p>
      {/* Named by a group around it: a name on <code> itself is not
          one assistive technology reliably reads. */}
      <div role="group" aria-label={t("needsPassword.commandLabel")}>
        {/* The settings on a muted line of their own, then the command:
            what a person reads first is the command they run. One code
            block, which selects and copies as the one line it is. */}
        <code className="block select-all break-words rounded-control bg-group px-2.5 py-2 font-mono text-small text-foreground">
          {parts.env.length > 0 ? (
            <span data-command-env="" className="block text-muted">
              {unbrokenTokens(parts.env)}{" "}
            </span>
          ) : null}
          <span data-command-argv="" className="block">
            {unbrokenTokens(parts.command)}
          </span>
        </code>
      </div>
      <div className="flex items-center gap-2">
        <button type="button" onClick={() => copy(command)} className={BUTTON.regular.grey}>
          {t("common.copyCommand")}
        </button>
        <span role="status" className="text-small text-muted">
          {copyWords}
        </span>
      </div>
      <p className="break-words text-small text-muted">{t("needsPassword.after")}</p>
    </div>
  );
}
