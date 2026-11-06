/**
 * The UI test view's switch: real components with simulated actions. While it
 * is on, the desktop bridge refuses every system action.
 */
export class TestViewMode {
  constructor(private active: boolean) {}

  /** Reads the `?ui-test=1` activation flag from a URL query string. */
  static requestedBy(search: string): boolean {
    return new URLSearchParams(search).get("ui-test") === "1";
  }

  get isActive(): boolean {
    return this.active;
  }

  set(active: boolean): void {
    this.active = active;
  }
}
