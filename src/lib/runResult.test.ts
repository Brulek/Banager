import { afterEach, describe, expect, it } from "vitest";
import i18n from "../i18n";
import type { OpKind, OpSummary, Outcome } from "./types";
import { failedRunWords, runTally } from "./runResult";

function op(id: number, outcome: Outcome, kind: OpKind = "Upgrade"): OpSummary {
  return {
    id,
    kind,
    instance_id: "brew:/opt/homebrew",
    artifact_kind: "Formula",
    name: `tool${id}`,
    status: "Done",
    outcome,
    argv_preview: ["/opt/homebrew/bin/brew", "upgrade", `tool${id}`],
    cancel_policy: "KillThenReconcile",
  };
}

const failed: Outcome = { Failed: { exit_code: 1, summary: "Error: something" } };
const t = (key: string, options?: Record<string, unknown>) => i18n.t(key, options);

afterEach(async () => {
  await i18n.changeLanguage("en");
});

describe("runTally", () => {
  it("sorts each ending by its tone, Banager's own failures and Unconfirmed among them", () => {
    expect(
      runTally([
        op(1, failed),
        op(2, { BanagerFailed: "Internal" }),
        op(3, "Unconfirmed"),
        op(4, { NeedsAttention: "GoneAfterUpgrade" }),
        op(5, "Succeeded"),
        op(6, "Cancelled"),
      ]),
    ).toEqual({ failed: 2, attention: 2, succeeded: 1, cancelled: 1 });
  });
});

describe("failedRunWords", () => {
  it("has nothing to say of a run with no failure: that is 需要查看's, or a success's", () => {
    expect(failedRunWords(t, [op(1, "Succeeded"), op(2, "Unconfirmed")])).toBeNull();
    expect(failedRunWords(t, [op(1, "Succeeded"), op(2, "Cancelled")])).toBeNull();
  });

  it("says failed updates as failed updates, then what else the run came to, in English", () => {
    expect(failedRunWords(t, [op(1, failed), op(2, failed)])).toBe("2 updates failed");
    expect(failedRunWords(t, [op(1, failed), op(2, "Succeeded"), op(3, "Succeeded")])).toBe(
      "1 update failed, 2 succeeded",
    );
    expect(
      failedRunWords(t, [op(1, failed), op(2, "Succeeded"), op(3, "Unconfirmed"), op(4, "Cancelled")]),
    ).toBe("1 update failed, 1 succeeded, 1 needs attention, 1 cancelled");
  });

  it("says the same in Chinese", async () => {
    await i18n.changeLanguage("zh-CN");
    expect(failedRunWords(t, [op(1, failed), op(2, failed)])).toBe("2个更新失败");
    expect(failedRunWords(t, [op(1, failed), op(2, "Succeeded"), op(3, "Succeeded")])).toBe("1个更新失败，2个已成功");
    expect(
      failedRunWords(t, [op(1, failed), op(2, "Succeeded"), op(3, { NeedsAttention: "UnchangedAfterUpgrade" }), op(4, "Cancelled")]),
    ).toBe("1个更新失败，1个已成功，1个需要查看，1个已取消");
  });

  // walk-4 W4-1: an update that stopped at sudo's password is not said
  // as a failure; its row says 「需要输入密码」 and keeps its update.
  const password: Outcome = {
    Failed: {
      exit_code: 1,
      summary:
        "sudo: a terminal is required to read the password; either use the -S option to read from standard input or configure an askpass helper\nsudo: a password is required",
    },
  };

  it("says updates that stopped at the Mac password as needing it, apart from the ones that failed", async () => {
    expect(failedRunWords(t, [op(1, password), op(2, password), op(3, password)])).toBe("3 need your password");
    expect(failedRunWords(t, [op(1, password)])).toBe("1 needs your password");
    expect(failedRunWords(t, [op(1, failed), op(2, password), op(3, password), op(4, "Succeeded")])).toBe(
      "1 update failed, 2 need your password, 1 succeeded",
    );
    // A run of other kinds keeps its plain failures.
    expect(failedRunWords(t, [op(1, password, "Uninstall")])).toBe("1 wasn't uninstalled");

    await i18n.changeLanguage("zh-CN");
    expect(failedRunWords(t, [op(1, password), op(2, password), op(3, password)])).toBe("3个需要输入密码");
    expect(failedRunWords(t, [op(1, failed), op(2, password), op(3, password), op(4, "Succeeded")])).toBe(
      "1个更新失败，2个需要输入密码，1个已成功",
    );
  });

  it("says uninstalls in the result block's words, and a run of several kinds as plain failures", async () => {
    const uninstalls = [op(1, failed, "Uninstall"), op(2, "Succeeded", "Uninstall"), op(3, "Succeeded", "Uninstall")];
    expect(failedRunWords(t, uninstalls)).toBe("Uninstalled 2; 1 wasn't uninstalled");
    expect(failedRunWords(t, [op(1, failed, "Uninstall")])).toBe("1 wasn't uninstalled");
    // Cancelled and needing a look count as not uninstalled, as the block counts them.
    expect(
      failedRunWords(t, [
        op(1, failed, "Uninstall"),
        op(2, "Cancelled", "Uninstall"),
        op(3, "Unconfirmed", "Uninstall"),
        op(4, "Succeeded", "Uninstall"),
      ]),
    ).toBe("Uninstalled 1; 3 weren't uninstalled");
    expect(failedRunWords(t, [op(1, failed, "Install"), op(2, "Succeeded")])).toBe("1 failed, 1 succeeded");

    await i18n.changeLanguage("zh-CN");
    expect(failedRunWords(t, uninstalls)).toBe("已卸载2个，1个没有卸载");
    expect(failedRunWords(t, [op(1, failed, "Uninstall"), op(2, failed, "Uninstall")])).toBe("2个没有卸载");
    expect(failedRunWords(t, [op(1, failed, "Install"), op(2, "Succeeded")])).toBe("1个失败，1个已成功");
  });
});
