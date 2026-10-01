import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, waitFor, within } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "./test/setup";
import App from "./App";
import type { InstalledArtifact, ManagerInstance, Settings, Sizes, Snapshot } from "./lib/types";
import { NO_FACTS, NO_SIZES } from "./lib/types";

// The Ollama source's page says under its title what its models take
// together (`modelsTotalText` in src/lib/sizes.ts, `usePageSubtitle` in
// src/App.tsx).

const mockInvoke = vi.mocked(invoke);

const brew: ManagerInstance = {
  id: "brew:/opt/homebrew",
  adapter_id: "brew",
  exe_path: "/opt/homebrew/bin/brew",
  prefix: "/opt/homebrew",
  scope: "User",
  version: "7.0.3",
  status: { unavailable: null, notes: [] },
  unverified_version: null,
  read_only_reason: null,
};

const OLLAMA = "ollama:http://127.0.0.1:11434";
const ollama: ManagerInstance = { ...brew, id: OLLAMA, adapter_id: "ollama", exe_path: "/opt/homebrew/bin/ollama" };

const jq: InstalledArtifact = {
  key: { instance_id: brew.id, kind: "Formula", name: "jq" },
  display_name: "jq",
  version: "1.8.2",
  reason: "Requested",
  description: "Lightweight and flexible command-line JSON processor",
  homepage: null,
  size_bytes: null,
  installed_at: null,
  path: null,
  auto_updates: false,
  uninstall_blocked: null,
  facts: NO_FACTS,
};

const llama: InstalledArtifact = {
  ...jq,
  key: { instance_id: OLLAMA, kind: "Model", name: "llama3.2:3b" },
  display_name: "llama3.2:3b",
  version: "8e4cdead7463",
  description: null,
  size_bytes: 2_019_393_189,
};

const snapshot: Snapshot = {
  generation: 1,
  round: 2,
  detect: "Found",
  instances: [brew, ollama],
  artifacts: [jq, llama],
  updates: [],
  refreshed_at: 1789700000,
  stale: false,
  errors: [],
};

const settings: Settings = {
  language: "System",
  show_technical_details: false,
  ignored_updates: [],
  skipped_versions: [],
  include_self_updating: false,
  auto_check: false,
  notify_updates: false,
};

let served: Sizes;

beforeEach(() => {
  mockInvoke.mockReset();
  served = NO_SIZES;
  vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockImplementation(function (this: HTMLElement) {
    return this.getAttribute("data-index") === null ? 600 : 56;
  });
  vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(800);
  mockInvoke.mockImplementation((cmd: string) => {
    if (cmd === "get_snapshot" || cmd === "refresh") return Promise.resolve(snapshot);
    if (cmd === "get_settings") return Promise.resolve(settings);
    if (cmd === "list_operations") return Promise.resolve([]);
    if (cmd === "get_sizes") return Promise.resolve(served);
    return Promise.resolve(undefined);
  });
});

afterEach(() => {
  vi.restoreAllMocks();
});

async function subtitleOn(source: string): Promise<() => string | null> {
  const view = renderWithProviders(<App />);
  await view.findByRole("heading", { level: 2, name: "Everything is up to date" });
  const sources = await view.findByRole("list", { name: "Sources" });
  fireEvent.click(await within(sources).findByRole("button", { name: source }));
  await view.findByRole("heading", { level: 1, name: source });
  return () => view.getByRole("heading", { level: 1 }).nextElementSibling?.textContent ?? null;
}

describe("the Ollama source's page", () => {
  it("says what its models take together after its count, once they are measured", async () => {
    served = {
      ...NO_SIZES,
      round: 2,
      done: true,
      models: [{ instance_id: OLLAMA, measured: { bytes: 6_620_000_000, partial: false, at_least: false } }],
    };
    const subtitle = await subtitleOn("Ollama");
    await waitFor(() => expect(subtitle()).toBe("1 tool · Ollama models: about\u00a06.6 GB in all"));
  });

  it("says only its count while they are measured", async () => {
    served = { ...NO_SIZES, round: 2, models: [{ instance_id: OLLAMA, measured: null }] };
    const subtitle = await subtitleOn("Ollama");
    await waitFor(() => expect(subtitle()).toBe("1 tool"));
  });

  it("is said on Ollama's page alone", async () => {
    served = {
      ...NO_SIZES,
      round: 2,
      done: true,
      models: [{ instance_id: OLLAMA, measured: { bytes: 6_620_000_000, partial: false, at_least: false } }],
    };
    const subtitle = await subtitleOn("Homebrew");
    await waitFor(() => expect(subtitle()).toBe("1 tool"));
  });
});
