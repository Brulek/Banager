import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import {
  FAILURE_CAUSE_KEYS,
  failureCause,
  failureDetail,
  lookupFailureCause,
  operationFailureCause,
  outcomeCause,
  type FailureCause,
} from "./failureCause";
import en from "../i18n/en.json";
import zhCN from "../i18n/zh-CN.json";
import zhHant from "../i18n/zh-Hant.json";

/**
 * What tools really print when they fail, as the last lines of their
 * stderr (`Outcome.Failed.summary`) -- Homebrew, npm, pip and pipx, uv,
 * cargo, rustup and Ollama -- and the cause each should be read as.
 */
const CASES: Array<[string, string, FailureCause | null]> = [
  [
    "brew: curl could not resolve the bottle's host",
    [
      "curl: (6) Could not resolve host: ghcr.io",
      "",
      'Error: git: Failed to download resource "git (2.55.1)"',
      "Download failed: https://ghcr.io/v2/homebrew/core/git/blobs/sha256:0c1d",
    ].join("\n"),
    "network",
  ],
  [
    "brew: a download that timed out",
    "curl: (28) Operation timed out after 300000 milliseconds with 0 bytes received",
    "network",
  ],
  [
    "brew: a folder another user owns",
    "Error: Permission denied @ apply2files - /usr/local/lib/node_modules/npm/node_modules/@colors/colors/lib/custom/zalgo.js",
    "permission",
  ],
  [
    "brew: another brew holds the keg's lock",
    [
      "Error: A `brew upgrade git` process has already locked /opt/homebrew/Cellar/git.",
      "Please wait for it to finish or terminate it to continue.",
    ].join("\n"),
    "busy",
  ],
  [
    "brew: another brew update",
    "Error: Another active Homebrew update process is already in progress.\nPlease wait for it to finish or terminate it to continue.",
    "busy",
  ],
  [
    "brew: the disk is full",
    "Error: No space left on device @ rb_sysopen - /Users/me/Library/Caches/Homebrew/downloads/1f2e--ffmpeg--8.0.bottle.tar.gz",
    "diskFull",
  ],
  [
    "npm: no DNS",
    [
      "npm error code ENOTFOUND",
      "npm error syscall getaddrinfo",
      "npm error errno ENOTFOUND",
      "npm error network request to https://registry.npmjs.org/typescript failed, reason: getaddrinfo ENOTFOUND registry.npmjs.org",
      "npm error network This is a problem related to network connectivity.",
    ].join("\n"),
    "network",
  ],
  [
    "npm: a global folder it cannot write",
    [
      "npm error code EACCES",
      "npm error syscall mkdir",
      "npm error path /usr/local/lib/node_modules/typescript",
      "npm error errno -13",
      "npm error Error: EACCES: permission denied, mkdir '/usr/local/lib/node_modules/typescript'",
    ].join("\n"),
    "permission",
  ],
  [
    "npm: the disk is full",
    "npm error code ENOSPC\nnpm error syscall write\nnpm error errno -28\nnpm error nospc ENOSPC: no space left on device, write",
    "diskFull",
  ],
  [
    "pipx: pip retried offline, then found no version",
    [
      "WARNING: Retrying (Retry(total=4, connect=None, read=None, redirect=None, status=None)) after connection broken by 'NewConnectionError('<pip._vendor.urllib3.connection.HTTPSConnection object at 0x1045c2f90>: Failed to establish a new connection: [Errno 8] nodename nor servname provided, or not known')': /simple/httpie/",
      "ERROR: Could not find a version that satisfies the requirement httpie (from versions: none)",
      "ERROR: No matching distribution found for httpie",
    ].join("\n"),
    "network",
  ],
  [
    "pip: a read that timed out",
    "pip._vendor.urllib3.exceptions.ReadTimeoutError: HTTPSConnectionPool(host='files.pythonhosted.org', port=443): Read timed out.",
    "network",
  ],
  [
    "pip: a certificate it could not check",
    "Could not fetch URL https://pypi.org/simple/httpie/: There was a problem confirming the ssl certificate: HTTPSConnectionPool(host='pypi.org', port=443): Max retries exceeded with url: /simple/httpie/ (Caused by SSLError(SSLCertVerificationError(1, '[SSL: CERTIFICATE_VERIFY_FAILED] certificate verify failed')))",
    "network",
  ],
  [
    "uv: dns error under its fetch",
    [
      "error: Failed to fetch: `https://pypi.org/simple/ruff/`",
      "  Caused by: Request failed after 3 retries",
      "  Caused by: error sending request for url (https://pypi.org/simple/ruff/)",
      "  Caused by: dns error: failed to lookup address information: nodename nor servname provided, or not known",
    ].join("\n"),
    "network",
  ],
  [
    "cargo: the index's host could not be resolved",
    [
      "warning: spurious network error (3 tries remaining): [6] Couldn't resolve host name (Could not resolve host: index.crates.io)",
      "error: failed to get `tokei` as a dependency of package `tokei v13.0.1`",
      "Caused by:",
      "  [6] Couldn't resolve host name (Could not resolve host: index.crates.io)",
    ].join("\n"),
    "network",
  ],
  [
    "cargo: a registry folder it may not write",
    "error: failed to create directory `/Users/me/.cargo/registry/cache/index.crates.io-1949cf8c6b5b557f`\n\nCaused by:\n  Permission denied (os error 13)",
    "permission",
  ],
  [
    "cargo: another cargo holds the package cache",
    "    Blocking waiting for file lock on package cache",
    "busy",
  ],
  [
    "rustup: a request that timed out",
    [
      "error: could not download file from 'https://static.rust-lang.org/dist/channel-rust-stable.toml.sha256'",
      "error: caused by: failed to make network request",
      "error: caused by: operation timed out",
    ].join("\n"),
    "network",
  ],
  [
    "ollama: no such host",
    'Error: pull model manifest: Get "https://registry.ollama.ai/v2/library/llama3/manifests/latest": dial tcp: lookup registry.ollama.ai: no such host',
    "network",
  ],
  [
    "macOS refused a move to the Trash",
    "“claude” couldn’t be moved to the Trash because you don’t have permission to access it. Operation not permitted",
    "permission",
  ],
  [
    "the last reason wins: a retry that timed out, then a refused file",
    "WARNING: Read timed out. Retrying…\nERROR: Could not install packages due to an OSError: [Errno 13] Permission denied: '/Library/Python/3.9/site-packages/six.py'",
    "permission",
  ],
  [
    "brew: a cask's uninstaller ran sudo with no terminal to ask in",
    [
      "==> Removing launchctl service com.example.helper",
      "Error: Failure while executing; `/usr/bin/sudo -u root -E LOGNAME=me USER=me USERNAME=me -- /bin/launchctl remove com.example.helper` exited with 1. Here's the output:",
      "sudo: a terminal is required to read the password; either use the -S option to read from standard input or configure an askpass helper",
      "sudo: a password is required",
    ].join("\n"),
    "needsPassword",
  ],
  [
    "brew: an older sudo's words for the same",
    "sudo: no tty present and no askpass program specified",
    "needsPassword",
  ],
  [
    "brew: the password window SUDO_ASKPASS names was closed",
    "sudo: no password was provided\nsudo: a password is required",
    "passwordNotAccepted",
  ],
  [
    "brew: SUDO_ASKPASS set but empty",
    "sudo: no askpass program specified, try setting SUDO_ASKPASS",
    "needsPassword",
  ],
  [
    "brew: the password window answered wrongly three times",
    "sudo: 3 incorrect password attempts",
    "passwordNotAccepted",
  ],
  [
    "sudo's password line wins over a refused file the rollback hit after it",
    [
      "sudo: a terminal is required to read the password; either use the -S option to read from standard input or configure an askpass helper",
      "sudo: a password is required",
      "==> Purging files for version 2.4.1 of Cask example",
      "Error: Permission denied @ apply2files - /Applications/Example.app/Contents/Info.plist",
    ].join("\n"),
    "needsPassword",
  ],
  // Not causes: names and words that only look like them.
  ["a formula named timeout", 'Error: No available formula with the name "timeout".', null],
  [
    "an npm package named connection-refused",
    "npm error 404 Not Found - GET https://registry.npmjs.org/connection-refused - Not found",
    null,
  ],
  ["openssl's bottle", "Error: openssl@3: the bottle needs the Apple Command Line Tools to be installed.", null],
  [
    "a crate named permission-denied that does not compile",
    "error: could not compile `permission-denied` (lib) due to 2 previous errors",
    null,
  ],
  [
    "a pipx package named timeout-decorator for another Python",
    "ERROR: Package 'timeout-decorator' requires a different Python: 3.14.2 not in '<3.12'",
    null,
  ],
  ["a checksum that did not match", "Error: ffmpeg: SHA256 mismatch\nExpected: 0c1d\n  Actual: 9f8e", null],
  ["nothing at all", "", null],
  [
    "a registry that asks for a password is not sudo's",
    "npm error code E401\nnpm error Unable to authenticate, a password is required",
    null,
  ],
  [
    "an account that may not use sudo at all",
    "Error: Failure while executing; `/usr/bin/sudo -E -- /bin/rm -f -- /Library/LaunchDaemons/com.example.plist` exited with 1. Here's the output:\nsudo is disabled by HOMEBREW_NO_SUDO.",
    null,
  ],
];

