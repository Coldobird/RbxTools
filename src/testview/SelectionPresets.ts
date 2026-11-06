export interface SelectionPreset {
  readonly name: string;
  readonly background: string;
  readonly color: string;
}

/** Text-selection colours the UI test view can preview, from the app palette. */
export class SelectionPresets {
  static readonly all: readonly SelectionPreset[] = [
    { name: "Peach", background: "#ffd0bc", color: "#29151c" },
    { name: "Cream", background: "#fff4db", color: "#29151c" },
    { name: "Gold", background: "#ffd05a", color: "#29151c" },
    { name: "Coral", background: "#ff9a8a", color: "#29151c" },
    { name: "Rose", background: "#d97b82", color: "#190e15" },
    { name: "Dusty pink", background: "#b8636d", color: "#fff4db" },
    { name: "Red", background: "#c14f5c", color: "#fff4db" },
    { name: "Burgundy", background: "#743039", color: "#fff4db" },
    { name: "Plum", background: "#512733", color: "#ffd0bc" },
    { name: "Ink", background: "#29151c", color: "#ffd05a" },
  ];

  /** The preset called `name`, or the first one. */
  static named(name: string): SelectionPreset {
    return SelectionPresets.all.find(preset => preset.name === name) ?? SelectionPresets.all[0];
  }
}
