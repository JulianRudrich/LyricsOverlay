//! The interface texts the Rust side shows itself, such as the tray menu. They
//! come from the same files as the web interface.

use std::collections::HashMap;

use crate::settings::Language;

const GERMAN: &str = include_str!("../../src/locales/de.json");
const ENGLISH: &str = include_str!("../../src/locales/en.json");

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Lang {
    De,
    En,
}

/// The language the interface uses: the chosen one, or the system's for "System".
pub fn resolve(language: Language) -> Lang {
    match language {
        Language::De => Lang::De,
        Language::En => Lang::En,
        Language::System => {
            let system = sys_locale::get_locale()
                .unwrap_or_default()
                .to_ascii_lowercase();
            if system.starts_with("de") {
                Lang::De
            } else {
                Lang::En
            }
        }
    }
}

pub struct Texts {
    texts: HashMap<String, String>,
    fallback: HashMap<String, String>,
}

impl Texts {
    pub fn new(lang: Lang) -> Self {
        let parse =
            |json: &str| serde_json::from_str::<HashMap<String, String>>(json).unwrap_or_default();
        Self {
            texts: parse(match lang {
                Lang::De => GERMAN,
                Lang::En => ENGLISH,
            }),
            fallback: parse(ENGLISH),
        }
    }

    pub fn get(&self, key: &str) -> String {
        self.texts
            .get(key)
            .or_else(|| self.fallback.get(key))
            .cloned()
            .unwrap_or_else(|| key.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_languages_parse_and_share_keys() {
        let parse = |json: &str| {
            serde_json::from_str::<HashMap<String, String>>(json).expect("valid locale file")
        };
        let mut german: Vec<_> = parse(GERMAN).into_keys().collect();
        let mut english: Vec<_> = parse(ENGLISH).into_keys().collect();
        german.sort();
        english.sort();
        assert_eq!(german, english);
    }

    #[test]
    fn looks_up_with_fallback() {
        assert_eq!(Texts::new(Lang::De).get("tray.quit"), "Beenden");
        assert_eq!(Texts::new(Lang::En).get("tray.quit"), "Quit");
        assert_eq!(Texts::new(Lang::De).get("missing.key"), "missing.key");
        assert_eq!(resolve(Language::De), Lang::De);
    }
}
