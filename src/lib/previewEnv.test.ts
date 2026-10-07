import { describe, expect, it } from "vitest";
import { previewEnvValue } from "./previewEnv";
import { commandTexts } from "../components/CommandPreview";
import { terminalCommand, terminalCommandParts } from "../components/PasswordCommand";
import type { OpSummary, PlanAction } from "./types";

describe("Ollama login previews", () => {
  it.each([
    ["http://alice:s%40cret@server:11434", "http://****:****@server:11434"],
    ["https://a@server", "https://****@server"],
    ["http://:pw@[::1]:11434", "http://****:****@[::1]:11434"],
    ["a:b@server:80", "****:****@server:80"],
    ["http://a'@server", "http://****@server"],
    ["http://server/path@name?q=a@b", "http://server/path@name?q=a@b"],
    ["http://server:11434", "http://server:11434"],
  ])("masks only the login in %s", (raw, shown) => {
    expect(previewEnvValue("OLLAMA_HOST", raw)).toBe(shown);
    expect(previewEnvValue("OLLAMA_HOST", shown)).toBe(shown);
    expect(previewEnvValue("OTHER", raw)).toBe(raw);
  });

  it("previews both command variants without mutating the input environment", () => {
    const env: [string, string][] = [["OLLAMA_HOST", "http://alice:secret@server:11434"], ["NO_COLOR", "1"]];
    const command = { program: "/mock/ollama", args: ["pull", "qwen:latest"], env };
    for (const action of [{ Command: command }, { CommandThen: { ...command, then: [["rm", "old:latest"]] } }] satisfies PlanAction[]) {
      const shown = commandTexts(action);
      expect(shown.length).toBe("Command" in action ? 1 : 2);
      for (const text of shown) {
        expect(text).toContain("http://****:****@server:11434");
        expect(text).toContain("NO_COLOR=1");
        expect(text).not.toMatch(/alice|secret/);
      }
    }
    expect(env[0][1]).toBe("http://alice:secret@server:11434");
  });

  it("round-trips the masked Rust plan and operation wire shapes into every command display", () => {
    const action: PlanAction = JSON.parse('{"Command":{"program":"/mock/ollama","args":["pull","qwen:latest"],"env":[["OLLAMA_HOST","http://****:****@server:11434"]]}}');
    const op: OpSummary = JSON.parse('{"id":10,"kind":"Upgrade","instance_id":"ollama:http://server:11434","artifact_kind":"Model","name":"qwen:latest","status":"Done","outcome":"Succeeded","argv_preview":["/mock/ollama","pull","qwen:latest"],"env_preview":[["OLLAMA_HOST","http://****:****@server:11434"]],"cancel_policy":"KillThenReconcile"}');
    expect(JSON.parse(JSON.stringify(action))).toEqual(action);
    expect(JSON.parse(JSON.stringify(op))).toEqual(op);
    expect(terminalCommand(op)).toBe(commandTexts(action)[0]);
    expect(terminalCommandParts(op)?.env.join(" ")).toContain("http://****:****@server:11434");
    op.env_preview = [["OLLAMA_HOST", "alice:secret@server:11434"]];
    expect(terminalCommand(op)).not.toMatch(/alice|secret/);
    expect(terminalCommandParts(op)?.env.join(" ")).not.toMatch(/alice|secret/);
  });
});
