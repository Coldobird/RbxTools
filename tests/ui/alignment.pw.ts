import { expect, test, type Locator, type Page } from "@playwright/test";
import { PNG } from "pngjs";

type InkMeasurement = {
  rect: { x: number; y: number; width: number; height: number };
  ink: { x: number; y: number; width: number; height: number };
  center: { x: number; y: number };
  samples: number;
};

/** Find rendered ink by differencing the target's screenshot against the same
 * element with visibility:hidden. This measures glyph/icon pixels, not its box. */
async function measureInk(locator: Locator): Promise<InkMeasurement> {
  await expect(locator).toBeVisible();
  const rect = await locator.evaluate((element) => {
    const { x, y, width, height } = element.getBoundingClientRect();
    return { x, y, width, height };
  });
  const clip = await locator.evaluate((element) => {
    const parent = element.parentElement ?? element;
    const { x, y, right, bottom } = parent.getBoundingClientRect();
    const left = Math.floor(x);
    const top = Math.floor(y);
    return { x: left, y: top, width: Math.ceil(right) - left, height: Math.ceil(bottom) - top };
  });
  const visible = PNG.sync.read(await locator.page().screenshot({ clip, animations: "disabled", caret: "hide" }));
  try {
    await locator.evaluate((element) => { (element as HTMLElement).style.visibility = "hidden"; });
    const hidden = PNG.sync.read(await locator.page().screenshot({ clip, animations: "disabled", caret: "hide" }));
    const result = inkBounds(visible, hidden, clip, rect);
    return result;
  } finally {
    await locator.evaluate((element) => { (element as HTMLElement).style.removeProperty("visibility"); });
  }
}

function inkBounds(visible: PNG, hidden: PNG, clip: { x: number; y: number }, rect: InkMeasurement["rect"]): InkMeasurement {
  let minX = visible.width;
  let minY = visible.height;
  let maxX = -1;
  let maxY = -1;
  let samples = 0;
  for (let y = 0; y < visible.height; y += 1) {
    for (let x = 0; x < visible.width; x += 1) {
      const i = (visible.width * y + x) * 4;
      const delta = Math.abs(visible.data[i] - hidden.data[i])
        + Math.abs(visible.data[i + 1] - hidden.data[i + 1])
        + Math.abs(visible.data[i + 2] - hidden.data[i + 2])
        + Math.abs(visible.data[i + 3] - hidden.data[i + 3]);
      if (delta < 24) continue;
      minX = Math.min(minX, x);
      minY = Math.min(minY, y);
      maxX = Math.max(maxX, x);
      maxY = Math.max(maxY, y);
      samples += 1;
    }
  }
  if (samples === 0) throw new Error("No visible ink found in the target element.");
  const ink = { x: clip.x + minX, y: clip.y + minY, width: maxX - minX + 1, height: maxY - minY + 1 };
  return { rect, ink, center: { x: ink.x + ink.width / 2, y: ink.y + ink.height / 2 }, samples };
}

function recordNear(failures: string[], measurements: string[], actual: number, expected: number, tolerance: number, subject: string) {
  const delta = actual - expected;
  measurements.push(`${subject}=${delta.toFixed(2)}px`);
  if (Math.abs(delta) > tolerance) failures.push(`${subject}: visible-ink center delta ${delta.toFixed(2)}px (allowed ±${tolerance}px)`);
}

async function ready(page: Page, width: number, height: number, textSize: "classic" | "large") {
  await page.setViewportSize({ width, height });
  await page.goto("/?ui-test=1");
  await page.emulateMedia({ reducedMotion: "reduce" });
  await page.evaluate(async () => { await document.fonts.ready; });
  await page.getByLabel("Text sizes").selectOption(textSize);
  await page.getByRole("button", { name: "Hide controls" }).click();
  await page.evaluate(async () => { await document.fonts.ready; });
}

const scenarios = [1212, 1000, 760].flatMap((width) =>
  (["classic", "large"] as const).map((textSize) => ({
    width,
    height: width === 1212 ? 930 : 800,
    textSize,
  })),
);

