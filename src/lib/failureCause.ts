/**
 * Why something failed, in a word a person knows, read off the words of
 * the program that failed (spec R10): the last lines a tool wrote to
 * stderr, or a message a check failed with. For an operation the core
 * reads them, as the tool wrote them, before a login is masked out of
 * them, and hands over only the cause (`Outcome.Failed.cause`,
 * `outcomeCause`; `history::failure_cause` in crates/banager-core, the
 * same rule).
 * Tools say the same few things when the network, the disk or a lock is in
 * the way -- Homebrew through curl, npm through Node, pip and pipx through
 * urllib3, cargo and uv through their HTTP clients, Ollama through Go's --
 * so a handful of their phrases cover most failures a person can do
 * something about. Anything else is null: the row keeps 「未能更新」, and
 * the log has the tool's own words.
 *
 * An operation's words are read further (`operationFailureCause`, r6
 * y3-batch): where none of those causes is named, for the ones only a
 * change meets -- a conflict, something missing, an app moved out of
 * Applications, a Mac the version does not support, a step that ran too
 * long. A lookup's are not: it changes nothing.
 *
 * Pure, and free of `t`: `FAILURE_CAUSE_KEYS` gives the words.
 */
import type { Fault, Outcome } from "./types";

/**
 * - `network`: the tool could not reach what it downloads from.
 * - `diskFull`: the disk has no room for it.
 * - `permission`: the system would not let it change a file.
 * - `busy`: something else holds the lock it needs -- another Homebrew,
 *   another cargo.
 * - `homebrewUpdating`: Homebrew was still updating its software list
 *   (`brew update`) when the time to wait for it ran out: a person need
 *   only wait, whatever made it slow.
 * - `needsPassword`: `sudo` wanted the Mac's password and had no way to
 *   ask for it. Banager runs every command with no terminal (stdin is
 *   /dev/null, crates/banager-core/src/runner/real.rs) and has no password
 *   window of its own; it only passes `SUDO_ASKPASS` on to Homebrew when
 *   that was already set (docs/what-we-run.md, "Passwords"). So a cask
 *   whose installer or uninstaller runs `sudo` stops there, and running the
 *   same command again from Banager stops there again: Homebrew resets
 *   sudo's remembered password before every command (`--reset-timestamp`
 *   in its brew.sh). The way on is Terminal, where sudo can ask.
 * - `passwordNotAccepted`: `sudo` did ask, in the password window
 *   `SUDO_ASKPASS` names, and got no password or a wrong one: the window
 *   was closed, or answered wrongly three times. A window was there, so
 *   this is not "can't be entered here"; trying again asks again, and
 *   Terminal is a way on too.
 *
 * Read only off an operation's words (`operationFailureCause`):
 *
 * - `conflict`: something already where it installs -- an app or a file of
 *   the same name, a link Homebrew would make, a package it conflicts with.
 * - `notFound`: something it needs is not there -- a package its source no
 *   longer has, a program it runs (`env: node: No such file or directory`).
 * - `appMissing`: a Homebrew cask's app is not where it was installed --
 *   moved to the Trash or deleted -- so Homebrew cannot back it up to
 *   update it ("It seems the App source '/Applications/…' is not there").
 * - `unsupported`: the version does not run on this Mac's macOS or chip.
 * - `timedOut`: a step of the tool's ran past the tool's own time limit;
 *   or, set by the core and never read off words, the read npm and uv
 *   take right before a command ran past Banager's own time for it, and
 *   the command was not started.
 * - `notLinked`: Homebrew installed a formula's new version but could not
 *   link it into its prefix ("The `brew link` step did not complete
 *   successfully"): an upgrade unlinks the old version first, so its
 *   commands may be gone from Terminal. Which file was in the way goes to
 *   stdout, not to the stderr lines read here.
 *
 * Never read off words, only kept by the history for a failure of
 * Banager's own (Rust `record_for`): `changed` -- what it was about to
 * change was no longer what the confirmation showed -- and `internal`.
 */
