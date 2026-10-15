import { expect, test, type Page } from "@playwright/test";

// Mock the desktop bridge: these tests must never contact Steam or change files.
async function mockDesktop(page: Page, running = true) {
  await page.addInitScript(({ running }) => {
    const mock = {
      reads: 0,
      running,
      complete: (_value: boolean) => {},
      fail: (_message: string) => {},
    };
    Object.assign(window, {
      isTauri: true,
      privacyMock: mock,
      __TAURI_INTERNALS__: {
        invoke: async (command: string, args: { private?: boolean }) => {
          if (command === "get_status") return {
            steamPath: "C:\\Steam\\steam.exe", targetPath: "C:\\Steam\\steam.exe",
            steamRunning: mock.running, steamOnline: mock.running,
            blocked: false, elevated: true,
          };
          if (command === "get_github_update") return { update: null };
          if (command === "get_spacewar_privacy") {
            mock.reads++;
            return new Promise<boolean>((resolve, reject) => {
              mock.complete = resolve;
              mock.fail = reject;
            });
          }
          if (command === "set_spacewar_privacy") return args.private;
          throw new Error(`Unexpected desktop command: ${command}`);
        },
      },
    });
  }, { running });
  await page.goto("/");
}

type PrivacyMock = {
  reads: number;
  running: boolean;
  complete: (value: boolean) => void;
  fail: (message: string) => void;
};
const readCount = (page: Page) => page.evaluate(() =>
  (window as unknown as { privacyMock: PrivacyMock }).privacyMock.reads);

test("startup privacy check survives Settings reopening and caches reads and changes", async ({ page }) => {
  await mockDesktop(page);
  await expect.poll(() => readCount(page)).toBe(1);
  const settings = page.getByRole("button", { name: "Settings", exact: true });
  await settings.click();
  await expect(page.getByRole("button", { name: "Checking…", exact: true })).toBeDisabled();
  await page.getByRole("button", { name: "Close dialog", exact: true }).click();
  await settings.click();
  expect(await readCount(page)).toBe(1);
  await page.evaluate(() => (window as unknown as { privacyMock: PrivacyMock }).privacyMock.complete(false));
  await page.getByRole("button", { name: "Make Private", exact: true }).click();
  await expect(page.getByRole("button", { name: "Make Public", exact: true })).toBeEnabled();
  await page.getByRole("button", { name: "Close dialog", exact: true }).click();
  await settings.click();
  await expect(page.getByRole("button", { name: "Make Public", exact: true })).toBeEnabled();
  expect(await readCount(page)).toBe(1);
});

test("failed startup check waits for manual retry", async ({ page }) => {
  await mockDesktop(page);
  await expect.poll(() => readCount(page)).toBe(1);
  await page.evaluate(() => (window as unknown as { privacyMock: PrivacyMock }).privacyMock.fail("Steam did not respond"));
  const settings = page.getByRole("button", { name: "Settings", exact: true });
  await settings.click();
  await expect(page.getByRole("alert")).toHaveText("Steam did not respond");
  await page.getByRole("button", { name: "Close dialog", exact: true }).click();
  await settings.click();
  expect(await readCount(page)).toBe(1);
  await page.getByRole("button", { name: "Retry check", exact: true }).click();
  await expect.poll(() => readCount(page)).toBe(2);
  await page.evaluate(() => (window as unknown as { privacyMock: PrivacyMock }).privacyMock.complete(true));
  await expect(page.getByRole("button", { name: "Make Public", exact: true })).toBeEnabled();
});

test("startup check waits until Steam is running", async ({ page }) => {
  await page.clock.install();
  await mockDesktop(page, false);
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await expect(page.getByText("Open Steam and sign in first.", { exact: true })).toBeVisible();
  expect(await readCount(page)).toBe(0);
  await page.evaluate(() => { (window as unknown as { privacyMock: PrivacyMock }).privacyMock.running = true; });
  await page.clock.runFor(3100);
  await expect.poll(() => readCount(page)).toBe(1);
  await page.evaluate(() => (window as unknown as { privacyMock: PrivacyMock }).privacyMock.complete(false));
  await expect(page.getByRole("button", { name: "Make Private", exact: true })).toBeEnabled();
});
