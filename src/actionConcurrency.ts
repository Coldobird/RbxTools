export function canStartAction(active: ReadonlySet<string>, next: string): boolean {
  if (active.has(next)) return false;
  if (active.size === 0) return true;
  const steamAction = (name: string) => name === "restart" || name === "network";
  return steamAction(next) && [...active].every(steamAction);
}
