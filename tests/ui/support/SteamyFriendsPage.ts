import type { Locator, Page } from "@playwright/test";

/** The Steamy Friends tool page, and the window parts tests read around it. */
export class SteamyFriendsPage {
  constructor(private readonly page: Page) {}

  async open(): Promise<void> {
    await this.page.locator(".nav-item").filter({ hasText: "Steamy Friends" }).click();
  }

  /** A button by its exact accessible name. */
  button(name: string): Locator {
    return this.page.getByRole("button", { name, exact: true });
  }

  /** The restart card's status text. */
  get restartStatus(): Locator {
    return this.page.locator(".control-card").first().locator(".control-status > .ui-label");
  }

  /** The network card's status line. */
  get networkStatus(): Locator {
    return this.page.locator(".network-card .control-status");
  }

  get networkHeading(): Locator {
    return this.page.getByRole("heading", { name: "Network paused", exact: true });
  }

  get dialog(): Locator {
    return this.page.getByRole("dialog");
  }

  get errorToast(): Locator {
    return this.page.getByRole("alert");
  }

  get notices(): Locator {
    return this.page.getByRole("status");
  }
}
