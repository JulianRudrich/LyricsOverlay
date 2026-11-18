//! The shortcut that shows and hides the overlay: checks a new one, registers
//! it, and keeps the old one working when another app already has the new one.

use std::sync::atomic::Ordering;

use serde::Serialize;
use tauri::AppHandle;
use tauri_plugin_global_shortcut::GlobalShortcutExt;

use crate::{commands, AppState};

/// Why a shortcut was not taken. The settings show a message for each.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum HotkeyError {
    /// Not one key combination: empty, two keys, or no key at all.
    Invalid,
    /// A key that cannot be part of a shortcut, such as Escape.
    Unsupported,
    /// A key that types, such as a letter, needs Ctrl, Alt or ⌘/Win with it.
    NeedsModifier,
    /// Another app has the shortcut, or the system keeps it.
    Taken,
}

/// The modifiers in the order shortcuts are written in, as macOS shows them: ⌃ ⌥ ⇧ ⌘.
const MODIFIERS: [&str; 4] = ["Ctrl", "Alt", "Shift", "Super"];

/// Keys besides letters, digits, F1 to F24 and the number pad's digits, named
/// as the browser's `KeyboardEvent.code` and the global-hotkey crate name them.
const NAMED_KEYS: [&str; 31] = [
    "Space",
    "Enter",
    "Tab",
    "Backspace",
    "Delete",
    "Insert",
    "Home",
    "End",
    "PageUp",
    "PageDown",
    "ArrowUp",
    "ArrowDown",
    "ArrowLeft",
    "ArrowRight",
    "Backquote",
    "Minus",
    "Equal",
    "BracketLeft",
    "BracketRight",
    "Backslash",
    "Semicolon",
    "Quote",
    "Comma",
    "Period",
    "Slash",
    "NumpadAdd",
    "NumpadSubtract",
    "NumpadMultiply",
    "NumpadDivide",
    "NumpadDecimal",
    "NumpadEnter",
];

/// Writes a shortcut the one way the settings keep it, such as `Ctrl+Alt+L`,
/// or says why it cannot be one. Modifiers may come in any order and case.
pub fn normalize(hotkey: &str) -> Result<String, HotkeyError> {
    let mut held = [false; MODIFIERS.len()];
    let mut key = None;
    for token in hotkey.split('+') {
        let token = token.trim();
        if token.is_empty() {
            return Err(HotkeyError::Invalid);
        }
        match modifier(token) {
            Some(index) => held[index] = true,
            None if key.is_some() => return Err(HotkeyError::Invalid),
            None => key = Some(key_name(token).ok_or(HotkeyError::Unsupported)?),
        }
    }
    let key = key.ok_or(HotkeyError::Invalid)?;
    // Shift alone would take a capital letter away from every app.
    let [ctrl, alt, _, command] = held;
    if !(ctrl || alt || command || is_function_key(&key)) {
        return Err(HotkeyError::NeedsModifier);
    }
    let mut parts: Vec<&str> = MODIFIERS
        .iter()
        .zip(held)
        .filter_map(|(name, on)| on.then_some(*name))
        .collect();
    parts.push(&key);
    Ok(parts.join("+"))
}

/// The index of a modifier in [`MODIFIERS`].
fn modifier(token: &str) -> Option<usize> {
    match token.to_ascii_lowercase().as_str() {
        "ctrl" | "control" => Some(0),
        "alt" | "option" => Some(1),
        "shift" => Some(2),
        "super" | "cmd" | "command" | "meta" => Some(3),
        "cmdorctrl" | "cmdorcontrol" | "commandorctrl" | "commandorcontrol" => {
            Some(if cfg!(target_os = "macos") { 3 } else { 0 })
        }
        _ => None,
    }
}

fn key_name(token: &str) -> Option<String> {
    let upper = token.to_ascii_uppercase();
    let single = upper
        .strip_prefix("KEY")
        .or_else(|| upper.strip_prefix("DIGIT"))
        .unwrap_or(&upper);
    if single.len() == 1 && single.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
        return Some(single.to_owned());
    }
    if is_function_key(&upper) {
        return Some(upper);
    }
    if let Some(digit) = upper
        .strip_prefix("NUMPAD")
        .filter(|rest| rest.len() == 1 && rest.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return Some(format!("Numpad{digit}"));
    }
    NAMED_KEYS
        .iter()
        .find(|name| name.eq_ignore_ascii_case(token))
        .map(|name| (*name).to_owned())
}

/// F1 to F24, which also work without a modifier.
fn is_function_key(key: &str) -> bool {
    key.strip_prefix('F')
        .and_then(|number| number.parse::<u8>().ok())
        .is_some_and(|number| (1..=24).contains(&number) && key == format!("F{number}"))
}

