export type TypeSize = "classic" | "large";

/** The UI test view panel's own settings, kept apart from the app's state. */
export interface TestViewPanelSettings {
  readonly panelOpen: boolean;
  readonly typeSize: TypeSize;
  /** The name of a `SelectionPresets` entry. */
  readonly selection: string;
}

export const defaultTestViewPanel: TestViewPanelSettings = {
  panelOpen: true,
  typeSize: "large",
  selection: "Peach",
};
