import { afterEach, describe, expect, it, vi } from "vitest";
import { fireEvent, screen } from "@testing-library/react";
import { renderWithProviders } from "../test/setup";
import i18n from "../i18n";
import { InstalledSelectionHeader, UninstallSelectedButton } from "./InstalledSelectionHeader";
import { artifactKeyId, useUiStore } from "../store/ui";
import { MAX_BATCH_UNINSTALL } from "../lib/batchUninstall";
import { BUTTON } from "./ui/controls";
import type { InstalledArtifact, Measured, Sizes } from "../lib/types";
import { NO_FACTS, NO_SIZES } from "../lib/types";

function formula(name: string): InstalledArtifact {
  return {
    key: { instance_id: "brew:/opt/homebrew", kind: "Formula", name },
    display_name: name,
    version: "1.0",
    reason: "Requested",
    description: null,
    homepage: null,
    size_bytes: null,
    installed_at: null,
    path: null,
    auto_updates: false,
    uninstall_blocked: null,
    facts: NO_FACTS,
  };
}

const rows = (count: number) => Array.from({ length: count }, (_, index) => formula(`tool-${index + 1}`));
const [jq, wget, git] = [formula("jq"), formula("wget"), formula("git")];

const measured = (bytes: number, more: Partial<Measured> = {}): Measured => ({
  bytes,
  partial: false,
  at_least: false,
  ...more,
});
const sizesOf = (entries: Array<[InstalledArtifact, Measured]>): Sizes => ({
  ...NO_SIZES,
  round: 1,
  done: true,
  artifacts: entries.map(([target, size]) => ({ key: target.key, version: "1.0", measured: size, old_versions: null })),
});

afterEach(async () => {
  await i18n.changeLanguage("en");
});

function box(): HTMLInputElement {
  return screen.getByRole("checkbox", { name: "Select all items that can be uninstalled here" }) as HTMLInputElement;
}

function status(): string {
  return document.querySelector("[data-selection-status]")?.textContent ?? "";
}

describe("the Installed list's header", () => {
  it("ticks every row shown that can be ticked, and unticks them once all are", () => {
    const { rerender } = renderWithProviders(<InstalledSelectionHeader shown={[jq, wget, git]} counted={[]} sizes={undefined} />);
    expect(box().checked).toBe(false);
    expect(box().indeterminate).toBe(false);
    // Nothing ticked: what ticking is for.
    expect(status()).toBe("Select tools to uninstall together");
    fireEvent.click(box());
    expect(useUiStore.getState().selectedUninstalls).toEqual([jq, wget, git].map((a) => artifactKeyId(a.key)));

    rerender(<InstalledSelectionHeader shown={[jq, wget, git]} counted={[wget]} sizes={undefined} />);
    // Some: a dash, and a press ticks the rest.
    expect(box().indeterminate).toBe(true);
    expect(box().checked).toBe(false);
    expect(status()).toBe("1 selected");

    rerender(<InstalledSelectionHeader shown={[jq, wget, git]} counted={[jq, wget, git]} sizes={undefined} />);
    expect(box().checked).toBe(true);
    expect(box().indeterminate).toBe(false);
    fireEvent.click(box());
    expect(useUiStore.getState().selectedUninstalls).toEqual([]);
  });

  it("only clears when more rows are shown than one batch takes, and says the limit", () => {
    const many = rows(MAX_BATCH_UNINSTALL + 1);
    const { rerender } = renderWithProviders(<InstalledSelectionHeader shown={many} counted={[]} sizes={undefined} />);
    expect(status()).toBe("Select tools to uninstall together, up to 20 at a time");
    fireEvent.click(box());
    expect(useUiStore.getState().selectedUninstalls).toEqual([]);

    useUiStore.getState().selectUninstalls(many.slice(0, 3).map((a) => a.key));
    rerender(<InstalledSelectionHeader shown={many} counted={many.slice(0, 3)} sizes={undefined} />);
    expect(status()).toBe("3 selected");
    fireEvent.click(box());
    expect(useUiStore.getState().selectedUninstalls).toEqual([]);

    rerender(<InstalledSelectionHeader shown={rows(23)} counted={rows(23)} sizes={undefined} />);
    expect(status()).toBe("23 selected. Up to 20 can be uninstalled at a time");
  });

  it("says about how much the ticked rows take, hedged by the weakest", () => {
    const sizes = sizesOf([
      [jq, measured(20_000_000)],
      [wget, measured(5_000_000)],
    ]);
    const { rerender } = renderWithProviders(
      <InstalledSelectionHeader shown={[jq, wget, git]} counted={[jq, wget]} sizes={sizes} />,
    );
    expect(status()).toBe("2 selected · About 25 MB");
    // git's size is not known: what they take is more.
    rerender(<InstalledSelectionHeader shown={[jq, wget, git]} counted={[jq, git]} sizes={sizes} />);
    expect(status()).toBe("2 selected · 20 MB or more");
    // None known: no size at all.
    rerender(<InstalledSelectionHeader shown={[jq, wget, git]} counted={[git]} sizes={sizes} />);
    expect(status()).toBe("1 selected");
  });

  it("is off with no row to tick", () => {
    renderWithProviders(<InstalledSelectionHeader shown={[]} counted={[]} sizes={undefined} />);
    expect(box()).toBeDisabled();
    expect(screen.getByText("Select All")).toHaveClass("text-tertiary");
  });

  it("says it in Chinese", async () => {
    await i18n.changeLanguage("zh-CN");
    const sizes = sizesOf([[jq, measured(1_200_000_000)]]);
    renderWithProviders(<InstalledSelectionHeader shown={[jq, wget]} counted={[jq, wget]} sizes={sizes} />);
    expect(screen.getByText("全选")).toBeInTheDocument();
    expect(screen.getByRole("checkbox", { name: "全选这里可以卸载的项目" })).toBeInTheDocument();
    expect(status()).toBe("已选择2个 · 1.2 GB以上");
  });
});

