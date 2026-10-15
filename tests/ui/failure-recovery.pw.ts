import { expect, test, type Page } from "@playwright/test";

type FailureState = {
  blocked: boolean;
  online: boolean | null;
  statusError: string | null;
  reconnectError: string | null;
  restartCalls: boolean[];
  installCalls: number;
  failInstall: (error: string) => void;
};

async function mockDesktop(page: Page, scenario: "restart" | "reconnect" | "update") {
  await page.addInitScript(({ scenario }) => {
    const state = {
      blocked: scenario === "reconnect",
      online: scenario !== "reconnect",
      statusError: null as string | null,
      reconnectError: null as string | null,
      restartCalls: [] as boolean[],
      installCalls: 0,
      failInstall: (_error: string) => {},
    };
    const status = () => ({
      steamPath: "C:\\Steam\\steam.exe", targetPath: "C:\\Steam\\steam.exe",
      steamRunning: true, steamOnline: state.online,
      blocked: state.blocked, reconnectError: state.reconnectError, elevated: true,
    });
    let callbackId = 0;
    // All desktop commands stay inside this mock, including failed installs.
    Object.assign(window, {
      isTauri: true,
      failureState: state,
      __TAURI_INTERNALS__: {
        transformCallback: () => ++callbackId,
        unregisterCallback: () => {},
        invoke: async (command: string, args: { force?: boolean; blocked?: boolean; progress?: { onmessage: (phase: string) => void } }) => {
          switch (command) {
            case "get_status":
              if (state.statusError) throw state.statusError;
              return status();
            case "get_spacewar_privacy": return false;
            case "get_github_update": return { update: scenario === "update" ? { version: "99.0.0" } : null };
            case "restart_steam":
              state.restartCalls.push(!!args.force);
              if (!args.force) throw "STEAM_STILL_RUNNING";
              args.progress?.onmessage("loading-ui");
              state.online = true;
              return "Steam restarted.";
            case "set_blocked":
              state.blocked = !!args.blocked;
              return status();
            case "install_github_update":
              state.installCalls++;
              return new Promise<void>((_resolve, reject) => { state.failInstall = reject; });
            default: throw new Error(`Unexpected desktop command: ${command}`);
          }
        },
      },
    });
  }, { scenario });
  await page.goto("/");
}

test("stuck shutdown asks before forcing and supports cancel and retry", async ({ page }) => {
  await mockDesktop(page, "restart");
  await page.locator(".nav-item").filter({ hasText: "Steamy Friends" }).click();
  const restart = page.getByRole("button", { name: "Restart Steam", exact: true });
  await expect(restart).toBeEnabled();
  await restart.click();
  await expect(page.getByRole("dialog")).toContainText("Force restart Steam?");
  expect(await page.evaluate(() => (window as unknown as { failureState: FailureState }).failureState.restartCalls)).toEqual([false]);
  await page.getByRole("button", { name: "Cancel", exact: true }).click();
  await expect(restart).toBeEnabled();
  await restart.click();
  await page.getByRole("button", { name: "Force restart", exact: true }).click();
  await expect(restart).toBeEnabled();
  await expect(page.getByRole("status")).toHaveCount(0);
  expect(await page.evaluate(() => (window as unknown as { failureState: FailureState }).failureState.restartCalls)).toEqual([false, false, true]);
});

test("connection loss during reconnect keeps the restoring label until a failure or confirmation", async ({ page }) => {
  await page.clock.install();
  await mockDesktop(page, "reconnect");
  await page.locator(".nav-item").filter({ hasText: "Steamy Friends" }).click();
  await page.getByRole("button", { name: "Restore and reconnect", exact: true }).click();
  const networkStatus = page.locator(".network-card .control-status");
  await expect(networkStatus).toContainText("Restoring connection…");
  await page.evaluate(() => {
    (window as unknown as { failureState: FailureState }).failureState.reconnectError = "Steam reconnect requests finished, but Steam is still offline.";
  });
  await page.clock.runFor(3100);
  await expect(page.getByRole("alert")).toContainText("Steam is still offline");
  await expect(page.getByRole("button", { name: "Stop network", exact: true })).toBeEnabled();
  await page.evaluate(() => {
    const state = (window as unknown as { failureState: FailureState }).failureState;
    state.online = true;
    state.reconnectError = null;
  });
  await page.clock.runFor(3100);
  await expect(networkStatus).toContainText("Steam online");
});

