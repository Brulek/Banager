import { describe, expect, it } from "vitest";
import { displayToken } from "./format";

describe("displayToken", () => {
  it("leaves a plain token alone", () => {
    expect(displayToken("--cask")).toBe("--cask");
    expect(displayToken("/opt/homebrew/bin/brew")).toBe("/opt/homebrew/bin/brew");
  });

  it("quotes a program path that contains a space", () => {
    expect(displayToken("/Users/Alice Smith/bin/brew")).toBe("'/Users/Alice Smith/bin/brew'");
  });

  it("shows an empty argument as ''", () => {
    expect(displayToken("")).toBe("''");
  });

  it("escapes a single quote inside a quoted token", () => {
    expect(displayToken("it's")).toBe("'it'\\''s'");
  });
});
