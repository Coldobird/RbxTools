import { Timeout } from "./Timeout";

const VISIBLE_MS = 8_000;

/** The error toast's lifetime: shown errors clear themselves unless asked to stay. */
export class ErrorToast {
  private readonly expiry = new Timeout();

  constructor(private readonly display: (message: string | null) => void) {}

  /** Shows `message`, or clears the toast with `null`. */
  show(message: string | null, { stays = false } = {}): void {
    this.expiry.clear();
    this.display(message);
    if (message && !stays) this.expiry.start(VISIBLE_MS, () => this.display(null));
  }

  cancel(): void {
    this.expiry.clear();
  }
}
