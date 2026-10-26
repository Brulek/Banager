import { describe, expect, it } from "vitest";
import en from "./en.json";
import zhCN from "./zh-CN.json";
import zhHant from "./zh-Hant.json";

/**
 * I18N-002: a source refuses to run only where Banager runs as the root
 * user (`env.euid == 0`, started with sudo), never in an ordinary
 * administrator account, which runs Homebrew fine. Each sentence names
 * root, and none says administrator.
 */
describe("the root user, by name", () => {
  it.each([
    ["en", en, /root user/, /administrator/i],
    ["zh-CN", zhCN, /root用户/, /管理员/],
    ["zh-Hant", zhHant, /root使用者/, /管理者/],
  ])("is said in %s wherever a source won't run as root", (_name, locale, root, admin) => {
    const sentences = [
      locale.updates.unavailableDetail.RefusesAsRoot,
      locale.sourceNotice.refusesAsRoot.title,
      locale.sourceNotice.refusesAsRoot.description,
      locale.diagnostics.text.statusWord.RefusesAsRoot,
    ];
    for (const sentence of sentences) {
      expect(sentence).toMatch(root);
      expect(sentence).not.toMatch(admin);
    }
  });
});