export type FailureCause =
  | "network"
  | "diskFull"
  | "permission"
  | "busy"
  | "homebrewUpdating"
  | "needsPassword"
  | "passwordNotAccepted"
  | "conflict"
  | "notFound"
  | "appMissing"
  | "unsupported"
  | "timedOut"
  | "notLinked"
  | "changed"
  | "internal";

/**
 * sudo's own fixed words for "I needed a password and got none" -- the
 * messages of sudo 1.9 (macOS 27 ships 1.9.17), as Homebrew passes them
 * on after "Error: Failure while executing; `/usr/bin/sudo …` exited with
 * 1. Here's the output:". Each only after "sudo: ", so a tool that merely
 * mentions a password -- a registry asking for credentials -- is not taken
 * for it. Not Homebrew's own "sudo is disabled by HOMEBREW_NO_SUDO.": that
 * is an account that may not use sudo at all, where typing a password in
 * Terminal would not help either.
 *
 * `NO_WAY_TO_ASK`, where there was no password window (`needsPassword`):
 *
 * - no terminal and no askpass helper: "a terminal is required to read the
 *   password; either use the -S option …", and before sudo 1.8.25 "no tty
 *   present and no askpass program specified";
 * - `-A` with an empty `SUDO_ASKPASS`: "no askpass program specified, try
 *   setting SUDO_ASKPASS".
 */
const NO_WAY_TO_ASK: RegExp[] = [
  /\bsudo: a terminal is required to read the password\b/i,
  /\bsudo: no tty present and no askpass program specified\b/i,
  /\bsudo: no askpass program specified\b/i,
];

/**
 * `ASKED_IN_A_WINDOW`, where the askpass helper `SUDO_ASKPASS` names did
 * ask (`passwordNotAccepted`): one the person closed, or answered with
 * nothing, "no password was provided"; one answered wrongly, "3 incorrect
 * password attempts".
 */
const ASKED_IN_A_WINDOW: RegExp[] = [
  /\bsudo: no password was provided\b/i,
  /\bsudo: \d+ incorrect password attempts?\b/i,
];

/**
 * The sudoers plugin's "a password is required", which follows either of
 * the above and on its own says only that none came: taken for
 * `needsPassword`, the way sudo with no terminal ends.
 */
const PASSWORD_REQUIRED = /\bsudo: a password is required\b/i;

/**
 * Which of the two password causes sudo's lines in `lines` name, or null:
 * no way to ask over a window that asked -- where sudo says it had none,
 * no window was there, whatever it said after -- and a window that asked
 * over a bare "a password is required".
 */
function sudoPasswordCause(lines: string[]): FailureCause | null {
  const any = (patterns: RegExp[]) => lines.some((line) => patterns.some((pattern) => pattern.test(line)));
  if (any(NO_WAY_TO_ASK)) return "needsPassword";
  if (any(ASKED_IN_A_WINDOW)) return "passwordNotAccepted";
  if (any([PASSWORD_REQUIRED])) return "needsPassword";
  return null;
}

/**
 * Each cause's phrases, after sudo's password lines (which `failureCause`
 * looks for first, on every line of the text it is given), first match wins, in this order: a
 * line about `brew update` running over its time is Homebrew's list, not the
 * network, though it says "timed out"; a full disk or a refused file is
 * named as such even where the tool goes on to say the download failed.
 *
 * Every phrase is several words or an error code with its boundaries, so a
 * package that happens to be called `timeout`, `connection-refused` or
 * `openssl` is never taken for one: "timed out" and not "timeout",
 * "Permission denied" with its space, `SSL` only as its own word.
 */