describe("failureCause", () => {
  it.each(CASES)("%s", (_, text, cause) => {
    expect(failureCause(text)).toBe(cause);
  });

  it("covers every cause with real output, and says nothing of what it cannot tell", () => {
    const found = new Set(CASES.map(([, , cause]) => cause));
    expect(found).toEqual(
      new Set(["network", "diskFull", "permission", "busy", "needsPassword", "passwordNotAccepted", null]),
    );
    expect(CASES.filter(([, , cause]) => cause === null).length).toBeGreaterThanOrEqual(6);
  });

  it("takes no way to ask over a window that asked, and a bare 'a password is required' for no way to ask", () => {
    expect(
      failureCause(
        "sudo: a terminal is required to read the password; either use the -S option to read from standard input or configure an askpass helper\nsudo: no password was provided\nsudo: a password is required",
      ),
    ).toBe("needsPassword");
    expect(failureCause("sudo: a password is required")).toBe("needsPassword");
  });

  it("finds sudo's lines only while they are in the text: a long rollback after them in a five-line summary hides them", () => {
    const sudo = [
      "sudo: a terminal is required to read the password; either use the -S option to read from standard input or configure an askpass helper",
      "sudo: a password is required",
    ];
    const rollback = [
      "==> Purging files for version 2.4.1 of Cask example",
      "==> Moving App 'Example.app' back to '/Applications/Example.app'",
      "==> Removing App '/Applications/Example.app'",
      "==> Backing App 'Example.app' up",
      "Error: Permission denied @ apply2files - /Applications/Example.app/Contents/Info.plist",
    ];
    // What `run_plan` keeps: the last five lines of stderr.
    const lastFive = (lines: string[]) => lines.slice(-5).join("\n");
    expect(failureCause(lastFive([...sudo, ...rollback.slice(-3)]))).toBe("needsPassword");
    expect(failureCause(lastFive([...sudo, ...rollback]))).toBe("permission");
  });

  it("reads a check that ran over waiting for brew update as Homebrew's list, not the network", () => {
    expect(failureCause("brew update timed out after 60 s")).toBe("homebrewUpdating");
    expect(failureCause("brew update 在60秒后超时")).toBe("homebrewUpdating");
    // Any other command that timed out is the network's.
    expect(failureCause("pipx list --json timed out after 60 s")).toBe("network");
  });
});

