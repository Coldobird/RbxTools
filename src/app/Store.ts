/** An observable value: React reads it with `useSyncExternalStore(subscribe, getSnapshot)`. */
export class Store<T extends object> {
  private readonly listeners = new Set<() => void>();

  constructor(private value: T) {}

  readonly subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  };

  readonly getSnapshot = (): T => this.value;

  update(change: Partial<T>): void {
    this.replace({ ...this.value, ...change });
  }

  replace(value: T): void {
    this.value = value;
    for (const listener of this.listeners) listener();
  }
}
