/** Keys that read better as a symbol or a word than as their name. */
const KEY_LABELS: Record<string, string> = {
  arrowup: "↑", up: "↑", arrowdown: "↓", down: "↓", arrowleft: "←", left: "←", arrowright: "→", right: "→",
  backquote: "`", minus: "-", equal: "=", bracketleft: "[", bracketright: "]", backslash: "\\",
  semicolon: ";", quote: "'", comma: ",", period: ".", slash: "/",
  space: "Space", tab: "Tab", enter: "Enter", backspace: "Backspace", delete: "Delete", insert: "Insert",
  home: "Home", end: "End", pageup: "Page Up", pagedown: "Page Down",
  numpadadd: "Num +", numpadsubtract: "Num −", numpadmultiply: "Num ×", numpaddivide: "Num ÷",
  numpaddecimal: "Num .", numpadenter: "Num Enter",
};

/** macOS writes some of them as symbols. */
const MAC_KEY_LABELS: Record<string, string> = {
  tab: "⇥", enter: "↩", backspace: "⌫", delete: "⌦", pageup: "⇞", pagedown: "⇟", home: "↖", end: "↘",
};

/** Shows a shortcut such as `Alt+Shift+L` the way the operating system writes it. */
export function hotkeyLabel(hotkey: string, platform: string): string {
  const parts = hotkey
    .split("+")
    .map((part) => part.trim())
    .filter(Boolean)
    .map((part) => part.replace(/^Key(?=[A-Z]$)/, "").replace(/^Digit(?=\d$)/, ""));
  const key = (part: string) => {
    const name = part.toLowerCase();
    const pad = /^numpad(\d)$/.exec(name);
    if (pad) return `Num ${pad[1]}`;
    return (platform === "macos" ? MAC_KEY_LABELS[name] : undefined) ?? KEY_LABELS[name] ?? part.toUpperCase();
  };
  if (platform === "macos") {
    const symbols: Record<string, string> = {
      alt: "⌥", option: "⌥", shift: "⇧", control: "⌃", ctrl: "⌃",
      super: "⌘", command: "⌘", cmd: "⌘", cmdorctrl: "⌘", commandorcontrol: "⌘",
    };
    return parts.map((part) => symbols[part.toLowerCase()] ?? key(part)).join(" ");
  }
  const names: Record<string, string> = {
    alt: "Alt", option: "Alt", shift: "Shift", control: "Ctrl", ctrl: "Ctrl",
    super: "Win", command: "Win", cmd: "Win", cmdorctrl: "Ctrl", commandorcontrol: "Ctrl",
  };
  return parts.map((part) => names[part.toLowerCase()] ?? key(part)).join(" + ");
}

/** The parts of a key press a shortcut is recorded from. */
export interface KeyPress {
  key: string;
  code: string;
  ctrlKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
  metaKey: boolean;
}

/** What one key press means while a new shortcut is recorded. */
export type Recording =
  /** Escape: keep the shortcut as it is. */
  | { kind: "cancel" }
  /** Only modifiers so far; they show while held. */
  | { kind: "held"; modifiers: string[] }
  /** A key that cannot be used, or a typing key without Ctrl, Alt or ⌘/Win. */
  | { kind: "problem"; problem: "unsupported" | "needsModifier" }
  /** A whole shortcut, written as the Rust side keeps it. */
  | { kind: "done"; hotkey: string };

/** Keys a shortcut may use besides letters, digits, F1 to F24 and the number pad's digits. */
const NAMED_KEYS = new Set([
  "Space", "Enter", "Tab", "Backspace", "Delete", "Insert", "Home", "End", "PageUp", "PageDown",
  "ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight", "Backquote", "Minus", "Equal", "BracketLeft",
  "BracketRight", "Backslash", "Semicolon", "Quote", "Comma", "Period", "Slash",
  "NumpadAdd", "NumpadSubtract", "NumpadMultiply", "NumpadDivide", "NumpadDecimal", "NumpadEnter",
]);

const FUNCTION_KEY = /^F([1-9]|1\d|2[0-4])$/;

/** In the order the Rust side writes them: ⌃ ⌥ ⇧ ⌘. */
function heldModifiers(press: KeyPress): string[] {
  const held: [boolean, string][] = [
    [press.ctrlKey, "Ctrl"],
    [press.altKey, "Alt"],
    [press.shiftKey, "Shift"],
    [press.metaKey, "Super"],
  ];
  return held.filter(([on]) => on).map(([, name]) => name);
}

/**
 * The key of a shortcut. macOS registers shortcuts by where a key sits, so the
 * key code counts there. Windows and Linux register letters and digits by what
 * they type, so on other keyboard layouts the typed character counts.
 */
function keyName(press: KeyPress, platform: string): string | null {
  const typed = /^(Key[A-Z]|Digit\d)$/.test(press.code) && /^[a-z0-9]$/i.test(press.key);
  if (platform !== "macos" && typed) return press.key.toUpperCase();
  const code = press.code;
  const letterOrDigit = /^(?:Key([A-Z])|Digit(\d))$/.exec(code);
  if (letterOrDigit) return letterOrDigit[1] ?? letterOrDigit[2] ?? null;
  if (FUNCTION_KEY.test(code) || /^Numpad\d$/.test(code) || NAMED_KEYS.has(code)) return code;
  return null;
}

/**
 * The modifiers still held after a key is let go while recording. Some engines
 * still report a modifier as held on its own release, so it is dropped here.
 */
export function releaseKey(release: KeyPress): string[] {
  const released: Record<string, string> = {
    Shift: "Shift", Control: "Ctrl", Alt: "Alt", AltGraph: "Alt", Meta: "Super", OS: "Super",
  };
  const name = released[release.key];
  return heldModifiers(release).filter((modifier) => modifier !== name);
}

/** Reads one key press while a new shortcut is recorded. */
export function recordKey(press: KeyPress, platform: string): Recording {
  const modifiers = heldModifiers(press);
  if (/^(Shift|Control|Alt|AltGraph|Meta|OS)(Left|Right)?$/.test(press.code) || /^(Shift|Control|Alt|AltGraph|Meta|OS)$/.test(press.key)) {
    return { kind: "held", modifiers };
  }
  if (press.code === "Escape" && modifiers.length === 0) return { kind: "cancel" };
  const key = keyName(press, platform);
  if (!key) return { kind: "problem", problem: "unsupported" };
  // Shift alone would take a capital letter away from every app.
  if (!(press.ctrlKey || press.altKey || press.metaKey) && !FUNCTION_KEY.test(key)) {
    return { kind: "problem", problem: "needsModifier" };
  }
  return { kind: "done", hotkey: [...modifiers, key].join("+") };
}