describe("outcomeCause", () => {
  it("takes a tool's failure's cause from the outcome, where the core read it", () => {
    expect(outcomeCause({ Failed: { exit_code: 1, summary: 'Error: git: Failed to download resource "git (2.55.1)"', cause: "network" } })).toBe(
      "network",
    );
    expect(outcomeCause({ Failed: { exit_code: 1, summary: "", cause: null } })).toBeNull();
  });

  it("never reads the cause off the summary, which a masked login may have changed (re-check 2's N1)", () => {
    // A proxy password `pass`, masked inside sudo's "password": the words
    // are gone from the summary, and the cause, read before the mask, says
    // it all the same.
    const masked =
      "sudo: a terminal is required to read the ****word; either use the -S option to read from standard input or configure an ask**** helper\nsudo: a ****word is required";
    expect(failureCause(masked)).toBeNull();
    expect(outcomeCause({ Failed: { exit_code: 1, summary: masked, cause: "needsPassword" } })).toBe("needsPassword");
    // And words in the summary the core did not find there say nothing.
    expect(outcomeCause({ Failed: { exit_code: 1, summary: "sudo: a password is required", cause: null } })).toBeNull();
  });

  it("knows Banager's own wait for brew update, and no other fault", () => {
    expect(outcomeCause({ BanagerFailed: { HomebrewStillUpdating: { minutes: 10 } } })).toBe("homebrewUpdating");
    expect(outcomeCause({ BanagerFailed: "Internal" })).toBeNull();
    expect(outcomeCause({ BanagerFailed: { ProgramMissing: { program: "/opt/homebrew/bin/brew" } } })).toBeNull();
  });

  it("gives nothing for an outcome that is not a failure", () => {
    expect(outcomeCause(null)).toBeNull();
    expect(outcomeCause("Succeeded")).toBeNull();
    expect(outcomeCause("Cancelled")).toBeNull();
    expect(outcomeCause({ NeedsAttention: "UnchangedAfterUpgrade" })).toBeNull();
  });
});

