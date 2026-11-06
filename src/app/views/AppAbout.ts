/** The app's identity, as the views present it. */
export interface AppAbout {
  readonly appVersion: string;
  /** Running inside the desktop app (not a browser preview). */
  readonly isDesktop: boolean;
}
