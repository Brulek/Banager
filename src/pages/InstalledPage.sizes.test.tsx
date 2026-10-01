import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import { WithToolbarSlot } from "../test/toolbarSlot";
import { InstalledPage } from "./InstalledPage";
import i18n from "../i18n";
import { useUiStore } from "../store/ui";
import type { InstalledArtifact, ManagerInstance, Measured, Settings, Sizes, Snapshot } from "../lib/types";
import { NO_FACTS, NO_SIZES } from "../lib/types";

// How much each tool takes on disk, in the Installed page's details
// (`sizeFact` in src/components/SizeFact.tsx, from `get_sizes`).

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
const ollama: ManagerInstance = {
  ...brew,
  id: OLLAMA,
  adapter_id: "ollama",
  exe_path: "/opt/homebrew/bin/ollama",
  prefix: "/Users/you/.ollama",
  version: "0.34.1",
};

function formula(name: string, version: string): InstalledArtifact {
  return {
    key: { instance_id: brew.id, kind: "Formula", name },
    display_name: name,
    version,
    reason: "Requested",
    description: `${name} blurb`,
    homepage: null,
    size_bytes: null,
    installed_at: null,
    path: null,
    auto_updates: false,
    uninstall_blocked: null,
    facts: NO_FACTS,
  };
}

// Homebrew keeps one other keg of node@22: its other version, whose size
// the round measures with it (`old_versions`).
const node: InstalledArtifact = {
  ...formula("node@22", "22.23.3"),
  facts: {
    ...NO_FACTS,
    homebrew: { deprecated: null, disabled: null, caveats: null, other_versions: ["22.22.0"] },
  },
};
const jq = formula("jq", "1.8.2");
const wget = formula("wget", "1.25.0");
const llama: InstalledArtifact = {
  ...formula("llama3.2:3b", "8e4cdead7463"),
  key: { instance_id: OLLAMA, kind: "Model", name: "llama3.2:3b" },
  display_name: "llama3.2:3b",
  description: null,
  size_bytes: 2_019_393_189,
};