describe("FAILURE_CAUSE_KEYS", () => {
  /** A dotted key's string in a locale, or undefined. */
  const lookup = (locale: unknown, key: string): unknown =>
    key.split(".").reduce<unknown>((node, part) => (node as Record<string, unknown> | undefined)?.[part], locale);

  it("has a word, a next step and a line for each cause, in both languages", () => {
    for (const keys of Object.values(FAILURE_CAUSE_KEYS)) {
      for (const key of [keys.word, keys.next, keys.line]) {
        expect(typeof lookup(zhCN, key), key).toBe("string");
        expect(typeof lookup(en, key), key).toBe("string");
      }
    }
  });

  it("says the four causes as the spec words them, and a next step as one sentence", () => {
    expect(lookup(zhCN, FAILURE_CAUSE_KEYS.passwordNotAccepted.word)).toBe("密码未被接受");
    expect(lookup(zhCN, FAILURE_CAUSE_KEYS.network.word)).toBe("网络连接失败");
    expect(lookup(zhCN, FAILURE_CAUSE_KEYS.diskFull.word)).toBe("磁盘空间不足");
    expect(lookup(zhCN, FAILURE_CAUSE_KEYS.permission.word)).toBe("没有权限");
    expect(lookup(zhCN, FAILURE_CAUSE_KEYS.busy.word)).toBe("另一个操作正在进行");
    expect(lookup(zhCN, FAILURE_CAUSE_KEYS.homebrewUpdating.line)).toBe("Homebrew正在联网查找新版本，请稍后再试。");
    expect(lookup(zhCN, FAILURE_CAUSE_KEYS.needsPassword.word)).toBe("需要输入密码");
    for (const keys of Object.values(FAILURE_CAUSE_KEYS)) {
      const next = lookup(zhCN, keys.next) as string;
      expect(next.match(/。/g)?.length, keys.next).toBe(1);
      expect(next.endsWith("。"), keys.next).toBe(true);
      // No command names: a person's words.
      expect(lookup(zhCN, keys.line) as string, keys.line).not.toMatch(/brew update|curl|npm|pip/);
    }
  });
});

