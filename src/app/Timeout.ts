/** A single pending callback that can be replaced or cancelled. */
export class Timeout {
  private handle: ReturnType<typeof setTimeout> | null = null;

  start(delay: number, callback: () => void): void {
    this.clear();
    this.handle = setTimeout(() => {
      this.handle = null;
      callback();
    }, delay);
  }

  clear(): void {
    if (this.handle !== null) clearTimeout(this.handle);
    this.handle = null;
  }
}
