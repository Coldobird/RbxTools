import type { Locator, Page } from "@playwright/test";

/** The UI test view's control panel. */
export class TestViewPanel {
  constructor(private readonly page: Page) {}

  /** Opens the app in the UI test view. */
  static async open(page: Page): Promise<TestViewPanel> {
    await page.goto("/?ui-test=1");
    return new TestViewPanel(page);
  }

  choice(label: string): Locator {
    return this.page.getByLabel(label, { exact: true });
  }

  async choose(label: string, value: string): Promise<void> {
    await this.choice(label).selectOption(value);
  }

  async showPage(page: "library" | "steamy"): Promise<void> {
    await this.choose("Page", page);
  }

  async simulateSteam(state: string): Promise<void> {
    await this.choose("Steam state", state);
  }

  async hide(): Promise<void> {
    await this.page.getByRole("button", { name: "Hide controls", exact: true }).click();
  }

  async show(): Promise<void> {
    await this.page.getByRole("button", { name: "Show controls", exact: true }).click();
  }

  /** Shows the panel, changes one choice, and hides it again. */
  async briefly(label: string, value: string): Promise<void> {
    await this.show();
    await this.choose(label, value);
    await this.hide();
  }
}
