/** Mirrors the backend's `ErrorKind`. */
export type DesktopErrorKind = "failed" | "busy" | "steam-still-running";

const kinds: readonly DesktopErrorKind[] = ["failed", "busy", "steam-still-running"];

/** A failed desktop call: a kind to react to, and a message for the user. */
export class DesktopError extends Error {
  constructor(readonly kind: DesktopErrorKind, message: string) {
    super(message);
    this.name = "DesktopError";
  }

  /** Normalizes anything thrown, including the backend's `{ kind, message }` errors. */
  static from(error: unknown): DesktopError {
    if (error instanceof DesktopError) return error;
    if (error instanceof Error) return new DesktopError("failed", error.message);
    if (typeof error === "object" && error !== null && "message" in error) {
      const { kind, message } = error as { kind?: unknown; message: unknown };
      const known = kinds.find(candidate => candidate === kind);
      return new DesktopError(known ?? "failed", String(message));
    }
    return new DesktopError("failed", String(error));
  }
}
