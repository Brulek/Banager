/**
 * What the browser preview's `get_system_facts` answers
 * (docs/ui-preview.md): the facts `crates/banager-core/src/diagnostics.rs`
 * reads on a real Mac -- macOS's version, the chip, the `PATH` folders and
 * each source's program -- for the pretend one, every path under its home
 * folder (`HOME` in ./mockData.ts) written as `~`, as Rust writes it.
 * Dev-only, like everything in src/dev.
 */
import type { ManagerInstance, SystemFacts } from "../lib/types";
import { HOME } from "./mockData";

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

/** `path` with the pretend home folder as `~` (`shown_path` in diagnostics.rs). */
function shown(path: string): string {
  if (path === HOME) return "~";
  return path.startsWith(`${HOME}/`) ? `~${path.slice(HOME.length)}` : path;
}

/** `get_system_facts` for the sources the preview has detected. */
export function mockSystemFacts(instances: readonly ManagerInstance[]): SystemFacts {
  return {
    macos_version: "27.0",
    chip: "Apple M2 Pro",
    arch: "aarch64",
    login_path: true,
    path_dirs: PATH_DIRS.map(shown),
    sources: instances.map((instance) => ({ instance_id: instance.id, exe_path: shown(instance.exe_path) })),
  };
}
