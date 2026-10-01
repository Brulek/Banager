/**
 * What the browser preview's `get_system_facts` answers
 * (docs/ui-preview.md): the facts `crates/banager-core/src/diagnostics.rs`
 * reads on a real Mac -- macOS's version, the chip, the `PATH` folders and
 * each source's program -- for the pretend one, every path under its home
 * folder (`HOME` in ./mockData.ts) written as `~`, as Rust writes it.
 * Dev-only, like everything in src/dev.
 */
import type { ManagerInstance, SystemFacts } from "../lib/types";
import { HOME, withHomeAsTilde } from "./mockData";
import type { ScenarioPath } from "./scenario";

/** The pretend Mac's `PATH`, as a login shell with Homebrew, pipx, uv and Cargo set up leaves it. */
const PATH_DIRS = [
  "/opt/homebrew/bin",
  "/opt/homebrew/sbin",
  `${HOME}/.local/bin`,
  `${HOME}/.cargo/bin`,
  "/usr/local/bin",
  "/System/Cryptexes/App/usr/bin",
  "/usr/bin",
  "/bin",
  "/usr/sbin",
  "/sbin",
];

/** The `PATH` an app opened from Finder starts with, when the login shell's could not be read (`?path=default`). */
const DEFAULT_PATH_DIRS = ["/usr/bin", "/bin", "/usr/sbin", "/sbin"];

/** A `PATH` folder in Documents, a protected place, which a round leaves unread (`?path=unread`). */
const PROTECTED_DIR = `${HOME}/Documents/bin`;

/**
 * `get_system_facts` for the sources the preview has detected. `path` is
 * `?path=`; `refreshed`, whether a round has committed: `path_folders` is
 * what the last one made of the `PATH` folders (`Session::path_folders`),
 * none before one has, nor when the login shell's `PATH` was not read.
 */
export function mockSystemFacts(
  instances: readonly ManagerInstance[],
  path: ScenarioPath = "read",
  refreshed = false,
): SystemFacts {
  const dirs =
    path === "default" ? DEFAULT_PATH_DIRS : path === "unread" ? [...PATH_DIRS, PROTECTED_DIR] : PATH_DIRS;
  return {
    macos_version: "27.0",
    chip: "Apple M2 Pro",
    arch: "aarch64",
    login_path: path !== "default",
    path_dirs: dirs.map(withHomeAsTilde),
    sources: instances.map((instance) => ({ instance_id: instance.id, exe_path: withHomeAsTilde(instance.exe_path) })),
    path_folders:
      !refreshed || path === "default"
        ? null
        : { read: PATH_DIRS.length, unread: path === "unread" ? [withHomeAsTilde(PROTECTED_DIR)] : [] },
  };
}