/// Registers the shortcut from the settings at startup. The settings show it
/// when another app has it.
pub fn register_saved(app: &AppHandle, state: &AppState, hotkey: &str) {
    let shortcuts = app.global_shortcut();
    let _ = shortcuts.unregister_all();
    let failed = match shortcuts.register(hotkey) {
        Ok(()) => false,
        Err(error) => {
            eprintln!("could not register the shortcut {hotkey}: {error}");
            true
        }
    };
    state.hotkey_error.store(failed, Ordering::SeqCst);
}

/// Changes the shortcut and keeps it in the settings. The new one is
/// registered before the old one is let go, so when another app has it, the
/// old one keeps working.
pub fn change(app: &AppHandle, state: &AppState, requested: &str) -> Result<String, HotkeyError> {
    let hotkey = normalize(requested)?;
    let current = state.settings().hotkey;
    let working = !state.hotkey_error.load(Ordering::SeqCst);
    if hotkey == current && working {
        return Ok(hotkey);
    }
    let shortcuts = app.global_shortcut();
    if let Err(error) = shortcuts.register(hotkey.as_str()) {
        eprintln!("could not register the shortcut {hotkey}: {error}");
        return Err(HotkeyError::Taken);
    }
    if hotkey != current {
        let _ = shortcuts.unregister(current.as_str());
    }
    state.hotkey_error.store(false, Ordering::SeqCst);
    state.update_settings(app, |settings| settings.hotkey = hotkey.clone());
    commands::emit_app_info(app, state);
    Ok(hotkey)
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use tauri_plugin_global_shortcut::Shortcut;

    use super::*;

    #[test]
    fn writes_shortcuts_one_way() {
        assert_eq!(normalize("Alt+Shift+L"), Ok("Alt+Shift+L".to_owned()));
        assert_eq!(
            normalize(" shift + alt + keyl "),
            Ok("Alt+Shift+L".to_owned())
        );
        assert_eq!(
            normalize("Super+Control+Option+Digit5"),
            Ok("Ctrl+Alt+Super+5".to_owned())
        );
        assert_eq!(
            normalize("Ctrl+Alt+numpad7"),
            Ok("Ctrl+Alt+Numpad7".to_owned())
        );
        assert_eq!(normalize("cmd+arrowup"), Ok("Super+ArrowUp".to_owned()));
        let expected = if cfg!(target_os = "macos") {
            "Shift+Super+K"
        } else {
            "Ctrl+Shift+K"
        };
        assert_eq!(normalize("CmdOrCtrl+Shift+KeyK"), Ok(expected.to_owned()));
    }

    #[test]
    fn function_keys_work_alone() {
        assert_eq!(normalize("F13"), Ok("F13".to_owned()));
        assert_eq!(normalize("shift+f5"), Ok("Shift+F5".to_owned()));
        assert_eq!(normalize("F25"), Err(HotkeyError::Unsupported));
        assert_eq!(normalize("F01"), Err(HotkeyError::Unsupported));
    }

    #[test]
    fn rejects_what_is_not_a_shortcut() {
        assert_eq!(normalize("L"), Err(HotkeyError::NeedsModifier));
        assert_eq!(normalize("Shift+L"), Err(HotkeyError::NeedsModifier));
        assert_eq!(normalize(""), Err(HotkeyError::Invalid));
        assert_eq!(normalize("Ctrl+"), Err(HotkeyError::Invalid));
        assert_eq!(normalize("Ctrl+Alt"), Err(HotkeyError::Invalid));
        assert_eq!(normalize("Ctrl+L+K"), Err(HotkeyError::Invalid));
        assert_eq!(normalize("Ctrl+Escape"), Err(HotkeyError::Unsupported));
        assert_eq!(normalize("Ctrl+Key"), Err(HotkeyError::Unsupported));
        assert_eq!(normalize("Ctrl+ä"), Err(HotkeyError::Unsupported));
    }

    #[test]
    fn every_key_registers() {
        let letters = ('A'..='Z').map(String::from);
        let digits = (0..=9).flat_map(|digit| [digit.to_string(), format!("Numpad{digit}")]);
        let functions = (1..=24).map(|number| format!("F{number}"));
        let named = NAMED_KEYS.iter().map(|name| (*name).to_owned());
        for key in letters.chain(digits).chain(functions).chain(named) {
            let hotkey = normalize(&format!("Ctrl+Alt+Shift+Super+{key}")).expect("valid");
            assert!(Shortcut::from_str(&hotkey).is_ok(), "{hotkey} parses");
        }
    }
}
