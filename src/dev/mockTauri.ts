/**
 * The browser preview's stand-in for "@tauri-apps/api/core"
 * (docs/ui-preview.md). vite.config.ts aliases that module to this file in
 * `vite --mode mock` (`pnpm dev:mock`) and in no other mode, so
 * src/lib/api.ts -- the one module that imports it -- talks to the mock
 * backend in ./mockBackend.ts instead of a Tauri window that is not there.
 * `pnpm build`, `pnpm tauri dev` and `pnpm tauri build` never resolve the
 * alias, and nothing else imports this file, so it is never bundled.
 *
 * It exports what api.ts imports -- `invoke`, `Channel` and the
 * `InvokeArgs` type -- typed against the real module, so a change to how
 * api.ts calls Tauri fails `pnpm typecheck` here too.
 */
import type {
  Channel as TauriChannel,
  InvokeArgs,
  invoke as tauriInvoke,
} from "@tauri-apps/api/core";
import { useUiStore } from "../store/ui";
import { createMockBackend } from "./mockBackend";
import { parseScenario } from "./scenario";

export type { InvokeArgs };

/** In every preview page's console, and in no production bundle (docs/ui-preview.md). */
export const MOCK_MARKER = "canager-ui-preview-mock";

/** Tauri's `Channel`, as far as api.ts uses it: the backend calls `onmessage`. */
export class Channel<T = unknown> implements Pick<TauriChannel<T>, "onmessage"> {
  onmessage: (response: T) => void;

  constructor(onmessage?: (response: T) => void) {
    this.onmessage = onmessage ?? (() => {});
  }
}

const { scenario, problems } = parseScenario(window.location.search);
const backend = createMockBackend(scenario);

if (scenario.page !== null) {
  useUiStore.setState({ page: scenario.page });
}

console.info(
  `[${MOCK_MARKER}] Browser preview with a mock backend (docs/ui-preview.md): ` +
    `state=${scenario.state} outcome=${scenario.outcome} scan=${scenario.scan}`,
);
for (const problem of problems) {
  console.warn(`[${MOCK_MARKER}] ${problem}`);
}

export function invoke<T>(cmd: string, args?: InvokeArgs): Promise<T> {
  return backend.invoke(cmd, args) as Promise<T>;
}

// Callable exactly as the real `invoke` is.
invoke satisfies typeof tauriInvoke;