const snapshot: Snapshot = {
  generation: 1,
  round: 4,
  detect: "Found",
  instances: [brew, ollama],
  artifacts: [jq, llama, node, wget],
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

const about = (bytes: number, more: Partial<Measured> = {}): Measured => ({
  bytes,
  partial: false,
  at_least: false,
  ...more,
});

let served: Sizes;

beforeEach(() => {
  mockInvoke.mockReset();
  served = NO_SIZES;
  vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockImplementation(function (this: HTMLElement) {
    return this.getAttribute("data-index") === null ? 600 : 56;
  });
  vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(800);
  mockInvoke.mockImplementation((cmd: string) => {
    if (cmd === "get_snapshot") return Promise.resolve(snapshot);
    if (cmd === "get_settings") return Promise.resolve(settings);
    if (cmd === "list_operations") return Promise.resolve([]);
    if (cmd === "get_sizes") return Promise.resolve(served);
    return Promise.resolve(undefined);
  });
});

afterEach(async () => {
  vi.restoreAllMocks();
  await i18n.changeLanguage("en");
});

async function openDetails(name: string): Promise<HTMLElement> {
  const label = await screen.findByText(name, { selector: "[data-tool-row] p" });
  const row = label.closest("[data-tool-row]") as HTMLElement;
  fireEvent.click(within(row).getByRole("button", { name: i18n.t("common.detailsLabel", { title: name }) }));
  return screen.findByRole("complementary", { name });
}

/** The details' facts, label to value text. */
function factsOf(inspector: HTMLElement): Record<string, string> {
  const facts = inspector.querySelector("[data-facts]");
  if (facts === null) return {};
  return Object.fromEntries(
    [...facts.children].map((row) => [row.firstElementChild?.textContent ?? "", row.lastElementChild?.textContent ?? ""]),
  );
}

function render() {
  return renderWithProviders(
    <WithToolbarSlot>
      <InstalledPage />
    </WithToolbarSlot>,
  );
}

describe("the Installed page's details, on disk use", () => {
  it("say about how much a tool takes, and what a formula's other versions take under those versions", async () => {
    served = {
      ...NO_SIZES,
      round: 4,
      done: true,
      artifacts: [
        { key: node.key, version: "22.23.3", measured: about(312_600_000), old_versions: about(298_400_000) },
        { key: jq.key, version: "1.8.2", measured: about(1_200_000), old_versions: null },
      ],
    };
    render();
    const inspector = await openDetails("node@22");
    expect(await within(inspector).findByText("About 312.6 MB")).toBeInTheDocument();
    expect(factsOf(inspector)["Space used"]).toBe("About\u00a0312.6 MB");
    // One word for those kegs, 「其他版本」, and their size beside them, in
    // the row right under the tool's own.
    expect(factsOf(inspector)["Other versions"]).toBe("22.22.0 " + "About\u00a0298.4 MB");
    const other = inspector.querySelector("[data-other-versions-size]");
    expect(other?.textContent).toBe("About\u00a0298.4 MB");
    expect(other).toHaveClass("text-muted");
    const terms = Object.keys(factsOf(inspector));
    expect(terms.indexOf("Other versions")).toBe(terms.indexOf("Space used") + 1);
    expect(within(inspector).getByText("Other versions").nextElementSibling).toHaveClass("select-text");
    expect(within(inspector).queryByText(/also installed/)).toBeNull();
    // The row's value selects, as a version does, to be copied.
    const value = within(inspector).getByText("Space used").nextElementSibling;
    expect(value).toHaveClass("select-text", "tabular-nums", "text-right");

    const jqDetails = await openDetails("jq");
    expect(await within(jqDetails).findByText("About 1.2 MB")).toBeInTheDocument();
    expect(within(jqDetails).queryByText("Other versions")).toBeNull();
  });

  it("say Calculating… while it is measured, and for a size measured at another version", async () => {
    served = {
      ...NO_SIZES,
      round: 4,
      artifacts: [
        { key: node.key, version: "22.23.3", measured: null, old_versions: null },
        { key: jq.key, version: "1.8.1", measured: about(1_000_000), old_versions: null },
      ],
    };
    render();
    const inspector = await openDetails("node@22");
    const measuring = await within(inspector).findByText("Calculating…");
    expect(measuring).toHaveClass("text-muted");
    expect(within(inspector).getByText("Space used").nextElementSibling).not.toHaveClass("select-text");
    // The other versions are said all the same; their size once it is in.
    expect(factsOf(inspector)["Other versions"]).toBe("22.22.0 ");

    const jqDetails = await openDetails("jq");
    expect(await within(jqDetails).findByText("Calculating…")).toBeInTheDocument();
    expect(within(jqDetails).queryByText("About 1 MB")).toBeNull();
  });

  it("say nothing of a tool that has no size to show", async () => {
    served = {
      ...NO_SIZES,
      round: 4,
      done: true,
      artifacts: [{ key: node.key, version: "22.23.3", measured: about(312_600_000), old_versions: null }],
    };
    render();
    const inspector = await openDetails("wget");
    await within(inspector).findByText("Version");
    expect(within(inspector).queryByText("Space used")).toBeNull();
    expect(within(inspector).queryByText("Calculating…")).toBeNull();
    expect(Object.keys(factsOf(inspector))).toEqual(["Version", "Status"]);
  });

  it("say at least, or that part of it could not be read, when the size is not all of it", async () => {
    served = {
      ...NO_SIZES,
      round: 4,
      done: true,
      artifacts: [
        { key: node.key, version: "22.23.3", measured: about(612_400_000, { at_least: true }), old_versions: null },
        { key: jq.key, version: "1.8.2", measured: about(22_700_000, { partial: true }), old_versions: null },
      ],
    };
    render();
    const inspector = await openDetails("node@22");
    // The text matcher reads the no-break space before the number as a space.
    expect(await within(inspector).findByText("612.4 MB or more")).toBeInTheDocument();
    const jqDetails = await openDetails("jq");
    expect(await within(jqDetails).findByText("About 22.7 MB; some of it couldn't be read")).toBeInTheDocument();
  });

  it("keep a model's own size, as Ollama reports it, said as every size is, and never measure it", async () => {
    served = {
      ...NO_SIZES,
      round: 4,
      done: true,
      models: [{ instance_id: OLLAMA, measured: about(6_620_000_000) }],
    };
    render();
    const inspector = await openDetails("llama3.2:3b");
    // Said as every other size is: 「占用空间」, 「约…」.
    await within(inspector).findByText("Space used");
    expect(factsOf(inspector)["Space used"]).toBe("About\u00a02 GB");
    expect(within(inspector).queryByText("Size")).toBeNull();
  });

  it("say it in Chinese, 约 before every number", async () => {
    await i18n.changeLanguage("zh-CN");
    served = {
      ...NO_SIZES,
      round: 4,
      done: true,
      artifacts: [
        { key: node.key, version: "22.23.3", measured: about(312_600_000), old_versions: about(1_200_000_000) },
        { key: jq.key, version: "1.8.2", measured: null, old_versions: null },
      ],
    };
    render();
    const inspector = await openDetails("node@22");
    await within(inspector).findByText("占用空间");
    expect(factsOf(inspector)["占用空间"]).toBe("约312.6 MB");
    expect(factsOf(inspector)["其他版本"]).toBe("22.22.0 " + "约1.2 GB");
    const jqDetails = await openDetails("jq");
    expect(await within(jqDetails).findByText("正在计算…")).toBeInTheDocument();
  });

  it("order the list By Size, the largest first, a tool with no size last, a model by its own size", async () => {
    served = {
      ...NO_SIZES,
      round: 4,
      done: true,
      artifacts: [
        { key: node.key, version: "22.23.3", measured: about(312_600_000), old_versions: null },
        { key: jq.key, version: "1.8.2", measured: about(1_200_000), old_versions: null },
      ],
    };
    render();
    await screen.findByText("wget", { selector: "[data-tool-row] p" });
    const rowNames = () =>
      [...document.querySelectorAll("[data-tool-row]")].map((row) => row.querySelector("p")?.textContent ?? "");
    expect(rowNames()).toEqual(["jq", "llama3.2:3b", "node@22", "wget"]);
    const sortBy = screen.getByRole("combobox", { name: "Sort Order" });
    fireEvent.change(sortBy, { target: { value: "size" } });
    await waitFor(() => expect(rowNames()).toEqual(["llama3.2:3b", "node@22", "jq", "wget"]));
    expect(sortBy.parentElement?.firstElementChild).toHaveTextContent(/^By Size$/);
    // The size the order goes by, in the version's place, as Finder's Size
    // column: 「—」, muted, for a row with none.
    const cells = () =>
      [...document.querySelectorAll("[data-tool-row]")].map((row) => row.querySelector("[data-size-cell]")?.textContent);
    expect(cells()).toEqual(["About\u00a02 GB", "About\u00a0312.6 MB", "About\u00a01.2 MB", "—"]);
    expect(document.querySelectorAll("[data-size-cell].text-muted")).toHaveLength(1);
    // By name again: the versions.
    fireEvent.change(sortBy, { target: { value: "name" } });
    await waitFor(() => expect(document.querySelector("[data-size-cell]")).toBeNull());
  });
});

describe("the Installed page's source headings, on disk use", () => {
  // What size.rs adds up per source (`Sizes.sources`): node@22 with its old
  // versions, jq and wget; Ollama's models as their folder.
  const measuredAll: Sizes = {
    ...NO_SIZES,
    round: 4,
    done: true,
    artifacts: [
      { key: node.key, version: "22.23.3", measured: about(312_600_000), old_versions: about(298_400_000) },
      { key: jq.key, version: "1.8.2", measured: about(1_200_000), old_versions: null },
      { key: wget.key, version: "1.25.0", measured: about(4_200_000), old_versions: null },
    ],
    models: [{ instance_id: OLLAMA, measured: about(6_620_000_000) }],
    total: about(7_236_400_000),
    sources: [
      { instance_id: brew.id, measured: about(616_400_000) },
      { instance_id: OLLAMA, measured: about(6_620_000_000) },
    ],
  };

  async function bySource() {
    useUiStore.getState().setInstalledSort("source");
    render();
    await screen.findByText("wget", { selector: "[data-tool-row] p" });
  }

  it("say what each source takes after its count, as Ollama's own line says its models", async () => {
    served = measuredAll;
    await bySource();
    expect(await screen.findByRole("heading", { level: 2, name: "Homebrew · 3 tools · about 616.4 MB" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { level: 2, name: "Ollama · 1 model · about 6.6 GB" })).toBeInTheDocument();
  });

  it("say at least where a tool of the source has no size, and nothing while it is measured", async () => {
    served = { ...measuredAll, artifacts: measuredAll.artifacts.filter((size) => size.key.name !== "wget") };
    await bySource();
    expect(
      await screen.findByRole("heading", { level: 2, name: "Homebrew · 3 tools · 616.4 MB or more" }),
    ).toBeInTheDocument();

    cleanupAndServe({ ...measuredAll, done: false, total: null, sources: [] });
    await bySource();
    expect(await screen.findByRole("heading", { level: 2, name: "Homebrew · 3 tools" })).toBeInTheDocument();
  });

  it("say no size while a search narrows the count to part of the source", async () => {
    served = measuredAll;
    await bySource();
    await screen.findByRole("heading", { level: 2, name: /^Homebrew · 3 tools · / });
    fireEvent.change(screen.getByRole("searchbox"), { target: { value: "j" } });
    expect(await screen.findByRole("heading", { level: 2, name: "Homebrew · 1 tool" })).toBeInTheDocument();
  });

  it("say in a tooltip that other versions count and caches do not, which the rows' sizes leave out", async () => {
    served = measuredAll;
    await bySource();
    const heading = await screen.findByRole("heading", { level: 2, name: "Homebrew · 3 tools · about 616.4 MB" });
    expect(heading).toHaveAttribute("title", expect.stringMatching(/including other versions but not caches/));

    cleanupAndServe({ ...measuredAll, done: false, total: null, sources: [] });
    await bySource();
    expect(await screen.findByRole("heading", { level: 2, name: "Homebrew · 3 tools" })).not.toHaveAttribute("title");
  });

  it("say it in Chinese, 约 before the number", async () => {
    await i18n.changeLanguage("zh-CN");
    served = measuredAll;
    await bySource();
    expect(await screen.findByRole("heading", { level: 2, name: "Homebrew · 3个 · 约616.4 MB" })).toBeInTheDocument();
  });
});

/** Unmounts what is rendered and serves `sizes` from now on. */
function cleanupAndServe(sizes: Sizes) {
  cleanup();
  served = sizes;
}
