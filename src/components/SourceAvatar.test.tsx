import { describe, expect, it } from "vitest";
import { render } from "@testing-library/react";
import { ADAPTER_LABEL_KEYS } from "../lib/sources";
import { SOURCE_AVATAR_CLASSES, SourceAvatar } from "./SourceAvatar";

describe("SourceAvatar", () => {
  it("has a colour for every source Canager knows", () => {
    // A source added to ADAPTER_LABEL_KEYS without a colour would get the
    // grey meant for a source this build does not know.
    expect(Object.keys(SOURCE_AVATAR_CLASSES).sort()).toEqual(
      Object.keys(ADAPTER_LABEL_KEYS).sort(),
    );
  });

  it("shows the first letter of the name, hidden from screen readers", () => {
    const { container } = render(<SourceAvatar adapterId="brew" label="Homebrew" />);
    const avatar = container.firstElementChild;
    expect(avatar).toHaveTextContent("H");
    expect(avatar).toHaveAttribute("aria-hidden", "true");
    expect(avatar?.className).toContain("bg-source-homebrew");
  });

  it("falls back to grey for a source it has no colour for", () => {
    const { container } = render(<SourceAvatar adapterId="toString" label="mystery" />);
    expect(container.firstElementChild).toHaveTextContent("M");
    expect(container.firstElementChild?.className).toContain("bg-muted");
  });
});
