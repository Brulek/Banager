import { describe, expect, it } from "vitest";
import { listedName, modelPath, nameKey, namesUnderSeveralSources } from "./names";

// The pretend Mac's two models (`MODELS` in src/dev/mockData.ts, which no
// test outside src/dev may import): one from another registry, one from
// Ollama's own.
const MODELS = {
  coder: "modelscope.cn/Qwen/Qwen2.5-Coder-7B-Instruct-GGUF:Q4_K_M",
  llama: "llama3.2:3b",
} as const;
// And `?state=many`'s (`MANY_MODELS` in src/dev/mockManyNames.ts): all from
// Ollama's own library.
const MANY_MODELS = ["deepseek-r1:14b", "gemma3:12b", "gpt-oss:20b", "mistral:7b", "nomic-embed-text:latest", "qwen3:8b"];

describe("namesUnderSeveralSources", () => {
  it("names only what two sources or more list, case aside", () => {
    const names = namesUnderSeveralSources([
      { name: "black", instanceId: "brew:/opt/homebrew" },
      { name: "Black", instanceId: "pipx" },
      { name: "jq", instanceId: "brew:/opt/homebrew" },
      // One source listing a name twice is not two sources.
      { name: "node", instanceId: "brew:/opt/homebrew" },
      { name: "node", instanceId: "brew:/opt/homebrew" },
    ]);
    expect([...names]).toEqual(["black"]);
    expect(names.has(nameKey("BLACK"))).toBe(true);
  });

  it("counts two instances of one kind of source as two sources", () => {
    // Homebrew in /opt/homebrew and in /usr/local.
    const names = namesUnderSeveralSources([
      { name: "git", instanceId: "brew:/opt/homebrew" },
      { name: "git", instanceId: "brew:/usr/local" },
    ]);
    expect([...names]).toEqual(["git"]);
  });
});

describe("modelPath", () => {
  it("splits a model pulled by a path into the model and where it came from", () => {
    // The pretend Mac's model from another registry.
    expect(modelPath({ kind: "Model" }, MODELS.coder)).toEqual({
      name: "Qwen2.5-Coder-7B-Instruct-GGUF:Q4_K_M",
      from: "modelscope.cn/Qwen",
    });
    expect(modelPath({ kind: "Model" }, "hf.co/bartowski/Llama-3.2-3B-Instruct-GGUF:Q4_K_M")).toEqual({
      name: "Llama-3.2-3B-Instruct-GGUF:Q4_K_M",
      from: "hf.co/bartowski",
    });
    // A user's namespace on Ollama's own registry.
    expect(modelPath({ kind: "Model" }, "someone/coder:7b")).toEqual({ name: "coder:7b", from: "someone" });
  });

  it("leaves whole a model named without a path, and every name that is not a model's", () => {
    expect(modelPath({ kind: "Model" }, MODELS.llama)).toBeNull();
    for (const name of MANY_MODELS) expect(modelPath({ kind: "Model" }, name)).toBeNull();
    // An npm scope and a Homebrew tap are part of the package's name.
    expect(modelPath({ kind: "Package" }, "@angular/cli")).toBeNull();
    expect(modelPath({ kind: "Formula" }, "tinygo-org/tools/tinygo")).toBeNull();
    // Nothing on one side of the slash: nothing to split.
    expect(modelPath({ kind: "Model" }, "/coder:7b")).toBeNull();
    expect(modelPath({ kind: "Model" }, "someone/")).toBeNull();
  });
});

describe("listedName", () => {
  it("is what a row shows as the name, and sorts by: a model's last path segment, else the name", () => {
    expect(listedName({ kind: "Model" }, MODELS.coder)).toBe("Qwen2.5-Coder-7B-Instruct-GGUF:Q4_K_M");
    expect(listedName({ kind: "Model" }, MODELS.llama)).toBe(MODELS.llama);
    expect(listedName({ kind: "Package" }, "@angular/cli")).toBe("@angular/cli");
  });
});
