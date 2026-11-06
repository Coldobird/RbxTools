import { expect, test } from "@playwright/test";
import { MockDesktop } from "./support/MockDesktop";
import { SteamyFriendsPage } from "./support/SteamyFriendsPage";

test("stuck shutdown asks before forcing and supports cancel and retry", async ({ page }) => {
  const desktop = await MockDesktop.open(page, "restart");
  const steam = new SteamyFriendsPage(page);
  await steam.open();
  const restart = steam.button("Restart Steam");
  await expect(restart).toBeEnabled();
  await restart.click();
  await expect(steam.dialog).toContainText("Force restart Steam?");
  expect(await desktop.restartCalls()).toEqual([false]);
  await steam.button("Cancel").click();
  await expect(restart).toBeEnabled();
  await restart.click();
  await steam.button("Force restart").click();
  await expect(restart).toBeEnabled();
  await expect(steam.notices).toHaveCount(0);
  expect(await desktop.restartCalls()).toEqual([false, false, true]);
});

test("connection loss during reconnect keeps the restoring label until confirmation", async ({ page }) => {
  await page.clock.install();
  const desktop = await MockDesktop.open(page, "reconnect");
  const steam = new SteamyFriendsPage(page);
  await steam.open();
  await steam.button("Restore and reconnect").click();
  await expect(steam.networkStatus).toContainText("Restoring connection…");
  await page.clock.runFor(3100);
  await expect(steam.networkStatus).toContainText("Restoring connection…");
  await desktop.setOnline(true);
  await page.clock.runFor(3100);
  await expect(steam.networkStatus).toContainText("Steam online");
});

test("missing responses get 30 seconds of grace, including a fresh period after resume", async ({ page }) => {
  await page.clock.install();
  const desktop = await MockDesktop.open(page, "restart");
  const steam = new SteamyFriendsPage(page);
  await steam.open();
  await expect(steam.networkStatus).toContainText("Steam online");
  await desktop.setOnline(null);
  await page.clock.runFor(27_000);
  await expect(steam.networkStatus).toContainText("Steam online");
  await expect(steam.errorToast).toHaveCount(0);
  await page.clock.runFor(3100);
  await expect(steam.networkStatus).toContainText("Last confirmed online");
  await desktop.setHidden(true);
  await page.clock.runFor(120_000);
  await desktop.setHidden(false);
  await expect(steam.networkStatus).toContainText("Steam online");
  await page.clock.runFor(27_000);
  await expect(steam.networkStatus).toContainText("Steam online");
  await page.clock.runFor(3100);
  await expect(steam.networkStatus).toContainText("Last confirmed online");
});

test("pending status queries stay quiet for 30 seconds", async ({ page }) => {
  await page.clock.install();
  const desktop = await MockDesktop.open(page, "restart");
  const steam = new SteamyFriendsPage(page);
  await steam.open();
  await expect(steam.networkStatus).toContainText("Steam online");
  await desktop.keepStatusChecksPending();
  await page.clock.runFor(29_000);
  await expect(steam.networkStatus).toContainText("Steam online");
  await expect(steam.errorToast).toHaveCount(0);
  await page.clock.runFor(4100);
  await expect(steam.errorToast).toContainText("still pending");
  await expect(steam.networkStatus).toContainText("Last confirmed online");
});

test("interrupted update keeps its dialog open and allows another attempt", async ({ page }) => {
  const desktop = await MockDesktop.open(page, "update");
  const dialog = new SteamyFriendsPage(page).dialog;
  await expect(dialog).toContainText("Update available");
  await dialog.getByRole("button", { name: "Update", exact: true }).click();
  await expect(dialog.getByRole("button", { name: "Downloading…", exact: true })).toBeDisabled();
  await expect(dialog.getByRole("button", { name: "Close dialog", exact: true })).toBeDisabled();
  await desktop.failInstall("Could not read the update: lost connection");
  await expect(dialog.getByRole("alert")).toContainText("lost connection");
  const retry = dialog.getByRole("button", { name: "Update", exact: true });
  await expect(retry).toBeEnabled();
  await retry.click();
  await expect.poll(() => desktop.installCalls()).toBe(2);
  await desktop.failInstall("Second attempt interrupted");
  await expect(retry).toBeEnabled();
  await dialog.getByRole("button", { name: "Cancel", exact: true }).click();
  await expect(dialog).not.toBeVisible();
});
