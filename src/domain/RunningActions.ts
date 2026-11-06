/** Every user action that can keep the app busy. */
export type ActionName = "restart" | "network" | "path" | "check-update" | "install-update";

/** Steam actions that may overlap each other, but nothing else. */
const steamActions: ReadonlySet<ActionName> = new Set(["restart", "network"]);

/**
 * The actions running now, oldest first, and which may start next. Restart
 * and network control can overlap; every other action runs alone. Immutable:
 * starting or finishing an action returns a new value.
 */
export class RunningActions {
  static readonly none = new RunningActions([]);

  private constructor(private readonly names: readonly ActionName[]) {
    Object.freeze(this);
  }

  /** Exactly these actions, as the UI test view simulates them. */
  static of(...names: ActionName[]): RunningActions {
    return new RunningActions([...new Set(names)]);
  }

  canStart(next: ActionName): boolean {
    if (this.has(next)) return false;
    if (this.isIdle) return true;
    return steamActions.has(next) && this.names.every(name => steamActions.has(name));
  }

  with(name: ActionName): RunningActions {
    return this.has(name) ? this : new RunningActions([...this.names, name]);
  }

  without(name: ActionName): RunningActions {
    return new RunningActions(this.names.filter(running => running !== name));
  }

  has(name: ActionName): boolean {
    return this.names.includes(name);
  }

  get isIdle(): boolean {
    return this.names.length === 0;
  }

  get oldest(): ActionName | null {
    return this.names[0] ?? null;
  }

  /** An action other than the two Steam controls is running. */
  get hasNonSteamAction(): boolean {
    return this.names.some(name => !steamActions.has(name));
  }
}