const PATTERNS: Array<[FailureCause, RegExp[]]> = [
  [
    "homebrewUpdating",
    [
      /\bbrew update\b.*(?:time[sd]? out|still running|超时)/i,
      /\bHomebrew\b.*\bstill updating\b/i,
    ],
  ],
  ["diskFull", [/No space left on device/i, /\bENOSPC\b/, /\bDisk quota exceeded\b/i, /\bdisk is full\b/i]],
  [
    "permission",
    [
      /Permission denied/i,
      /Operation not permitted/i,
      /\bEACCES\b/,
      /\bEPERM\b/,
      /\bare not writable by your user\b/i,
      /\bis not writable\b/i,
    ],
  ],
  [
    "busy",
    [
      /\balready locked\b/i,
      /\bhas already locked\b/i,
      /\bis locked by another process\b/i,
      /\banother active Homebrew\b/i,
      /\banother `?brew [a-z]+`? process is already (?:running|in progress)\b/i,
      /\bprocess is already (?:running|in progress)\b/i,
      /\bwaiting for file lock\b/i,
    ],
  ],
  [
    "network",
    [
      /\bcould(?: not|n't) resolve host\b/i,
      /\btimed out\b/i,
      /\bTimeout was reached\b/i,
      /\bconnection (?:refused|reset|aborted)\b/i,
      /\bremote end closed connection without response\b/i,
      /\bnetwork is unreachable\b/i,
      /\bno route to host\b/i,
      /\bTemporary failure in name resolution\b/i,
      /\bnodename nor servname provided\b/i,
      /\bno such host\b/i,
      /\bi\/o timeout\b/i,
      /\bdns error\b/i,
      /\berror sending request\b/i,
      /\bFailed to establish a new connection\b/i,
      /\bMax retries exceeded\b/i,
      /\bFailed to download\b/i,
      /\bfailed to fetch\b/i,
      /\bcurl: \((?:6|7|28|35|52|56)\)/,
      /\b(?:ENOTFOUND|EAI_AGAIN|ECONNREFUSED|ECONNRESET|ETIMEDOUT|ENETUNREACH)\b/,
      /\bnpm (?:ERR!|error) network\b/i,
      /\bSSL(?:Error|_ERROR\w*|\b)/,
      /\bssl certificate\b/i,
      /\bcertificate verify failed\b/i,
    ],
  ],
];

/**
 * The phrases only an operation's words are read for, after `PATTERNS`
 * found none (`operationFailureCause`), first match wins, in this order:
 * an app gone from Applications before anything else that says a source
 * is missing; a Mac the version does not support before the conflict or
 * the missing file that follows from it. Rust's `by_operation_cause`
 * (crates/banager-core/src/history/failure_cause.rs) is the same list.
 */
const OPERATION_PATTERNS: Array<[FailureCause, RegExp[]]> = [
  [
    "notLinked",
    [
      /\bThe `brew link` step did not complete successfully\b/i,
      /\bAn unexpected error occurred during the `brew link` step\b/i,
    ],
  ],
  ["appMissing", [/\bIt seems the App source '[^']*\/Applications\/[^']*' is not there\b/i]],
  [
    "unsupported",
    [
      /\bdoes not run on macOS versions\b/i,
      /\bis not available on macOS\b/i,
      /\bdepends on hardware architecture being one of\b/i,
      /\beither does not compile or function as expected on macOS\b/i,
      /\bEBADPLATFORM\b/,
      /\bis not a supported wheel on this platform\b/i,
    ],
  ],
  [
    "conflict",
    [
      /\bIt seems there is already an? [A-Za-z ]+ at\b/i,
      /\bconflicts with '/i,
      /\bconflicting formulae\b/i,
      /\bCould not symlink\b/i,
      /\balready exists\. You may want to remove it\b/i,
      /\bTo force the link and overwrite all conflicting files\b/i,
      /\bEEXIST\b/,
      /\bbinary `[^`]+` already exists in destination\b/i,
      /\bExecutable already exists\b/i,
    ],
  ],
  [
    "notFound",
    [
      /\bIt seems the [A-Za-z ]+ source '[^']*' is not there\b/i,
      /\bNo available (?:formula|cask)\b/i,
      /\bNo (?:cask|formula|formulae) with (?:this|the) name\b/i,
      /\b(?:Cask|Formula) '[^']+' is not installed\b/i,
      /\bNo such keg\b/i,
      /\bE404\b/,
      /\b404 Not Found\b/i,
      /\bis not in this registry\b/i,
      /\bcommand not found\b/i,
      /\bNo such file or directory\b/i,
      /\bENOENT\b/,
      /\bNo matching distribution found\b/i,
      /\bcould not find `[^`]+` in registry\b/i,
      /\bwas not found in the package registry\b/i,
      /\bfile does not exist\b/i,
    ],
  ],
  [
    "permission",
    [/\bFailed to quarantine\b/i, /\bFailed to release .* from quarantine\b/i, /\bCannot remove undeletable\b/i],
  ],
  ["timedOut", [/\bTimeout::Error\b/, /\bexecution expired\b/i, /\bdid not finish within\b/i]],
];

/** The first of `patterns`' causes a line of `lines` names, from the last line up. */
function lastNamed(lines: string[], patterns: Array<[FailureCause, RegExp[]]>): FailureCause | null {
  for (let index = lines.length - 1; index >= 0; index -= 1) {
    for (const [cause, phrases] of patterns) {
      if (phrases.some((pattern) => pattern.test(lines[index]))) return cause;
    }
  }
  return null;
}

/**
 * The cause `text` names, or null. Read from its last line up, so that
 * where a tool retried and then failed for another reason -- "Read timed
 * out. Retrying…" and then "Permission denied" -- the reason it stopped on
 * is the one given.
 *
 * But sudo's password lines first, on any line of `text`: where sudo got
 * no password the command stopped there, and whatever the tool said after
 * it -- a cask's rollback refused a file, "Permission denied" -- follows
 * from that, and would send a person to fix the wrong thing. For a failed
 * operation the core reads the same lines `Failed.summary` holds, only the
 * last five the tool wrote to stderr (`run_plan` in
 * crates/banager-core/src/adapters/mod.rs), as the tool wrote them: a
 * rollback that says more than that after sudo's lines pushes them out,
 * and then the cause is whatever those last lines say, or none.
 */
export function failureCause(text: string): FailureCause | null {
  const lines = text.split(/\r?\n/).filter((line) => line.trim() !== "");
  const password = sudoPasswordCause(lines);
  if (password !== null) return password;
  return lastNamed(lines, PATTERNS);
}

/**
 * Why an operation failed, by its tool's words: `failureCause` on all of
 * them first; only where that names none, the causes only a change meets
 * (`OPERATION_PATTERNS`), from the last line up. Two passes, so a network
 * failure a tool retried and then gave up on in other words -- pip's "No
 * matching distribution found" after its retries -- stays the network.
 * The core reads an operation's failure with the same rule (Rust
 * `operation_failure_cause`) and hands the window only the cause
 * (`Outcome.Failed.cause`); this is the mock backend's, and the shared
 * cases' check that the two agree.
 */
export function operationFailureCause(text: string): FailureCause | null {
  return failureCause(text) ?? lastNamed(text.split(/\r?\n/).filter((line) => line.trim() !== ""), OPERATION_PATTERNS);
}

/**
 * Whether the words for `cause` send a person to the tool's own words for
 * what it was -- which file was in the way, what was missing, what the
 * version needs -- so that the failure keeps the tool's first error line
 * beside the cause (`failureDetail`), as the history keeps it once the log
 * that had it is gone (Rust `FailureCause::keeps_its_line`; review of r6
 * y3-batch, finding 4).
 */
export function causeKeepsItsLine(cause: FailureCause): boolean {
  return cause === "conflict" || cause === "notFound" || cause === "unsupported";
}

/**
 * Whether one part of an address's path looks like a token a mirror or a
 * registry put there in place of a login (`https://host/<token>/simple/`):
 * 20 characters or more, only letters, digits, `-` and `_`, with both a
 * letter and a digit (Rust `looks_like_a_token`).
 */
function looksLikeAToken(part: string): boolean {
  return part.length >= 20 && /^[A-Za-z0-9_-]+$/.test(part) && /\d/.test(part) && /[A-Za-z]/.test(part);
}

/** How long a kept error line may be, the `…` of a cut one included: Rust's `DETAIL_CHARS`. */
const DETAIL_CHARS = 160;

/**
 * The one line of a failed tool's words that says what went wrong, where
 * no cause names it (Rust `failure_detail`, crates/banager-core/src/history/
 * mod.rs, which the history keeps): the first line that says it is an
 * error -- `Error:`, `error:`, `fatal:`, npm's `npm error` but for its
 * bookkeeping (`code`, `errno`, `path`, the log file) -- or, with none, the
 * last line, its label taken off; where it ends with a colon, with the line
 * after it. Masked as the history masks it: the escape codes that colour
 * it, any home folder (`/Users/<name>` as `~`, not `/Users/Shared`), a
 * login in an address, an address's query and fragment, a part of its
 * path that looks like a token (`looksLikeAToken`); cut to 160
 * characters. Null for words that are all blank. 「最近的更新记录」 says it
 * behind 「原因：」 for an update this window ran as for one the history
 * kept, so the two read the same.
 */
export function failureDetail(summary: string): string | null {
  const bookkeeping =
    /^npm (?:err!|error) (?:code|errno|syscall|path|dest|signal|command|cwd|\d{3}\s*$|a complete log|log files)/i;
  const lines = summary
    .split(/\r?\n/)
    .map((line) => line.replace(/\x1b\[[0-9;?]*[ -/]*[@-~]/g, "").trim())
    .filter((line) => line !== "" && line !== "[…]");
  const found = lines.findIndex((each) => /^(?:error|fatal|npm (?:err!|error))\b|^E:/i.test(each) && !bookkeeping.test(each));
  const index = found === -1 ? lines.length - 1 : found;
  const line = lines[index];
  if (line === undefined) return null;
  const unlabelled = line.replace(/^(?:(?:error|fatal)\b\s*(?:\[[^\]]*\])?\s*:?|npm (?:err!|error)\b|E:)\s*/i, "").trim();
  let text = unlabelled === "" ? line : unlabelled;
  // "An exception occurred within a child process:", and the reason on the
  // next line: the two together (review of r6 y3-batch, finding 5).
  const next = lines[index + 1];
  if (text.endsWith(":") && next !== undefined) text = `${text} ${next}`;
  const masked = text
    // `/Users/Shared` is no one's home (review of r6 y3-batch, finding 8).
    .replace(/\/Users\/([^/\s'"`]+)/g, (whole: string, name: string) => (name === "Shared" ? whole : "~"))
    .replace(/([A-Za-z][A-Za-z0-9+.-]*:\/\/)[^/\s@'"`]+@/g, "$1****@")
    .replace(/([A-Za-z][A-Za-z0-9+.-]*:\/\/[^\s?#'"`]*)[?#][^\s'"`]*/g, "$1")
    .replace(
      /([A-Za-z][A-Za-z0-9+.-]*:\/\/[^/\s'"`]+)(\/[^\s'"`]*)/g,
      (_whole: string, origin: string, rest: string) =>
        origin +
        rest
          .split("/")
          .map((part) => (looksLikeAToken(part) ? "****" : part))
          .join("/"),
    )
    .trim();
  const characters = [...masked];
  return characters.length <= DETAIL_CHARS ? masked : `${characters.slice(0, DETAIL_CHARS - 1).join("")}…`;
}

/**
 * The cause a failed lookup's words give (`failureCause`), where the
 * words for it hold for a lookup too: only `network`. The others are said
 * of a change -- 「没有权限修改它的文件」, a lock another operation holds, a
 * password -- and a lookup changes nothing: a lookup refused a file it
 * reads, or one a full disk stopped, says no cause rather than the wrong
 * one, and its row keeps the tool's own words behind "Show technical
 * details" (walk-2 review 1.3).
 */
export function lookupFailureCause(text: string): FailureCause | null {
  return failureCause(text) === "network" ? "network" : null;
}

/**
 * The cause of an operation's failure, from its outcome: a tool's own
 * words, classified by the core before a login was masked out of them
 * (`Failed.cause`; never read off `Failed.summary`, which the mask may have
 * changed); Banager's own `HomebrewStillUpdating`, which is the Homebrew
 * list by definition. Every other outcome -- one that worked, was
 * cancelled, or failed for a reason Banager words itself -- has none.
 */
export function outcomeCause(outcome: Outcome | null): FailureCause | null {
  if (outcome === null || typeof outcome === "string") return null;
  if ("Failed" in outcome) return outcome.Failed.cause ?? null;
  if ("BanagerFailed" in outcome) {
    const fault = outcome.BanagerFailed;
    return typeof fault !== "string" && "HomebrewStillUpdating" in fault ? "homebrewUpdating" : null;
  }
  return null;
}

/**
 * A failure of Banager's own as the history keeps it (Rust
 * `history::fault_result`): a cause for each, and none of the paths or
 * names a `Fault` carries. macOS's words for a program it would not start
 * are read like a tool's (`operationFailureCause`), with the line kept
 * where they name no cause or one that points at them. 「最近的更新记录」
 * says an update this window ran with it, so a line reads the same before
 * a restart and after it (review of r6 y3-batch, finding 6); a row and
 * the operation bar keep Banager's own words for a fault (`outcomeCause`).
 */
export function faultFailure(fault: Fault): { cause: FailureCause | null; detail: string | null } {
  if (fault === "Panicked" || fault === "Internal") return { cause: "internal", detail: null };
  if (fault === "HomebrewSettingsChanged" || fault === "ChangedSinceShown") return { cause: "changed", detail: null };
  if ("HomebrewStillUpdating" in fault) return { cause: "homebrewUpdating", detail: null };
  if ("ProgramMissing" in fault) return { cause: "notFound", detail: null };
  if ("SpawnFailed" in fault) {
    const cause = operationFailureCause(fault.SpawnFailed.detail);
    const detail = cause === null || causeKeepsItsLine(cause) ? failureDetail(fault.SpawnFailed.detail) : null;
    return { cause, detail };
  }
  return { cause: "changed", detail: null };
}

/**
 * The words for a cause: `word`, the few that stand where 「未能更新」
 * would -- a row's status, the operation bar -- and `next`, the one
 * sentence that says what to do about it, beside the word in the log and
 * under a failed check. `line` is the two together, as one sentence.
 * Spelled out, so the reachability test finds every key.
 */
export const FAILURE_CAUSE_KEYS: Record<FailureCause, { word: string; next: string; line: string }> = {
  network: {
    word: "failure.cause.network",
    next: "failure.next.network",
    line: "failure.line.network",
  },
  diskFull: {
    word: "failure.cause.diskFull",
    next: "failure.next.diskFull",
    line: "failure.line.diskFull",
  },
  permission: {
    word: "failure.cause.permission",
    next: "failure.next.permission",
    line: "failure.line.permission",
  },
  busy: {
    word: "failure.cause.busy",
    next: "failure.next.busy",
    line: "failure.line.busy",
  },
  homebrewUpdating: {
    word: "failure.cause.homebrewUpdating",
    next: "failure.next.homebrewUpdating",
    line: "failure.line.homebrewUpdating",
  },
  needsPassword: {
    word: "failure.cause.needsPassword",
    next: "failure.next.needsPassword",
    line: "failure.line.needsPassword",
  },
  passwordNotAccepted: {
    word: "failure.cause.passwordNotAccepted",
    next: "failure.next.passwordNotAccepted",
    line: "failure.line.passwordNotAccepted",
  },
  conflict: {
    word: "failureMore.cause.conflict",
    next: "failureMore.next.conflict",
    line: "failureMore.line.conflict",
  },
  notFound: {
    word: "failureMore.cause.notFound",
    next: "failureMore.next.notFound",
    line: "failureMore.line.notFound",
  },
  appMissing: {
    word: "failureMore.cause.appMissing",
    next: "failureMore.next.appMissing",
    line: "failureMore.line.appMissing",
  },
  unsupported: {
    word: "failureMore.cause.unsupported",
    next: "failureMore.next.unsupported",
    line: "failureMore.line.unsupported",
  },
  timedOut: {
    word: "failureMore.cause.timedOut",
    next: "failureMore.next.timedOut",
    line: "failureMore.line.timedOut",
  },
  notLinked: {
    word: "failureMore.cause.notLinked",
    next: "failureMore.next.notLinked",
    line: "failureMore.line.notLinked",
  },
  changed: {
    word: "failureMore.cause.changed",
    next: "failureMore.next.changed",
    line: "failureMore.line.changed",
  },
  internal: {
    word: "failureMore.cause.internal",
    next: "failureMore.next.internal",
    line: "failureMore.line.internal",
  },
};
