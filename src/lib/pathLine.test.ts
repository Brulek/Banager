import { describe, expect, it } from "vitest";
import { exportPathLine } from "./pathLine";

// The author's decision U15 (a), 2026-10-06: for a command in a folder
// Terminal does not search, the line to add to a shell startup file, to
// copy. Banager never writes it anywhere itself.
describe("exportPathLine", () => {
  it("puts the folder first, with ~ as $HOME so the line works inside quotes", () => {
    expect(exportPathLine("~/.local/bin")).toBe('export PATH="$HOME/.local/bin:$PATH"');
    expect(exportPathLine("~/.grok/bin")).toBe('export PATH="$HOME/.grok/bin:$PATH"');
    expect(exportPathLine("~")).toBe('export PATH="$HOME:$PATH"');
    expect(exportPathLine("/opt/homebrew/opt/node@22/bin")).toBe('export PATH="/opt/homebrew/opt/node@22/bin:$PATH"');
    // A space is safe inside the quotes.
    expect(exportPathLine("~/My Tools/bin")).toBe('export PATH="$HOME/My Tools/bin:$PATH"');
  });

  it("escapes what the shell would read inside double quotes, so the folder is taken as written", () => {
    expect(exportPathLine('~/a"b/bin')).toBe('export PATH="$HOME/a\\"b/bin:$PATH"');
    expect(exportPathLine("~/a$b/bin")).toBe('export PATH="$HOME/a\\$b/bin:$PATH"');
    expect(exportPathLine("~/a`b/bin")).toBe('export PATH="$HOME/a\\`b/bin:$PATH"');
    expect(exportPathLine("/x\\y/bin")).toBe('export PATH="/x\\\\y/bin:$PATH"');
  });

  it("gives no line for a folder the search path cannot hold, or one it cannot place", () => {
    // `:` separates the folders of the search path; a line break ends the line.
    expect(exportPathLine("~/a:b/bin")).toBeNull();
    expect(exportPathLine("~/a\nb/bin")).toBeNull();
    // Neither absolute nor under the home folder: where would it be?
    expect(exportPathLine("bin")).toBeNull();
    expect(exportPathLine("~other/bin")).toBeNull();
    expect(exportPathLine("")).toBeNull();
  });
});
