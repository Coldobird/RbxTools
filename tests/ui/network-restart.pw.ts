import { expect, test } from "@playwright/test";

test("last confirmed labels give way to a confirmed Steam shutdown", async ({ page }) => {
  await page.goto("/?ui-test=1");
  await page.getByLabel("Page", { exact: true }).selectOption("steamy");
  const row = page.locator(".network-card .control-status");
  for (const state of ["online", "offline"]) {
    await page.getByLabel("Steam state", { exact: true }).selectOption(`${state}-last-confirmed`);
    await expect(row).toContainText(`Last confirmed ${state}`);
    await page.getByLabel("Page", { exact: true }).selectOption("library");
    await expect(page.locator(".art-status")).toContainText(`Last confirmed ${state}`);
    await page.getByLabel("Page", { exact: true }).selectOption("steamy");
    await page.getByLabel("Steam state", { exact: true }).selectOption("stopped");
    await expect(row).toContainText("Steam is not running");
    await expect(row).not.toHaveAttribute("title");
  }
});

test("restart reports each phase and disables only its own action until UI loads", async ({ page }) => {
  await page.clock.install();
  await page.goto("/?ui-test=1");
  await page.getByLabel("Page", { exact: true }).selectOption("steamy");
  await page.getByRole("button", { name: "Hide controls", exact: true }).click();
  await page.clock.pauseAt(new Date(Date.now() + 1000));
  await page.getByRole("button", { name: "Restart Steam", exact: true }).click();
  const status = page.locator(".control-card").first().locator(".control-status > .ui-label");
  await expect(status).toHaveText("Restarting…");
  await expect(page.getByRole("button", { name: "Restarting…", exact: true })).toBeDisabled();
  await page.clock.runFor(650);
  await expect(status).toHaveText("Loading UI…");
  await expect(page.getByRole("button", { name: "Loading UI…", exact: true })).toBeDisabled();
  await expect(page.getByRole("button", { name: "Stop network", exact: true })).toBeEnabled();
  await expect(page.getByRole("status")).toHaveCount(0);
  await page.clock.runFor(650);
  await expect(status).toHaveText("Loaded");
  await expect(page.getByRole("button", { name: "Restart Steam", exact: true })).toBeEnabled();
  await expect(page.getByRole("status")).toContainText("Steam restarted");
});

for (const first of ["restart", "network"] as const) {
  test(`start and pause overlap with ${first} first without clearing the other action`, async ({ page }) => {
    await page.clock.install();
    await page.goto("/?ui-test=1");
    await page.getByLabel("Page", { exact: true }).selectOption("steamy");
    await page.getByLabel("Steam state", { exact: true }).selectOption("stopped");
    await page.getByRole("button", { name: "Hide controls", exact: true }).click();
    await page.clock.pauseAt(new Date(Date.now() + 1000));
    const start = page.getByRole("button", { name: "Start Steam", exact: true });
    const pause = page.getByRole("button", { name: "Stop network", exact: true });
    if (first === "restart") {
      await start.click();
      await expect(pause).toBeEnabled();
      await pause.click();
    } else {
      await pause.click();
      await expect(start).toBeEnabled();
      await start.click();
    }
    await expect(page.getByRole("button", { name: "Restarting…", exact: true })).toBeDisabled();
    await expect(page.getByRole("button", { name: "Stop network", exact: true })).toBeDisabled();
    await page.clock.runFor(650);
    await expect(page.getByRole("button", { name: "Loading UI…", exact: true })).toBeDisabled();
    await expect(page.getByRole("button", { name: "Restore and reconnect", exact: true })).toBeEnabled();
    await page.clock.runFor(650);
    await expect(page.getByRole("button", { name: "Restart Steam", exact: true })).toBeEnabled();
    await expect(page.getByRole("heading", { name: "Network paused", exact: true })).toBeVisible();
    await expect(page.locator(".network-card .control-status")).toContainText("Incoming and outgoing traffic blocked");
    await page.getByRole("button", { name: "Show controls", exact: true }).click();
    await expect(page.getByLabel("Steam state", { exact: true })).toHaveValue("blocked");
  });
}

test("restoring the network during UI loading does not unlock restart early", async ({ page }) => {
  await page.clock.install();
  await page.goto("/?ui-test=1");
  await page.getByLabel("Page", { exact: true }).selectOption("steamy");
  await page.getByLabel("Steam state", { exact: true }).selectOption("blocked");
  await page.getByRole("button", { name: "Hide controls", exact: true }).click();
  await page.clock.pauseAt(new Date(Date.now() + 1000));
  await page.getByRole("button", { name: "Restart Steam", exact: true }).click();
  await page.clock.runFor(650);
  await page.getByRole("button", { name: "Restore and reconnect", exact: true }).click();
  await expect(page.getByRole("button", { name: "Loading UI…", exact: true })).toBeDisabled();
  await expect(page.getByRole("button", { name: "Restoring and reconnecting…", exact: true })).toBeDisabled();
  await page.clock.runFor(650);
  await expect(page.getByRole("button", { name: "Restart Steam", exact: true })).toBeEnabled();
  await expect(page.getByRole("button", { name: "Stop network", exact: true })).toBeEnabled();
  await expect(page.locator(".network-card .control-status")).toContainText("Steam online");
});

for (const force of [false, true]) {
  test(`network stays paused through ${force ? "forced" : "normal"} restart until restored`, async ({ page }) => {
    await page.goto("/?ui-test=1");
    await page.getByLabel("Page", { exact: true }).selectOption("steamy");
    await page.getByLabel("Steam state", { exact: true }).selectOption("online");
    await page.getByRole("button", { name: "Hide controls", exact: true }).click();
    await page.getByRole("button", { name: "Stop network", exact: true }).click();
    const restore = page.getByRole("button", { name: "Restore and reconnect", exact: true });
    await expect(restore).toBeEnabled();
    await expect(page.getByRole("heading", { name: "Network paused", exact: true })).toBeVisible();

    if (force) {
      await page.getByRole("button", { name: "Show controls", exact: true }).click();
      await page.getByLabel("Modal", { exact: true }).selectOption("force");
      await page.getByRole("button", { name: "Hide controls", exact: true }).click();
      await expect(page.getByRole("dialog")).toContainText("The network will stay paused.");
      await page.getByRole("button", { name: "Force restart", exact: true }).click();
    } else {
      await page.getByRole("button", { name: "Restart Steam", exact: true }).click();
    }

    await expect(page.getByRole("status")).toContainText("Steam restarted");
    await expect(restore).toBeEnabled();
    await expect(page.getByRole("heading", { name: "Network paused", exact: true })).toBeVisible();
    await page.getByRole("button", { name: "Show controls", exact: true }).click();
    await expect(page.getByLabel("Steam state", { exact: true })).toHaveValue("blocked");
    await page.getByRole("button", { name: "Hide controls", exact: true }).click();

    await restore.click();
    await expect(page.getByRole("button", { name: "Stop network", exact: true })).toBeEnabled();
    await page.getByRole("button", { name: "Show controls", exact: true }).click();
    await expect(page.getByLabel("Steam state", { exact: true })).toHaveValue("online");
  });
}
