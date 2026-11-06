/** Holds an update found at startup until no dialog or action is in the way of its dialog. */
export class StartupUpdate {
  private pending = false;

  hold(): void {
    this.pending = true;
  }

  drop(): void {
    this.pending = false;
  }

  /** Releases the held update once the window is clear; true when it should be shown now. */
  release(windowIsClear: boolean): boolean {
    if (!this.pending || !windowIsClear) return false;
    this.pending = false;
    return true;
  }
}
