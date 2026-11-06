/** Whether the app window is visible, and when it becomes visible again. */
export class PageVisibility {
  get isHidden(): boolean {
    return document.hidden;
  }

  /** Calls `listener` each time the page becomes visible; returns an unsubscribe function. */
  onVisible(listener: () => void): () => void {
    const handler = () => {
      if (!document.hidden) listener();
    };
    document.addEventListener("visibilitychange", handler);
    return () => document.removeEventListener("visibilitychange", handler);
  }
}
