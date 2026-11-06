import { expect, it } from "vitest";
import { initialAppState } from "../AppState";
import { SettingsView } from "./SettingsView";

const live = initialAppState(false, true);
const testView = initialAppState(true, false);

it("names the edition outside the test view", () => {
  expect(new SettingsView(live, { appVersion: "1.2.3", isDesktop: true }).footnote).toBe("RBX Tools v1.2.3 · Windows desktop edition");
  expect(new SettingsView(live, { appVersion: "1.2.3", isDesktop: false }).footnote).toBe("RBX Tools v1.2.3 · Browser preview · no system changes");
  expect(new SettingsView(testView, { appVersion: "1.2.3", isDesktop: true }).footnote).toBe("RBX Tools v1.2.3");
});

it("says when Steam has not been found", () => {
  expect(new SettingsView(live, { appVersion: "1.2.3", isDesktop: true }).steamPathLabel).toBe("Not detected yet");
});
