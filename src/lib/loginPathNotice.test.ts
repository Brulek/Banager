import { describe, expect, it } from "vitest";
import { loginPathNotice } from "./loginPathNotice";
import en from "../i18n/en.json";
import zhCN from "../i18n/zh-CN.json";
import zhHant from "../i18n/zh-Hant.json";

describe("loginPathNotice", () => {
  it("says nothing until the facts are in, nor once the login shell's PATH is read", () => {
    expect(loginPathNotice(undefined)).toBeNull();
    expect(loginPathNotice(null)).toBeNull();
    expect(loginPathNotice({ login_path: true })).toBeNull();
  });

  it("warns, with Check Again, when it could not be read", () => {
    expect(loginPathNotice({ login_path: false })).toEqual({
      id: "login-path-unread",
      variant: "warning",
      titleKey: "loginPathNotice.title",
      descriptionKey: "loginPathNotice.description",
      action: { id: "checkAgain", labelKey: "header.checkAgain" },
    });
    // In all three languages, naming what may be missing, claiming nothing more.
    expect(en.loginPathNotice.title).toBe("Couldn't read Terminal's settings");
    expect(zhCN.loginPathNotice.title).toBe("无法读取终端的设置");
    expect(zhHant.loginPathNotice.title).toBe("無法讀取終端機的設定");
    for (const description of [
      en.loginPathNotice.description,
      zhCN.loginPathNotice.description,
      zhHant.loginPathNotice.description,
    ]) {
      expect(description).toContain("npm");
    }
  });
});
