import { describe, expect, it } from "vitest";
import { AUTOSPACE } from "../i18n/autospace";
import { commandMatching, searchMatch, searchTextOf } from "./searchMatch";
import { NO_FACTS, type ArtifactKey, type CommandFact } from "./types";

const tool = (name: string, commands: string[], displayName = name) => ({
  display_name: displayName,
  key: { instance_id: "brew-1", kind: "Formula", name } as ArtifactKey,
  facts: { ...NO_FACTS, commands: commands.map((command): CommandFact => ({ name: command, state: "Runs" })) },
});

const ripgrep = tool("ripgrep", ["rg"]);
const python = tool("python@3.13", ["idle3.13", "pip3.13", "pydoc3.13", "python3.13"]);
const claude = tool("@anthropic-ai/claude-code", ["claude"], "Claude Code");
const grokBuild = tool("grok-build", ["agent"]);
const gh = tool("gh", ["gh"]);

describe("searchMatch", () => {
  it("finds ripgrep by rg, a command it puts on the Mac, and says so", () => {
    expect(searchMatch(ripgrep, "rg")).toEqual({ by: "command", command: "rg" });
  });

  it("finds python@3.13 by pip3.13, and by a command's start", () => {
    expect(searchMatch(python, "pip3.13")).toEqual({ by: "command", command: "pip3.13" });
    expect(searchMatch(python, "pip")).toEqual({ by: "command", command: "pip3.13" });
    expect(searchMatch(python, "pydoc")).toEqual({ by: "command", command: "pydoc3.13" });
  });

  it("finds a tool whose package name says nothing of its command", () => {
    expect(searchMatch(grokBuild, "agent")).toEqual({ by: "command", command: "agent" });
    expect(searchMatch(claude, "claude")).toEqual({ by: "text" });
  });

  it("matches by name first: no command hint where the name already matches", () => {
    expect(searchMatch(gh, "gh")).toEqual({ by: "text" });
    // "python" is in the name too: the name's match, anywhere in it, as before.
    expect(searchMatch(python, "python")).toEqual({ by: "text" });
    // An empty search matches every tool, by its name.
    expect(searchMatch(ripgrep, "")).toEqual({ by: "text" });
  });

  it("does not match a command by letters in its middle or end", () => {
    // "g" is in "rg", but the command starts with "r".
    expect(searchMatch(tool("finder", ["rg"]), "g")).toBeNull();
    expect(searchMatch(python, "3.13x")).toBeNull();
    expect(searchMatch(python, "ip3")).toBeNull();
    expect(searchMatch(grokBuild, "gent")).toBeNull();
  });

  it("is a match case aside, naming the command as the facts spell it", () => {
    expect(searchMatch(tool("thing", ["Thing-CLI"]), "thing-c")).toEqual({ by: "command", command: "Thing-CLI" });
  });

  it("matches nothing for a tool whose commands are not known", () => {
    expect(searchMatch(tool("ripgrep", []), "rg")).toBeNull();
  });
});

describe("searchMatch over a tool's lines", () => {
  const ffmpeg = tool("ffmpeg", ["ffmpeg", "ffprobe"]);
  const ffmpegText = searchTextOf(ffmpeg, "音视频播放录制转换工具", "Play, record, convert, and stream audio and video");
  const jq = tool("jq", ["jq"]);
  const jqText = searchTextOf(jq, "命令行JSON处理工具", "Lightweight and flexible command-line JSON processor");
  const claudeText = searchTextOf(claude, "Anthropic的AI编程助手", "Anthropic's AI coding assistant");

  it("finds a tool by a word of the line its row shows, Chinese or Latin, case aside", () => {
    expect(searchMatch(ffmpeg, "视频", ffmpegText)).toEqual({ by: "text" });
    expect(searchMatch(jq, "json", jqText)).toEqual({ by: "text" });
    expect(searchMatch(claude, "编程", claudeText)).toEqual({ by: "text" });
    expect(searchMatch(claude, "anthropic", claudeText)).toEqual({ by: "text" });
    // Without its lines, as before: its names and commands only.
    expect(searchMatch(ffmpeg, "视频")).toBeNull();
  });

  it("finds it by the line in the other language too", () => {
    expect(searchMatch(ffmpeg, "video", ffmpegText)).toEqual({ by: "text" });
    expect(searchMatch(claude, "coding", claudeText)).toEqual({ by: "text" });
  });

  it("matches a Latin word of a line by its start only, so a short command is not buried", () => {
    // "rg" is in "large" and "merge", but begins no word there.
    const big = tool("git-lfs", ["git-lfs"]);
    const bigText = searchTextOf(big, "Git extension for versioning large files", "用于对大文件进行版本管理的Git扩展");
    expect(searchMatch(big, "rg", bigText)).toBeNull();
    expect(searchMatch(big, "large", bigText)).toEqual({ by: "text" });
    // After Chinese, a Latin word begins: 「命令行JSON」.
    expect(searchMatch(jq, "son", jqText)).toBeNull();
    // Chinese has no spaces to begin a word after: anywhere.
    expect(searchMatch(big, "版本", bigText)).toEqual({ by: "text" });
  });

  it("sees through the narrow gap autospace puts between Chinese and Latin", () => {
    const spaced = searchTextOf(jq, `命令行${AUTOSPACE}JSON${AUTOSPACE}处理工具`, null);
    expect(searchMatch(jq, "行json处", spaced)).toEqual({ by: "text" });
  });

  it("names the command where only the package's name, which the row does not show, matches too", () => {
    // Antigravity CLI's package is called agy, and so is its command: its row shows neither.
    const agy = tool("agy", ["agy"], "Antigravity CLI");
    const agyText = searchTextOf(agy, "Google的AI编程助手", "Google's AI coding assistant");
    expect(searchMatch(agy, "agy", agyText)).toEqual({ by: "command", command: "agy" });
    expect(searchMatch(agy, "agy")).toEqual({ by: "command", command: "agy" });
    // What the row shows matches: no word about a command.
    expect(searchMatch(agy, "antigravity", agyText)).toEqual({ by: "text" });
    // The package's name alone, with no command to name: still found.
    const code = tool("visual-studio-code", ["code"], "Microsoft Visual Studio Code");
    expect(searchMatch(code, "visual-studio")).toEqual({ by: "text" });
  });
});

describe("commandMatching", () => {
  const facts = (names: string[]): CommandFact[] => names.map((name) => ({ name, state: null }));

  it("prefers the command that is exactly the search over one that starts with it", () => {
    expect(commandMatching(facts(["pip3", "pip3.13"]), "pip3.13")).toBe("pip3.13");
    expect(commandMatching(facts(["git", "git-shell"]), "git")).toBe("git");
    expect(commandMatching(facts(["git-shell", "gitk"]), "git")).toBe("git-shell");
  });

  it("matches nothing for an empty search", () => {
    expect(commandMatching(facts(["rg"]), "")).toBeNull();
  });
});