describe("operationFailureCause", () => {
  it("reads an operation's words as Rust does: the cases operation_failure_cause is tested on", () => {
    // r6 y3-batch, finding 3. The first causes on all the words, then the
    // ones only a change meets, from the last line up.
    const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
    const cases: Array<{ name: string; text: string; cause: FailureCause | null }> = JSON.parse(
      readFileSync(path.join(root, "crates/banager-core/src/history/operation_cause_cases.json"), "utf-8"),
    );
    expect(cases.length).toBeGreaterThanOrEqual(30);
    for (const { name, text, cause } of cases) {
      expect(operationFailureCause(text), name).toBe(cause);
    }
  });

  it("leaves a lookup's reading as it was", () => {
    expect(failureCause("Error: It seems the App source '/Applications/Foo.app' is not there.")).toBeNull();
    expect(failureCause("env: node: No such file or directory")).toBeNull();
    expect(lookupFailureCause("npm error code E404\nnpm error 404 Not Found")).toBeNull();
  });

  it("words the causes only a change meets in every language, each next step one sentence", () => {
    const lookup = (locale: unknown, key: string): unknown =>
      key.split(".").reduce<unknown>((node, part) => (node as Record<string, unknown> | undefined)?.[part], locale);
    const added: FailureCause[] = ["conflict", "notFound", "appMissing", "unsupported", "timedOut", "changed", "internal"];
    for (const cause of added) {
      for (const key of Object.values(FAILURE_CAUSE_KEYS[cause])) {
        for (const locale of [en, zhCN, zhHant]) expect(typeof lookup(locale, key), key).toBe("string");
      }
    }
    expect(lookup(zhCN, FAILURE_CAUSE_KEYS.appMissing.word)).toBe("App已不在原来的位置");
    expect(lookup(zhCN, FAILURE_CAUSE_KEYS.appMissing.next)).toContain("废纸篓");
    expect(lookup(zhHant, FAILURE_CAUSE_KEYS.appMissing.next)).toContain("垃圾桶");
  });
});

describe("failureDetail", () => {
  // The same lines as Rust's failure_detail tests
  // (test_every_failure_keeps_a_cause_and_one_no_cause_names_keeps_its_first_error_line,
  // test_a_kept_error_line_has_the_home_folder_logins_and_queries_masked_and_is_short),
  // so a failure says the same before a restart and after it.
  it("keeps the first line that says what went wrong, its label off, or the last line", () => {
    expect(
      failureDetail("==> Purging files for version 0.2.0 of Cask claudebar\nError: SHA256 mismatch\nExpected: 1f2e\n  Actual: 3a4b"),
    ).toBe("SHA256 mismatch");
    expect(
      failureDetail(
        "npm error code ETARGET\nnpm error notarget No matching version found for typescript@99.\nnpm error A complete log of this run can be found in: /Users/me/.npm/_logs/x.log",
      ),
    ).toBe("notarget No matching version found for typescript@99.");
    expect(failureDetail("Building wheel\nsomething odd happened")).toBe("something odd happened");
    expect(failureDetail("Exec format error (os error 8)")).toBe("Exec format error (os error 8)");
    expect(failureDetail("  \n")).toBeNull();
  });

  it("masks the home folder, a login, a query and a fragment, and cuts it to 160 characters", () => {
    expect(
      failureDetail(
        "Error: \u001b[31mcannot read\u001b[0m /Users/brulek/Library/Caches/x and /Users/other/y via https://me:secret@mirror.example/simple/?token=abc#frag",
      ),
    ).toBe("cannot read ~/Library/Caches/x and ~/y via https://****@mirror.example/simple/");
    const long = failureDetail(`Error: ${"x".repeat(400)}`) ?? "";
    expect([...long].length).toBe(160);
    expect(long.endsWith("…")).toBe(true);
  });
});

describe("lookupFailureCause", () => {
  it("reads a lookup's words as the network where the cases Rust's transient flag is tested on say, and lists every difference", () => {
    // `transient` is Rust's `says_network_failed` (crates/banager-core/src/
    // adapters/mod.rs): whether a later check can get past it. `network`
    // is what this window shows. Where they differ, the case says why.
    const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
    const cases: Array<{ name: string; text: string; transient: boolean; network: boolean; why?: string }> =
      JSON.parse(
        readFileSync(path.join(root, "crates/banager-core/src/adapters/network_words_cases.json"), "utf-8"),
      );
    expect(cases.length).toBeGreaterThanOrEqual(30);
    for (const { name, text, transient, network, why } of cases) {
      expect(lookupFailureCause(text) === "network", name).toBe(network);
      expect(why !== undefined, name).toBe(transient !== network);
    }
  });
});
