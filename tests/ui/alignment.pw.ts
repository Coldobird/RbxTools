import { expect, test, type Page } from "@playwright/test";
import { AlignmentReport, InkMeter } from "./support/InkMeter";
import { TestViewPanel } from "./support/TestViewPanel";

type TextSize = "classic" | "large";

/** Opens the UI test view at a window size and text size, with fonts loaded and the panel hidden. */
async function openTestView(page: Page, width: number, height: number, textSize: TextSize): Promise<TestViewPanel> {
  await page.setViewportSize({ width, height });
  const panel = await TestViewPanel.open(page);
  await page.emulateMedia({ reducedMotion: "reduce" });
  await page.evaluate(async () => { await document.fonts.ready; });
  await panel.choose("Text sizes", textSize);
  await panel.hide();
  await page.evaluate(async () => { await document.fonts.ready; });
  return panel;
}

const scenarios = [1212, 1000, 760].flatMap(width =>
  (["classic", "large"] as const).map(textSize => ({ width, height: width === 1212 ? 930 : 800, textSize })),
);

for (const scenario of scenarios) {
  test(`visible ink is centered at ${scenario.width}px in ${scenario.textSize} mode`, async ({ page }) => {
    const panel = await openTestView(page, scenario.width, scenario.height, scenario.textSize);
    const meter = new InkMeter(page);
    const report = new AlignmentReport();

    const slot = await meter.measure(page.locator(".slot-icon > :first-child"));
    const slotCenter = InkMeter.center(await InkMeter.box(page.locator(".slot-icon")));
    report.near(slot.center.x, slotCenter.x, "empty slot icon horizontal");
    report.near(slot.center.y, slotCenter.y, "empty slot icon vertical");

    const count = await meter.measure(page.locator(".nav-count > .ui-label"));
    const countCenter = InkMeter.center(await InkMeter.box(page.locator(".nav-count")));
    report.near(count.center.x, countCenter.x, "navigation numeral horizontal");
    report.near(count.center.y, countCenter.y, "navigation numeral vertical");

    const navIcons = page.locator(".nav-item > svg, .nav-item > img");
    await expect(navIcons).toHaveCount(3);
    for (const icon of await navIcons.all()) {
      const ink = await meter.measure(icon);
      const button = icon.locator("xpath=..");
      const label = await meter.measure(button.locator(":scope > .ui-label"));
      const iconType = await icon.evaluate(element => element.tagName.toLowerCase());
      report.near(ink.center.y, label.center.y, `navigation ${iconType} to visible label`);
      report.near(ink.center.y, InkMeter.center(await InkMeter.box(button)).y, `navigation ${iconType} to button center`);
    }

    const tags = page.locator(".tool-tags > span");
    await expect(tags).toHaveCount(2);
    for (const tag of await tags.all()) {
      const tagIcon = await meter.measure(tag.locator(":scope > svg"));
      const tagText = await meter.measure(tag.locator(".ui-label"));
      report.near(tagIcon.center.y, tagText.center.y, "tool tag icon to visible label");
    }

    await page.locator(".nav-item").filter({ hasText: "Steamy Friends" }).click();
    for (const state of ["online", "offline", "connecting", "paused"] as const) {
      await panel.briefly("Steam state", state);
      const networkIcon = page.locator(".network-card .control-status > .status-icon");
      await expect(networkIcon).toHaveCount(1);
      await expect(networkIcon).toBeVisible();
      for (const row of await page.locator(".control-status").all()) {
        const iconLocator = row.locator(":scope > .status-icon");
        if (await iconLocator.count() === 0) continue;
        const icon = await meter.measure(iconLocator);
        const label = await meter.measure(row.locator(":scope > .ui-label"));
        report.near(icon.center.y, label.center.y, `control status ${state} icon to visible label`);
        report.near(icon.center.y, InkMeter.center(await InkMeter.box(row)).y, `control status ${state} icon to row center`);
      }
    }
    console.log(`Alignment at ${scenario.width}px (${scenario.textSize}): ${report.summary()}`);
    report.assertAligned();
  });
}

test("ink measurement detects a deliberate four-pixel displacement", async ({ page }) => {
  await openTestView(page, 1212, 930, "large");
  const meter = new InkMeter(page);
  const target = page.locator(".slot-icon > :first-child");
  const before = await meter.measure(target);
  const slotCenter = InkMeter.center(await InkMeter.box(page.locator(".slot-icon")));
  await target.evaluate(element => { (element as HTMLElement).style.transform = "translateY(4px)"; });
  const after = await meter.measure(target);
  expect(after.center.y - before.center.y, "negative control displacement").toBeGreaterThanOrEqual(3.5);
  expect(Math.abs(after.center.y - slotCenter.y), "alignment check rejects the displaced ink").toBeGreaterThan(AlignmentReport.TOLERANCE_PX);
});
