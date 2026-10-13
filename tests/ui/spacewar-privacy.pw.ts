import { expect, test, type Page } from "@playwright/test";

async function openSettings(page: Page, privacy: string, steamState = "online") {
  await page.goto("/?ui-test=1");
  await page.getByLabel("Steam state", { exact: true }).selectOption(steamState);
  await page.getByLabel("Spacewar privacy state", { exact: true }).selectOption(privacy);
  await page.getByLabel("Modal", { exact: true }).selectOption("settings");
  await expect(page.getByRole("dialog")).toBeVisible();
}

test("Spacewar preview toggles privacy and retains it when Settings reopens", async ({ page }) => {
  await openSettings(page, "public");

  const makePrivate = page.getByRole("button", { name: "Make Private", exact: true });
  await expect(makePrivate).toBeEnabled();
  await makePrivate.click();
  await expect(page.getByRole("button", { name: "Make Public", exact: true })).toBeEnabled();

  await page.getByRole("button", { name: "Make Public", exact: true }).click();
  await expect(makePrivate).toBeEnabled();

  await page.getByRole("button", { name: "Close dialog", exact: true }).click();
  await page.getByLabel("Modal", { exact: true }).selectOption("settings");
  await expect(makePrivate).toBeEnabled();
});

test("Spacewar preview handles checking, retry, blocked, and stopped states", async ({ page }) => {
  await openSettings(page, "checking");

  const checking = page.getByRole("button", { name: "Checking…", exact: true });
  await expect(checking).toBeDisabled();

  await page.getByLabel("Spacewar privacy state", { exact: true }).selectOption("error");
  await expect(page.getByRole("alert")).toHaveText("Could not check Spacewar privacy. Try again.");
  const retry = page.getByRole("button", { name: "Retry check", exact: true });
  await expect(retry).toBeEnabled();
  await retry.click();
  await expect(page.getByRole("button", { name: "Make Private", exact: true })).toBeEnabled();

  await page.getByLabel("Steam state", { exact: true }).selectOption("blocked");
  const blockedAction = page.getByRole("button", { name: "Make Private", exact: true });
  await expect(blockedAction).toBeDisabled();
  await expect(page.getByText("Restore Steam’s network access first.", { exact: true })).toBeVisible();

  await page.getByLabel("Steam state", { exact: true }).selectOption("stopped");
  await expect(blockedAction).toBeDisabled();
  await expect(page.getByText("Open Steam and sign in first.", { exact: true })).toBeVisible();
});

for (const width of [760, 1000, 1212]) {
  test(`Spacewar Settings description fits at ${width}px`, async ({ page }) => {
    await page.setViewportSize({ width, height: 800 });
    await openSettings(page, "public");

    const group = page.getByRole("group", { name: "Spacewar privacy", exact: true });
    await expect(group.locator("p")).toHaveText(
      "We use Spacewar to reconnect to Steam faster. Make it private so friends won’t see you playing it.",
    );
    const dimensions = await group.evaluate(element => {
      const modal = element.closest<HTMLElement>(".modal");
      return {
        viewportWidth: window.innerWidth,
        documentWidth: document.documentElement.scrollWidth,
        groupClientWidth: element.clientWidth,
        groupScrollWidth: element.scrollWidth,
        contentClientWidth: (element.querySelector("div") as HTMLElement).clientWidth,
        contentScrollWidth: (element.querySelector("div") as HTMLElement).scrollWidth,
        paragraphClientWidth: (element.querySelector("p") as HTMLElement).clientWidth,
        paragraphScrollWidth: (element.querySelector("p") as HTMLElement).scrollWidth,
        modalClientWidth: modal?.clientWidth,
        modalScrollWidth: modal?.scrollWidth,
      };
    });

    expect(dimensions.documentWidth).toBeLessThanOrEqual(dimensions.viewportWidth);
    expect(dimensions.groupScrollWidth).toBeLessThanOrEqual(dimensions.groupClientWidth);
    expect(dimensions.contentScrollWidth).toBeLessThanOrEqual(dimensions.contentClientWidth);
    expect(dimensions.paragraphScrollWidth).toBeLessThanOrEqual(dimensions.paragraphClientWidth);
    expect(dimensions.modalScrollWidth).toBeLessThanOrEqual(dimensions.modalClientWidth!);
  });
}
