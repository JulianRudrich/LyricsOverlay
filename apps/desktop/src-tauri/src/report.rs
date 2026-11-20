//! Report a problem: the bug report form on GitHub, filled in with what helps
//! to find the problem, and the log file to look at. Nothing is sent from
//! here: the listener reads the issue in the browser and submits it.

use std::fmt::Write;
use std::path::Path;

use media_session::{Service, SourcePreference};
use tauri::{AppHandle, Manager};
use tauri_plugin_opener::OpenerExt;

use crate::{logging, system, AppState};

/// The bug report form. The query fills in its fields by their ids in
/// .github/ISSUE_TEMPLATE/bug_report.yml.
const NEW_ISSUE: &str =
    "https://github.com/JulianRudrich/Singline/issues/new?template=bug_report.yml";
/// The form's field for the log, which goes last.
const LOG_FIELD: &str = "logs";
/// A line break, encoded.
const NEWLINE: &str = "%0A";
/// Longer addresses risk being cut off by the browser or refused by GitHub.
const MAX_URL: usize = 7_000;
/// More lines than fit, so the address decides how many go along.
const LOG_LINES: usize = 200;

/// Opens the bug report form in the browser, filled in with the version, the
/// system, the player and the last lines of the log.
#[tauri::command]
pub async fn report_problem(app: AppHandle) {
    let state = app.state::<AppState>();
    let source = state.settings().source;
    let player = state
        .playback
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .player
        .as_ref()
        .map(|player| player.name.clone());
    let fields = form_fields(
        app.package_info().version.to_string(),
        player_field(source, player.as_deref()),
    );
    let log = logging::file_path(&app)
        .map(|path| logging::last_lines(&path, LOG_LINES))
        .unwrap_or_default();
    if let Err(error) = app
        .opener()
        .open_url(issue_url(&fields, &log), None::<&str>)
    {
        log::error!("could not open the bug report form: {error}");
    }
}

/// Shows the log file in Finder or Explorer, creating it when it is missing.
#[tauri::command]
pub async fn show_log_file(app: AppHandle) {
    let Some(path) = logging::file_path(&app) else {
        log::error!("there is no folder for the log file");
        return;
    };
    if let Err(error) = logging::open(&path) {
        log::error!("could not create the log file: {error}");
        return;
    }
    if let Err(error) = reveal(&app, &path) {
        log::error!("could not show the log file: {error}");
    }
}

/// Selects the file in the file manager. Not every file manager on Linux
/// can, so there its folder opens instead.
fn reveal(app: &AppHandle, path: &Path) -> Result<(), tauri_plugin_opener::Error> {
    match (app.opener().reveal_item_in_dir(path), path.parent()) {
        (Err(_), Some(folder)) if cfg!(target_os = "linux") => app
            .opener()
            .open_path(folder.to_string_lossy(), None::<&str>),
        (revealed, _) => revealed,
    }
}

