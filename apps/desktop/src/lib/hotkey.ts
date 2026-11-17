/** Shows a shortcut such as `Alt+Shift+L` the way the operating system writes it. */
export function hotkeyLabel(hotkey: string, platform: string): string {
  const parts = hotkey
    .split("+")
    .map((part) => part.trim())
    .filter(Boolean)
    .map((part) => part.replace(/^Key(?=[A-Z]$)/, "").replace(/^Digit(?=\d$)/, ""));
  if (platform === "macos") {
    const symbols: Record<string, string> = {
      alt: "⌥", option: "⌥", shift: "⇧", control: "⌃", ctrl: "⌃",
      super: "⌘", command: "⌘", cmd: "⌘", cmdorctrl: "⌘", commandorcontrol: "⌘",
    };
    return parts.map((part) => symbols[part.toLowerCase()] ?? part.toUpperCase()).join(" ");
  }
  const names: Record<string, string> = {
    alt: "Alt", option: "Alt", shift: "Shift", control: "Ctrl", ctrl: "Ctrl",
    super: "Win", command: "Win", cmd: "Win", cmdorctrl: "Ctrl", commandorcontrol: "Ctrl",
  };
  return parts.map((part) => names[part.toLowerCase()] ?? part.toUpperCase()).join(" + ");
}
