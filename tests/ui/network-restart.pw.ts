import { expect, test } from "@playwright/test";

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