for (const scenario of scenarios) {
  test(`visible ink is centered at ${scenario.width}px in ${scenario.textSize} mode`, async ({ page }) => {
    await ready(page, scenario.width, scenario.height, scenario.textSize);
    const failures: string[] = [];
    const measurements: string[] = [];

    const slot = await measureInk(page.locator(".slot-icon > :first-child"));
    const slotBox = await page.locator(".slot-icon").evaluate((element) => {
      const { x, y, width, height } = element.getBoundingClientRect();
      return { x, y, width, height };
    });
    recordNear(failures, measurements, slot.center.x, slotBox.x + slotBox.width / 2, 1, "empty slot icon horizontal");
    recordNear(failures, measurements, slot.center.y, slotBox.y + slotBox.height / 2, 1, "empty slot icon vertical");

    const count = await measureInk(page.locator(".nav-count > .ui-label"));
    const countBox = await page.locator(".nav-count").evaluate((element) => {
      const { x, y, width, height } = element.getBoundingClientRect();
      return { x, y, width, height };
    });
    recordNear(failures, measurements, count.center.x, countBox.x + countBox.width / 2, 1, "navigation numeral horizontal");
    recordNear(failures, measurements, count.center.y, countBox.y + countBox.height / 2, 1, "navigation numeral vertical");

    const navIcons = page.locator(".nav-item > svg, .nav-item > img");
    await expect(navIcons).toHaveCount(3);
    for (const icon of await navIcons.all()) {
      const ink = await measureInk(icon);
      const button = icon.locator("xpath=..");
      const label = await measureInk(button.locator(":scope > .ui-label"));
      const iconType = await icon.evaluate((element) => element.tagName.toLowerCase());
      recordNear(failures, measurements, ink.center.y, label.center.y, 1, `navigation ${iconType} to visible label`);
      const buttonBox = await button.evaluate((element) => {
        const { y, height } = element.getBoundingClientRect();
        return { y, height };
      });
      recordNear(failures, measurements, ink.center.y, buttonBox.y + buttonBox.height / 2, 1, `navigation ${iconType} to button center`);
    }

    const tags = page.locator(".tool-tags > span");
    await expect(tags).toHaveCount(2);
    for (const tag of await tags.all()) {
      const tagIcon = await measureInk(tag.locator(":scope > svg"));
      const tagText = await measureInk(tag.locator(".ui-label"));
      recordNear(failures, measurements, tagIcon.center.y, tagText.center.y, 1, "tool tag icon to visible label");
    }

    await page.locator(".nav-item").filter({ hasText: "Steamy Friends" }).click();
    for (const state of ["online", "offline", "connecting", "blocked"] as const) {
      await page.getByRole("button", { name: "Show controls" }).click();
      await page.getByLabel("Steam state").selectOption(state);
      await page.getByRole("button", { name: "Hide controls" }).click();
      const networkIcon = page.locator(".network-card .control-status > .status-icon");
      await expect(networkIcon).toHaveCount(1);
      await expect(networkIcon).toBeVisible();
      for (const row of await page.locator(".control-status").all()) {
        const iconLocator = row.locator(":scope > .status-icon");
        if (await iconLocator.count() === 0) continue;
        const icon = await measureInk(iconLocator);
        const label = await measureInk(row.locator(":scope > .ui-label"));
        recordNear(failures, measurements, icon.center.y, label.center.y, 1, `control status ${state} icon to visible label`);
        const rowBox = await row.evaluate((element) => {
          const { y, height } = element.getBoundingClientRect();
          return { y, height };
        });
        recordNear(failures, measurements, icon.center.y, rowBox.y + rowBox.height / 2, 1, `control status ${state} icon to row center`);
      }
    }
    console.log(`Alignment at ${scenario.width}px (${scenario.textSize}): ${measurements.join(", ")}`);
    expect(failures, failures.join("\n")).toEqual([]);
  });
}

test("ink measurement detects a deliberate four-pixel displacement", async ({ page }) => {
  await ready(page, 1212, 930, "large");
  const target = page.locator(".slot-icon > :first-child");
  const before = await measureInk(target);
  const box = await page.locator(".slot-icon").evaluate((element) => {
    const { y, height } = element.getBoundingClientRect();
    return { y, height };
  });
  await target.evaluate((element) => { (element as HTMLElement).style.transform = "translateY(4px)"; });
  const after = await measureInk(target);
  expect(after.center.y - before.center.y, "negative control displacement").toBeGreaterThanOrEqual(3.5);
  expect(Math.abs(after.center.y - (box.y + box.height / 2)), "alignment check rejects the displaced ink").toBeGreaterThan(1);
});
