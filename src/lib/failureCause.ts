/**
 * Why something failed, in a word a person knows, read off the words of
 * the program that failed (spec R10): the last lines a tool wrote to
 * stderr (`Outcome.Failed.summary`), or a message a check failed with.
 * Tools say the same few things when the network, the disk or a lock is in
 * the way -- Homebrew through curl, npm through Node, pip and pipx through
 * urllib3, cargo and uv through their HTTP clients, Ollama through Go's --
 * so a handful of their phrases cover most failures a person can do
 * something about. Anything else is null: the row keeps 「未能更新」, and
 * the log has the tool's own words.
 *
 * Pure, and free of `t`: `failureCauseKeys` gives the words.
 */
import type { Outcome } from "./types";

/**
 * - `network`: the tool could not reach what it downloads from.
 * - `diskFull`: the disk has no room for it.
 * - `permission`: the system would not let it change a file.
 * - `busy`: something else holds the lock it needs -- another Homebrew,
 *   another cargo.
 * - `homebrewUpdating`: Homebrew was still updating its software list
 *   (`brew update`) when the time to wait for it ran out: a person need
 *   only wait, whatever made it slow.
 */
export type FailureCause = "network" | "diskFull" | "permission" | "busy" | "homebrewUpdating";

/**
 * Each cause's phrases, first match wins, in this order: a line about
 * `brew update` running over its time is Homebrew's list, not the
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
      /\bconnection (?:refused|reset)\b/i,
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
 * The cause `text` names, or null. Read from its last line up, so that
 * where a tool retried and then failed for another reason -- "Read timed
 * out. Retrying…" and then "Permission denied" -- the reason it stopped on
 * is the one given.
 */
export function failureCause(text: string): FailureCause | null {
  const lines = text.split(/\r?\n/).filter((line) => line.trim() !== "");
  for (let index = lines.length - 1; index >= 0; index -= 1) {
    for (const [cause, patterns] of PATTERNS) {
      if (patterns.some((pattern) => pattern.test(lines[index]))) return cause;
    }
  }
  return null;
}

/**
 * The cause of an operation's failure, from its outcome: a tool's own
 * words (`Failed.summary`), classified; Canager's own `HomebrewStillUpdating`,
 * which is the Homebrew list by definition. Every other outcome -- one
 * that worked, was cancelled, or failed for a reason Canager words itself
 * -- has none.
 */
export function outcomeCause(outcome: Outcome | null): FailureCause | null {
  if (outcome === null || typeof outcome === "string") return null;
  if ("Failed" in outcome) return failureCause(outcome.Failed.summary);
  if ("CanagerFailed" in outcome) {
    const fault = outcome.CanagerFailed;
    return typeof fault !== "string" && "HomebrewStillUpdating" in fault ? "homebrewUpdating" : null;
  }
  return null;
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
};
