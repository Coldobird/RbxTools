import { expect, it } from "vitest";
import { initialAppState } from "../AppState";
import { ToastView } from "./ToastView";

const live = initialAppState(false, true);

it("marks browser-preview errors as preview notes and offers updates only when one is found", () => {
  const failed = { ...live, error: "Restart is available in the installed desktop app." };
  expect(new ToastView(failed, { appVersion: "1.2.3", isDesktop: false }).isSystemError).toBe(false);
  expect(new ToastView(failed, { appVersion: "1.2.3", isDesktop: true }).isSystemError).toBe(true);
  const announced = { ...live, notice: { message: "RBX Tools v2.0.0 is available.", offersUpdate: true } };
  expect(new ToastView(announced, { appVersion: "1.2.3", isDesktop: true }).showsUpdateButton).toBe(false);
  const withUpdate = { ...announced, update: { ...announced.update, available: { version: "2.0.0" } } };
  const offered = new ToastView(withUpdate, { appVersion: "1.2.3", isDesktop: true });
  expect(offered.showsUpdateButton).toBe(true);
  expect(offered.update.canInstall).toBe(true);
});
