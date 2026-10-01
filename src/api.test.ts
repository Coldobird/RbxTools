import { describe, expect, it } from "vitest";
import { fileName, getStatus, makeSpacewarPrivate, restartSteam, setBlocked } from "./api";

describe("browser preview boundary", () => {
  it("does not present fake Steam or network state", async () => {
    expect(await getStatus()).toMatchObject({
      steamPath: null,
      steamRunning: false,
      targetPath: null,
      blocked: false,
      elevated: false,
    });
  });
  it("cannot restart or block anything outside Tauri", async () => {
    await expect(restartSteam(false)).rejects.toThrow("desktop app");
    await expect(setBlocked(true)).rejects.toThrow("desktop app");
    await expect(makeSpacewarPrivate()).rejects.toThrow("desktop app");
  });
  it("displays Windows executable names", () => {
    expect(fileName("C:\\Program Files (x86)\\Steam\\steam.exe")).toBe(
      "steam.exe",
    );
    expect(fileName(null)).toBe("No executable selected");
  });
});
