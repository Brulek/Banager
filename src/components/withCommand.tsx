/**
 * A sentence with a command set into it as code. Shared by the Updates
 * page's pinned rows, the Installed page's pinned rows and the uninstall
 * dialog's pinned refusal, so all three show an unpin command the same way.
 */
import type { ReactNode } from "react";

/**
 * What `{{command}}` is translated to first, so the sentence can be cut
 * around it: a private-use character, which neither en.json nor
 * zh-CN.json contains. The command itself never passes through `t`, so
 * nothing in it can be taken for this.
 */
export const COMMAND_SLOT = "";

/**
 * A translated sentence with `command` set into it as code rather than as
 * a word of the sentence, so a reader who does not use Terminal can see
 * where the command starts and stops -- inline, "run brew unpin glib in
 * Terminal;" invites copying "in Terminal;" along with it. `select-all`
 * makes one click select the whole command and nothing else.
 *
 * A translation that does not hold `COMMAND_SLOT` exactly once (it
 * dropped or repeated `{{command}}`) gets the command back as plain text
 * in every place the slot is, rather than a `<code>` in the wrong one.
 */
export function withCommand(sentence: string, command: string): ReactNode {
  const parts = sentence.split(COMMAND_SLOT);
  if (parts.length !== 2) return parts.join(command);
  return (
    <>
      {parts[0]}
      <code className="select-all rounded bg-[var(--color-hover)] px-1 font-mono text-[var(--color-foreground)]">
        {command}
      </code>
      {parts[1]}
    </>
  );
}
