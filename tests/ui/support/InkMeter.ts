import { expect, type Locator, type Page } from "@playwright/test";
import { PNG } from "pngjs";

export interface Box {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface InkMeasurement {
  rect: Box;
  ink: Box;
  center: { x: number; y: number };
  samples: number;
}

/** Pixel difference below which two screenshots count as unchanged. */
const INK_THRESHOLD = 24;

/**
 * Measures rendered ink: the pixels an element actually draws, found by
 * differencing a screenshot of it against the same area with the element
 * hidden. This measures glyph and icon pixels, not CSS boxes.
 */
export class InkMeter {
  constructor(private readonly page: Page) {}

  /** The bounding box of an element's CSS layout. */
  static async box(locator: Locator): Promise<Box> {
    return locator.evaluate(element => {
      const { x, y, width, height } = element.getBoundingClientRect();
      return { x, y, width, height };
    });
  }

  static center(box: Box): { x: number; y: number } {
    return { x: box.x + box.width / 2, y: box.y + box.height / 2 };
  }

  async measure(locator: Locator): Promise<InkMeasurement> {
    await expect(locator).toBeVisible();
    const rect = await InkMeter.box(locator);
    const clip = await locator.evaluate(element => {
      const parent = element.parentElement ?? element;
      const { x, y, right, bottom } = parent.getBoundingClientRect();
      const left = Math.floor(x);
      const top = Math.floor(y);
      return { x: left, y: top, width: Math.ceil(right) - left, height: Math.ceil(bottom) - top };
    });
    const visible = await this.capture(clip);
    try {
      await locator.evaluate(element => { (element as HTMLElement).style.visibility = "hidden"; });
      return InkMeter.difference(visible, await this.capture(clip), clip, rect);
    } finally {
      await locator.evaluate(element => { (element as HTMLElement).style.removeProperty("visibility"); });
    }
  }

  private async capture(clip: Box): Promise<PNG> {
    return PNG.sync.read(await this.page.screenshot({ clip, animations: "disabled", caret: "hide" }));
  }

  private static difference(visible: PNG, hidden: PNG, clip: Box, rect: Box): InkMeasurement {
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
        if (delta < INK_THRESHOLD) continue;
        minX = Math.min(minX, x);
        minY = Math.min(minY, y);
        maxX = Math.max(maxX, x);
        maxY = Math.max(maxY, y);
        samples += 1;
      }
    }
    if (samples === 0) throw new Error("No visible ink found in the target element.");
    const ink = { x: clip.x + minX, y: clip.y + minY, width: maxX - minX + 1, height: maxY - minY + 1 };
    return { rect, ink, center: InkMeter.center(ink), samples };
  }
}

/** Collects alignment checks, each allowed one CSS pixel, and fails with all of them at once. */
export class AlignmentReport {
  static readonly TOLERANCE_PX = 1;

  private readonly failures: string[] = [];
  private readonly measurements: string[] = [];

  near(actual: number, expected: number, subject: string): void {
    const delta = actual - expected;
    this.measurements.push(`${subject}=${delta.toFixed(2)}px`);
    if (Math.abs(delta) > AlignmentReport.TOLERANCE_PX) {
      this.failures.push(`${subject}: visible-ink center delta ${delta.toFixed(2)}px (allowed ±${AlignmentReport.TOLERANCE_PX}px)`);
    }
  }

  summary(): string {
    return this.measurements.join(", ");
  }

  assertAligned(): void {
    expect(this.failures, this.failures.join("\n")).toEqual([]);
  }
}
