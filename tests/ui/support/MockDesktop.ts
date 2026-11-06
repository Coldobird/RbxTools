import type { Page } from "@playwright/test";

export type Scenario = "restart" | "reconnect" | "update";

/** The fake Steam the mock backend serves, as seen from the browser. */
interface MockState {
  networkPaused: boolean;
  online: boolean | null;
  statusError: { kind: string; message: string } | null;
  restartCalls: boolean[];
  installCalls: number;
  failInstall: (error: string) => void;
}

type MockWindow = Window & { rbxMock: MockState };

/**
 * A stand-in desktop backend installed into the page before it loads. Every
 * desktop command stays inside this mock, including failed installs.
 */
export class MockDesktop {
  private constructor(private readonly page: Page) {}

  /** Installs the mock for `scenario` and opens the app. */
  static async open(page: Page, scenario: Scenario): Promise<MockDesktop> {
    await page.addInitScript(({ scenario }) => {
      const state: MockState = {
        networkPaused: scenario === "reconnect",
        online: scenario !== "reconnect",
        statusError: null,
        restartCalls: [],
        installCalls: 0,
        failInstall: () => {},
      };
      const status = () => ({
        steamPath: "C:\\Steam\\steam.exe",
        steamRunning: true,
        steamOnline: state.online,
        networkPaused: state.networkPaused,
      });
      let callbackId = 0;
      Object.assign(window, {
        isTauri: true,
        rbxMock: state,
        __TAURI_INTERNALS__: {
          transformCallback: () => ++callbackId,
          unregisterCallback: () => {},
          invoke: async (command: string, args: { force?: boolean; paused?: boolean; progress?: { onmessage: (phase: string) => void } }) => {
            switch (command) {
              case "get_status":
                if (state.statusError) throw state.statusError;
                return status();
              case "find_update":
                return { update: scenario === "update" ? { version: "99.0.0" } : null };
              case "restart_steam":
                state.restartCalls.push(!!args.force);
                if (!args.force) throw { kind: "steam-still-running", message: "Steam is still running." };
                args.progress?.onmessage("loading-ui");
                state.online = true;
                return;
              case "set_network_paused":
                state.networkPaused = !!args.paused;
                return status();
              case "install_update":
                state.installCalls++;
                return new Promise<void>((_resolve, reject) => { state.failInstall = reject; });
              case "log_timing":
                return;
              default:
                throw new Error(`Unexpected desktop command: ${command}`);
            }
          },
        },
      });
    }, { scenario });
    await page.goto("/");
    return new MockDesktop(page);
  }

  restartCalls(): Promise<boolean[]> {
    return this.page.evaluate(() => (window as unknown as MockWindow).rbxMock.restartCalls);
  }

  installCalls(): Promise<number> {
    return this.page.evaluate(() => (window as unknown as MockWindow).rbxMock.installCalls);
  }

  /** What the next status checks report: online, offline, or no reading (`null`). */
  async setOnline(online: boolean | null): Promise<void> {
    await this.page.evaluate(online => { (window as unknown as MockWindow).rbxMock.online = online; }, online);
  }

  /** Makes status checks fail with a busy error. */
  async keepStatusChecksPending(): Promise<void> {
    await this.page.evaluate(() => {
      (window as unknown as MockWindow).rbxMock.statusError = { kind: "busy", message: "Steam status check is still pending." };
    });
  }

  /** Ends the running install with `error`. */
  async failInstall(error: string): Promise<void> {
    await this.page.evaluate(error => (window as unknown as MockWindow).rbxMock.failInstall(error), error);
  }

  /** Hides or shows the window, as minimizing it would. */
  async setHidden(hidden: boolean): Promise<void> {
    await this.page.evaluate(hidden => {
      Object.defineProperty(document, "hidden", { configurable: true, value: hidden });
      document.dispatchEvent(new Event("visibilitychange"));
    }, hidden);
  }
}
