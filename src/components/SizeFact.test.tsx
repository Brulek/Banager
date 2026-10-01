import { describe, expect, it } from "vitest";
import { fireEvent } from "@testing-library/react";
import { renderWithProviders } from "../test/setup";
import i18n from "../i18n";
import { sizeFact } from "./SizeFact";
import { NO_FACTS, NO_SIZES, type InstalledArtifact, type Sizes } from "../lib/types";

// The ⓘ after a measured size that leaves something out (`sizeNoteOf`).

function tool(instance_id: string, name: string): InstalledArtifact {
  return {
    key: { instance_id, kind: "Tool", name },
    display_name: name,
    version: "1.0.0",
    reason: "Requested",
    description: null,
    homepage: null,
    size_bytes: null,
    installed_at: null,
    path: `/Users/you/bin/${name}`,
    auto_updates: false,
    uninstall_blocked: null,
    facts: NO_FACTS,
  };
}

function measuredFor(artifact: InstalledArtifact): Sizes {
  return {
    ...NO_SIZES,
    round: 1,
    done: true,
    artifacts: [
      {
        key: artifact.key,
        version: artifact.version,
        measured: { bytes: 11_100_000, partial: false, at_least: false },
        old_versions: null,
      },
    ],
  };
}

function renderValue(artifact: InstalledArtifact) {
  const row = sizeFact(i18n.getFixedT("en"), artifact, measuredFor(artifact));
  if (row === null) throw new Error("no size row");
  return renderWithProviders(<div data-value="">{row.value}</div>);
}

describe("sizeFact's ⓘ", () => {
  it("says a crate's size is its program files only, behind an ⓘ after the number", () => {
    const { getByRole, container } = renderValue(tool("cargo:/Users/you/.cargo", "ripgrep"));
    expect(container.querySelector("[data-value]")?.textContent?.trim()).toBe("About 11.1 MB");
    const details = getByRole("button", { name: "Details: Space used" });
    fireEvent.click(details);
    expect(document.getElementById(details.getAttribute("aria-controls") ?? "")).toHaveTextContent(
      "Counts only its program files, not what it downloads or caches.",
    );
  });

  it("says the same of a tool with its own installer, and that a uv tool shares files with uv's cache", () => {
    const rustup = renderValue(tool("standalone-rustup", "rustup"));
    fireEvent.click(rustup.getByRole("button", { name: "Details: Space used" }));
    expect(document.body).toHaveTextContent("Counts only its program files");
    rustup.unmount();
    const ruff = renderValue(tool("uv", "ruff"));
    fireEvent.click(ruff.getByRole("button", { name: "Details: Space used" }));
    expect(document.body).toHaveTextContent("so the tool alone may take less");
  });

  it("has no ⓘ for a source whose size is all of the tool", () => {
    const { queryByRole } = renderValue(tool("npm:/opt/homebrew", "prettier"));
    expect(queryByRole("button")).toBeNull();
  });
});

describe("sizeFact for a measured 0", () => {
  it("has no row rather than 「约0 B」, in both languages", () => {
    // npm's corepack under Homebrew's node: its folder is only links into the
    // node keg, which take no blocks.
    const corepack = { ...tool("npm:/opt/homebrew", "corepack"), key: { instance_id: "npm:/opt/homebrew", kind: "Package" as const, name: "corepack" } };
    const zero: Sizes = {
      ...NO_SIZES,
      round: 1,
      done: true,
      artifacts: [
        { key: corepack.key, version: corepack.version, measured: { bytes: 0, partial: false, at_least: false }, old_versions: null },
      ],
    };
    expect(sizeFact(i18n.getFixedT("en"), corepack, zero)).toBeNull();
    expect(sizeFact(i18n.getFixedT("zh-CN"), corepack, zero)).toBeNull();
    // A size its source reports keeps its row, as before; and a few bytes are said.
    const few: Sizes = {
      ...zero,
      artifacts: [{ ...zero.artifacts[0], measured: { bytes: 4_096, partial: false, at_least: false } }],
    };
    expect(sizeFact(i18n.getFixedT("en"), corepack, few)?.term).toBe("Space used");
  });
});