test("missing responses get 30 seconds of grace, including a fresh period after resume", async ({ page }) => {
  await page.clock.install();
  await mockDesktop(page, "restart");
  await page.locator(".nav-item").filter({ hasText: "Steamy Friends" }).click();
  const row = page.locator(".network-card .control-status");
  await expect(row).toContainText("Steam online");
  await page.evaluate(() => { (window as unknown as { failureState: FailureState }).failureState.online = null; });
  await page.clock.runFor(27_000);
  await expect(row).toContainText("Steam online");
  await expect(page.getByRole("alert")).toHaveCount(0);
  await page.clock.runFor(3100);
  await expect(row).toContainText("Last confirmed online");
  await page.evaluate(() => {
    Object.defineProperty(document, "hidden", { configurable: true, value: true });
    document.dispatchEvent(new Event("visibilitychange"));
  });
  await page.clock.runFor(120_000);
  await page.evaluate(() => {
    Object.defineProperty(document, "hidden", { configurable: true, value: false });
    document.dispatchEvent(new Event("visibilitychange"));
  });
  await expect(row).toContainText("Steam online");
  await page.clock.runFor(27_000);
  await expect(row).toContainText("Steam online");
  await page.clock.runFor(3100);
  await expect(row).toContainText("Last confirmed online");
});

test("pending status queries stay quiet for 30 seconds", async ({ page }) => {
  await page.clock.install();
  await mockDesktop(page, "restart");
  await page.locator(".nav-item").filter({ hasText: "Steamy Friends" }).click();
  const row = page.locator(".network-card .control-status");
  await expect(row).toContainText("Steam online");
  await page.evaluate(() => {
    (window as unknown as { failureState: FailureState }).failureState.statusError = "Steam status check is still pending.";
  });
  await page.clock.runFor(29_000);
  await expect(row).toContainText("Steam online");
  await expect(page.getByRole("alert")).toHaveCount(0);
  await page.clock.runFor(4100);
  await expect(page.getByRole("alert")).toContainText("still pending");
  await expect(row).toContainText("Last confirmed online");
});

test("interrupted update keeps its dialog open and allows another attempt", async ({ page }) => {
  await mockDesktop(page, "update");
  const dialog = page.getByRole("dialog");
  await expect(dialog).toContainText("Update available");
  await dialog.getByRole("button", { name: "Update", exact: true }).click();
  await expect(dialog.getByRole("button", { name: "Downloading…", exact: true })).toBeDisabled();
  await expect(dialog.getByRole("button", { name: "Close dialog", exact: true })).toBeDisabled();
  await page.evaluate(() => (window as unknown as { failureState: FailureState }).failureState.failInstall("Could not read the update: lost connection"));
  await expect(dialog.getByRole("alert")).toContainText("lost connection");
  const retry = dialog.getByRole("button", { name: "Update", exact: true });
  await expect(retry).toBeEnabled();
  await retry.click();
  await expect.poll(() => page.evaluate(() => (window as unknown as { failureState: FailureState }).failureState.installCalls)).toBe(2);
  await page.evaluate(() => (window as unknown as { failureState: FailureState }).failureState.failInstall("Second attempt interrupted"));
  await expect(retry).toBeEnabled();
  await dialog.getByRole("button", { name: "Cancel", exact: true }).click();
  await expect(dialog).not.toBeVisible();
});
