import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "./test/setup";
import App from "./App";
import { useUiStore } from "./store/ui";
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
    await waitFor(() => expect(subtitle()).toBe("1 model · Ollama models: about\u00a06.6 GB in all"));
  });

  it("says only its count while the models measured are a round before the list", async () => {
    // A model deleted, the list refreshed (round 2), the folder not yet
    // measured again: round 1's total would be the folder before.
    served = {
      ...NO_SIZES,
      round: 1,
      done: true,
      models: [{ instance_id: OLLAMA, measured: { bytes: 6_620_000_000, partial: false, at_least: false } }],
    };
    const subtitle = await subtitleOn("Ollama");
    await waitFor(() => expect(subtitle()).toBe("1 model"));
  });

  it("says only its count while they are measured", async () => {
    served = { ...NO_SIZES, round: 2, models: [{ instance_id: OLLAMA, measured: null }] };
    const subtitle = await subtitleOn("Ollama");
    await waitFor(() => expect(subtitle()).toBe("1 model"));
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

/** Opens the Installed page on every source, as the sidebar's Installed row does. */
async function subtitleOnAll(): Promise<() => string | null> {
  const view = renderWithProviders(<App />);
  await view.findByRole("heading", { level: 2, name: "Everything is up to date" });
  const nav = await view.findByRole("navigation", { name: "Navigation" });
  fireEvent.click(within(nav).getByRole("button", { name: /^Installed/ }));
  await view.findByRole("heading", { level: 1, name: "Installed" });
  return () => view.getByRole("heading", { level: 1 }).nextElementSibling?.textContent ?? null;
}

describe("the Installed page's subtitle, on disk use", () => {
  // jq and the models measured; size.rs's totals (`Sizes.total`, `.sources`).
  const measured: Sizes = {
    ...NO_SIZES,
    round: 2,
    done: true,
    artifacts: [
      {
        key: jq.key,
        version: "1.8.2",
        measured: { bytes: 1_200_000, partial: false, at_least: false },
        old_versions: null,
      },
    ],
    models: [{ instance_id: OLLAMA, measured: { bytes: 6_620_000_000, partial: false, at_least: false } }],
    total: { bytes: 6_621_200_000, partial: false, at_least: false },
    sources: [
      { instance_id: brew.id, measured: { bytes: 1_200_000, partial: false, at_least: false } },
      { instance_id: OLLAMA, measured: { bytes: 6_620_000_000, partial: false, at_least: false } },
    ],
  };

  it("says what everything takes after the count on every source's list", async () => {
    served = measured;
    const subtitle = await subtitleOnAll();
    await waitFor(() => expect(subtitle()).toBe("2 tools · about 6.6 GB"));
  });

  it("says what one source's tools take on that source's list, and Ollama's models line on Ollama's", async () => {
    served = measured;
    const subtitle = await subtitleOn("Homebrew");
    await waitFor(() => expect(subtitle()).toBe("1 tool · about 1.2 MB"));
    const sources = await screen.findByRole("list", { name: "Sources" });
    fireEvent.click(within(sources).getByRole("button", { name: "Ollama" }));
    await waitFor(() => expect(subtitle()).toBe("1 model · Ollama models: about 6.6 GB in all"));
  });

  it("says at least when a tool counted has no size, and nothing until the round is done", async () => {
    served = { ...measured, artifacts: [] };
    const subtitle = await subtitleOnAll();
    await waitFor(() => expect(subtitle()).toBe("2 tools · 6.6 GB or more"));
  });

  it("says in a tooltip what a total holds, and that models' shared files count once over Ollama's", async () => {
    served = measured;
    const subtitle = await subtitleOn("Homebrew");
    await waitFor(() => expect(subtitle()).toBe("1 tool · about 1.2 MB"));
    const line = () => screen.getByRole("heading", { level: 1 }).nextElementSibling;
    expect(line()).toHaveAttribute("title", expect.stringMatching(/including other versions but not caches/));
    const sources = await screen.findByRole("list", { name: "Sources" });
    fireEvent.click(within(sources).getByRole("button", { name: "Ollama" }));
    await waitFor(() => expect(subtitle()).toBe("1 model · Ollama models: about 6.6 GB in all"));
    // Models: the files several share count once, which adding up the rows does not.
    expect(line()).toHaveAttribute("title", expect.stringMatching(/Files several models share count once/));
  });

  it("says how many of how many while the 显示 popup shows only some, and no size, which is of all of them", async () => {
    served = measured;
    const subtitle = await subtitleOnAll();
    await waitFor(() => expect(subtitle()).toBe("2 tools · about\u00a06.6 GB"));
    act(() => useUiStore.getState().setInstalledShow("twins"));
    await waitFor(() => expect(subtitle()).toBe("0 of 2 tools"));
    act(() => useUiStore.getState().setInstalledShow("all"));
    await waitFor(() => expect(subtitle()).toBe("2 tools · about\u00a06.6 GB"));
  });

  it("says only the count while the sizes are measured", async () => {
    served = { ...measured, done: false, total: null, sources: [] };
    const subtitle = await subtitleOnAll();
    await waitFor(() => expect(subtitle()).toBe("2 tools"));
  });
});