describe("Uninstall Selected", () => {
  it("is there only while something is ticked, grey, and opens the sheet from itself", () => {
    const onOpen = vi.fn();
    const { rerender } = renderWithProviders(<UninstallSelectedButton count={0} sheetOpen={false} onOpen={onOpen} />);
    expect(screen.queryByRole("button")).toBeNull();
    rerender(<UninstallSelectedButton count={3} sheetOpen={false} onOpen={onOpen} />);
    const button = screen.getByRole("button", { name: "Uninstall Selected (3)…" });
    expect(button.className).toBe(BUTTON.regular.grey);
    fireEvent.click(button);
    expect(onOpen).toHaveBeenCalledWith(button);
  });

  it("is off past the most one batch takes, and while its sheet is up", () => {
    const { rerender } = renderWithProviders(
      <UninstallSelectedButton count={MAX_BATCH_UNINSTALL + 1} sheetOpen={false} onOpen={vi.fn()} />,
    );
    expect(screen.getByRole("button", { name: "Uninstall Selected (21)…" })).toBeDisabled();
    rerender(<UninstallSelectedButton count={MAX_BATCH_UNINSTALL} sheetOpen={false} onOpen={vi.fn()} />);
    expect(screen.getByRole("button")).toBeEnabled();
    rerender(<UninstallSelectedButton count={2} sheetOpen onOpen={vi.fn()} />);
    expect(screen.getByRole("button")).toBeDisabled();
  });

  it("says it in Chinese", async () => {
    await i18n.changeLanguage("zh-CN");
    const { rerender } = renderWithProviders(<UninstallSelectedButton count={3} sheetOpen={false} onOpen={vi.fn()} />);
    expect(screen.getByRole("button", { name: "卸载所选（3）…" })).toBeInTheDocument();
    rerender(<UninstallSelectedButton count={3} sheetOpen={false} onOpen={vi.fn()} compact />);
    expect(screen.getByRole("button", { name: "卸载（3）…" })).toBeInTheDocument();
  });

  it("is shorter in a narrow window, so the toolbar keeps room for the page's title", () => {
    renderWithProviders(<UninstallSelectedButton count={3} sheetOpen={false} onOpen={vi.fn()} compact />);
    expect(screen.getByRole("button", { name: "Uninstall (3)…" })).toBeEnabled();
  });
});