/// The fields Singline fills in, before the log. The system's name must be
/// one of the form's options.
fn form_fields(singline_version: String, player: String) -> [(&'static str, String); 4] {
    [
        ("system", system::name().to_owned()),
        ("system-version", system::version()),
        ("singline-version", singline_version),
        ("player", player),
    ]
}

/// The player Singline follows, if any, and the playback source chosen in
/// the settings.
fn player_field(source: SourcePreference, player: Option<&str>) -> String {
    let source = source.service().map_or("Automatic", Service::title);
    match player {
        Some(name) => format!("{name} (playback source: {source})"),
        None => format!("No player found (playback source: {source})"),
    }
}

/// The address of the filled-in form. The log goes last, without its oldest
/// lines when the address would reach MAX_URL characters.
fn issue_url(fields: &[(&str, String)], log: &[String]) -> String {
    let mut url = NEW_ISSUE.to_owned();
    for (name, value) in fields {
        let _ = write!(url, "&{name}={}", encode(value));
    }
    let lines: Vec<String> = log.iter().map(|line| encode(line)).collect();
    // The newest lines that fit, with a line break before all but the first.
    let mut length = url.len() + "&=".len() + LOG_FIELD.len();
    let mut kept = 0;
    for line in lines.iter().rev() {
        let added = line.len() + if kept == 0 { 0 } else { NEWLINE.len() };
        if length + added >= MAX_URL {
            break;
        }
        length += added;
        kept += 1;
    }
    if kept > 0 {
        let log = lines[lines.len() - kept..].join(NEWLINE);
        let _ = write!(url, "&{LOG_FIELD}={log}");
    }
    url
}

/// Percent-encodes everything but letters, digits and `-._~`, so any text
/// survives as a value in the query.
fn encode(text: &str) -> String {
    let mut encoded = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
            encoded.push(char::from(byte));
        } else {
            let _ = write!(encoded, "%{byte:02X}");
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fields() -> Vec<(&'static str, String)> {
        vec![
            ("system", "macOS".to_owned()),
            ("system-version", "macOS 15.2, aarch64".to_owned()),
            ("singline-version", "0.3.0".to_owned()),
            ("player", "Spotify (playback source: Automatic)".to_owned()),
        ]
    }

    #[test]
    fn values_are_percent_encoded() {
        assert_eq!(encode("macOS 15.2, aarch64"), "macOS%2015.2%2C%20aarch64");
        assert_eq!(encode("a&b=c#d+e?f/g%"), "a%26b%3Dc%23d%2Be%3Ff%2Fg%25");
        assert_eq!(encode("Grüße\n"), "Gr%C3%BC%C3%9Fe%0A");
        assert_eq!(encode("Az-09._~"), "Az-09._~");
    }

    #[test]
    fn the_address_fills_in_the_form() {
        let log = ["first line".to_owned(), "second: line".to_owned()];
        assert_eq!(
            issue_url(&fields(), &log),
            "https://github.com/JulianRudrich/Singline/issues/new?template=bug_report.yml\
             &system=macOS&system-version=macOS%2015.2%2C%20aarch64&singline-version=0.3.0\
             &player=Spotify%20%28playback%20source%3A%20Automatic%29\
             &logs=first%20line%0Asecond%3A%20line"
        );
        assert!(
            !issue_url(&fields(), &[]).contains("logs="),
            "without a log the field stays empty"
        );
    }

    #[test]
    fn the_oldest_log_lines_make_room() {
        let log: Vec<String> = (1..=300)
            .map(|number| {
                format!(
                    "2026-11-20T12:34:56.789Z INFO  singline::playback: line {number:03} {}",
                    "x".repeat(60)
                )
            })
            .collect();
        let url = issue_url(&fields(), &log);
        assert!(url.len() < MAX_URL, "{} characters", url.len());
        assert!(url.len() > MAX_URL - 150, "the log fills the room");
        let (_, logs) = url.split_once("&logs=").expect("the log goes along");
        assert!(!logs.contains('&'), "the log goes last");
        let kept: Vec<&str> = logs.split("%0A").collect();
        assert!(kept.len() < 300);
        for (line, number) in kept.iter().zip(301 - kept.len()..) {
            assert!(line.contains(&format!("line%20{number:03}%20")), "{line}");
        }
    }

    #[test]
    fn a_line_too_long_for_the_address_is_left_out() {
        let log = ["x".repeat(MAX_URL)];
        assert!(!issue_url(&fields(), &log).contains("logs="));
    }

    #[test]
    fn the_player_comes_with_the_playback_source() {
        assert_eq!(
            player_field(SourcePreference::Automatic, Some("Spotify")),
            "Spotify (playback source: Automatic)"
        );
        assert_eq!(
            player_field(SourcePreference::AppleMusic, None),
            "No player found (playback source: Apple Music)"
        );
    }

    #[test]
    fn the_fields_exist_in_the_bug_report_form() {
        let form = include_str!("../../../../.github/ISSUE_TEMPLATE/bug_report.yml");
        let fields = form_fields("0.3.0".into(), "Spotify".into());
        for id in fields.iter().map(|(id, _)| *id).chain([LOG_FIELD]) {
            assert!(form.contains(&format!("id: {id}\n")), "{id}");
        }
        assert!(
            form.contains(&format!("- {}\n", system::name())),
            "the system is one of the options"
        );
    }
}
