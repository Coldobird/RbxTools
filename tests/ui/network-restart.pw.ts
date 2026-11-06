import { expect, test } from "@playwright/test";
import { SteamyFriendsPage } from "./support/SteamyFriendsPage";
import { TestViewPanel } from "./support/TestViewPanel";

test("last confirmed labels give way to a confirmed Steam shutdown", async ({ page }) => {
  const panel = await TestViewPanel.open(page);
  const steam = new SteamyFriendsPage(page);
  await panel.showPage("steamy");
  for (const state of ["online", "offline"]) {
    await panel.simulateSteam(`${state}-last-confirmed`);
    await expect(steam.networkStatus).toContainText(`Last confirmed ${state}`);
    await panel.showPage("library");
    await expect(page.locator(".art-status")).toContainText(`Last confirmed ${state}`);
    await panel.showPage("steamy");
    await panel.simulateSteam("stopped");
    await expect(steam.networkStatus).toContainText("Steam is not running");
    await expect(steam.networkStatus).not.toHaveAttribute("title");
  }
});

test("restart reports each phase and disables only its own action until UI loads", async ({ page }) => {
  await page.clock.install();
  const panel = await TestViewPanel.open(page);
  const steam = new SteamyFriendsPage(page);
  await panel.showPage("steamy");
  await panel.hide();
  await page.clock.pauseAt(new Date(Date.now() + 1000));
  await steam.button("Restart Steam").click();
  await expect(steam.restartStatus).toHaveText("Restarting…");
  await expect(steam.button("Restarting…")).toBeDisabled();
  await page.clock.runFor(650);
  await expect(steam.restartStatus).toHaveText("Loading UI…");
  await expect(steam.button("Loading UI…")).toBeDisabled();
  await expect(steam.button("Stop network")).toBeEnabled();
  await expect(steam.notices).toHaveCount(0);
  await page.clock.runFor(650);
  await expect(steam.restartStatus).toHaveText("Loaded");
  await expect(steam.button("Restart Steam")).toBeEnabled();
  await expect(steam.notices).toHaveCount(0);
});

for (const first of ["restart", "network"] as const) {
  test(`start and pause overlap with ${first} first without clearing the other action`, async ({ page }) => {
    await page.clock.install();
    const panel = await TestViewPanel.open(page);
    const steam = new SteamyFriendsPage(page);
    await panel.showPage("steamy");
    await panel.simulateSteam("stopped");
    await panel.hide();
    await page.clock.pauseAt(new Date(Date.now() + 1000));
    const start = steam.button("Start Steam");
    const pause = steam.button("Stop network");
    if (first === "restart") {
      await start.click();
      await expect(pause).toBeEnabled();
      await pause.click();
    } else {
      await pause.click();
      await expect(start).toBeEnabled();
      await start.click();
    }
    await expect(steam.button("Restarting…")).toBeDisabled();
    await expect(steam.button("Stop network")).toBeDisabled();
    await page.clock.runFor(650);
    await expect(steam.button("Loading UI…")).toBeDisabled();
    await expect(steam.button("Restore and reconnect")).toBeEnabled();
    await page.clock.runFor(650);
    await expect(steam.button("Restart Steam")).toBeEnabled();
    await expect(steam.networkHeading).toBeVisible();
    await expect(steam.networkStatus).toContainText("Steam offline");
    await panel.show();
    await expect(panel.choice("Steam state")).toHaveValue("paused");
  });
}

test("restoring the network during UI loading does not unlock restart early", async ({ page }) => {
  await page.clock.install();
  const panel = await TestViewPanel.open(page);
  const steam = new SteamyFriendsPage(page);
  await panel.showPage("steamy");
  await panel.simulateSteam("paused");
  await panel.hide();
  await page.clock.pauseAt(new Date(Date.now() + 1000));
  await steam.button("Restart Steam").click();
  await page.clock.runFor(650);
  await steam.button("Restore and reconnect").click();
  await expect(steam.button("Loading UI…")).toBeDisabled();
  await expect(steam.button("Restoring and reconnecting…")).toBeDisabled();
  await page.clock.runFor(650);
  await expect(steam.button("Restart Steam")).toBeEnabled();
  await expect(steam.button("Stop network")).toBeEnabled();
  await expect(steam.networkStatus).toContainText("Steam online");
});

for (const force of [false, true]) {
  test(`network stays paused through ${force ? "forced" : "normal"} restart until restored`, async ({ page }) => {
    const panel = await TestViewPanel.open(page);
    const steam = new SteamyFriendsPage(page);
    await panel.showPage("steamy");
    await panel.simulateSteam("online");
    await panel.hide();
    await steam.button("Stop network").click();
    const restore = steam.button("Restore and reconnect");
    await expect(restore).toBeEnabled();
    await expect(steam.networkHeading).toBeVisible();

    if (force) {
      await panel.briefly("Modal", "force");
      await expect(steam.dialog).toContainText("The network will stay paused.");
      await steam.button("Force restart").click();
    } else {
      await steam.button("Restart Steam").click();
    }

    await expect(steam.notices).toHaveCount(0);
    await expect(restore).toBeEnabled();
    await expect(steam.networkHeading).toBeVisible();
    await panel.show();
    await expect(panel.choice("Steam state")).toHaveValue("paused");
    await panel.hide();

    await restore.click();
    await expect(steam.button("Stop network")).toBeEnabled();
    await panel.show();
    await expect(panel.choice("Steam state")).toHaveValue("online");
  });
}
