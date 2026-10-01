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
export const MOCK_MARKER = "banager-ui-preview-mock";

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
    `state=${scenario.state} outcome=${scenario.outcome} scan=${scenario.scan} sizes=${scenario.sizes}`,
);
for (const problem of problems) {
  console.warn(`[${MOCK_MARKER}] ${problem}`);
}

/**
 * Replies reach the page one message each, after the task that asked, as
 * over Tauri's IPC: a `MessageChannel`, which the browser does not slow
 * down the way it does a chain of `setTimeout(0)`s.
 */
const replies: Array<() => void> = [];
const replyChannel = new MessageChannel();
replyChannel.port1.onmessage = () => replies.shift()?.();

function inATaskOfItsOwn(): Promise<void> {
  return new Promise((resolve) => {
    replies.push(resolve);
    replyChannel.port2.postMessage(null);
  });
}

/**
 * A command, answered as the app's are: in a task of its own. The mock
 * backend answers at once when it has nothing to wait for; handed back
 * as it is, a page that awaits one answer after another -- Update all
 * submitting 120 tools, one after the other -- would run them all in the
 * task that asked, drawing no frame until the last, which the app, whose
 * every answer comes back over IPC, never does.
 */
export function invoke<T>(cmd: string, args?: InvokeArgs): Promise<T> {
  return backend.invoke(cmd, args).then(
    async (value) => {
      await inATaskOfItsOwn();
      return value as T;
    },
    async (error: unknown) => {
      await inATaskOfItsOwn();
      throw error;
    },
  );
}

// Callable exactly as the real `invoke` is.
invoke satisfies typeof tauriInvoke;
