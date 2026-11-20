//! The log file: what Singline did in its last sessions, so a problem can be
//! found later. It stays on this device. Report a problem copies its last
//! lines into a GitHub issue, which the listener reads before sending it.
//!
//! Since those lines can become public, the log never holds tokens, client
//! IDs, the Genius token or lyrics text, and song titles and artists appear
//! only at debug level.
//!
//! Info and above go to the file, debug lines too with SINGLINE_DEBUG. Every
//! line is also printed to stderr, so a terminal still shows it. Once the
//! file is over 1 MB it becomes `singline.old.log`, replacing the one before.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use log::{LevelFilter, Log, Metadata, Record};
use tauri::{AppHandle, Manager};

const FILE_NAME: &str = "singline.log";
const OLD_FILE_NAME: &str = "singline.old.log";
/// A file larger than this is moved aside before the next line.
const MAX_SIZE: u64 = 1024 * 1024;

/// The target of lines for the end-to-end checks, such as the line the
/// overlay shows. They go to stderr as they are, without time or level, and
/// never into the file, as they hold lyrics, song titles and pairing codes.
pub const CHECKS: &str = "singline::checks";

/// The crates of this workspace. Other crates add only their warnings and
/// errors: their info and debug lines are many and say little about Singline.
const OWN_CRATES: [&str; 6] = [
    "singline",
    "lyrics_core",
    "lyrics_providers",
    "media_session",
    "spotify_connect",
    "transcription",
];

/// Logs to stderr and to the log file from now on, panics included. Call it
/// first thing at startup.
pub fn init(app: &AppHandle) {
    let level = if std::env::var_os("SINGLINE_DEBUG").is_some() {
        LevelFilter::Debug
    } else {
        LevelFilter::Info
    };
    let logger = Logger {
        level,
        path: file_path(app),
        file: Mutex::new(None),
    };
    if log::set_logger(Box::leak(Box::new(logger))).is_ok() {
        log::set_max_level(level);
    }
    log_panics();
}

/// Where the log goes: the system's folder for app logs, or `logs` in
/// SINGLINE_DATA_DIR, which tests set.
pub fn file_path(app: &AppHandle) -> Option<PathBuf> {
    match std::env::var_os("SINGLINE_DATA_DIR") {
        Some(directory) => Some(PathBuf::from(directory).join("logs").join(FILE_NAME)),
        None => app.path().app_log_dir().ok().map(|dir| dir.join(FILE_NAME)),
    }
}

/// Opens the log for appending, creating it and its folder when missing.
pub fn open(path: &Path) -> io::Result<File> {
    if let Some(directory) = path.parent() {
        fs::create_dir_all(directory)?;
    }
    OpenOptions::new().create(true).append(true).open(path)
}

/// The last `count` lines of the log, oldest first; none without a log.
pub fn last_lines(path: &Path, count: usize) -> Vec<String> {
    let Ok(bytes) = fs::read(path) else {
        return Vec::new();
    };
    let mut lines: Vec<String> = String::from_utf8_lossy(&bytes)
        .lines()
        .rev()
        .take(count)
        .map(str::to_owned)
        .collect();
    lines.reverse();
    lines
}

/// Whether a log of `size` bytes is moved aside before the next line.
fn is_full(size: u64) -> bool {
    size > MAX_SIZE
}

/// Whether a line comes from Singline's own code rather than a library.
fn is_own(target: &str) -> bool {
    let name = target.split("::").next().unwrap_or(target);
    OWN_CRATES.contains(&name)
}

/// One line of the log, such as
/// `2026-11-20T12:34:56.789Z WARN  singline::spotify: message`.
fn line(time: &str, record: &Record) -> String {
    format!(
        "{time} {:<5} {}: {}\n",
        record.level(),
        record.target(),
        record.args()
    )
}

/// Logs a panic with where it happened, then lets the usual hook report it.
fn log_panics() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let payload = info.payload();
        let message = payload
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
            .unwrap_or("no message");
        let location = info
            .location()
            .map_or_else(|| "an unknown place".to_owned(), ToString::to_string);
        let thread = std::thread::current();
        log::error!(
            "thread '{}' panicked at {location}: {message}",
            thread.name().unwrap_or("unnamed")
        );
        previous(info);
    }));
}

struct Logger {
    level: LevelFilter,
    /// `None` when the system has no folder for logs; then only stderr gets the lines.
    path: Option<PathBuf>,
    file: Mutex<Option<LogFile>>,
}

impl Logger {
    /// Appends a line to the file, opening it first, or again once it is full.
    fn append(&self, line: &str) {
        let Some(path) = &self.path else {
            return;
        };
        let mut file = self.file.lock().unwrap_or_else(|p| p.into_inner());
        if file.as_ref().is_none_or(|open| is_full(open.size)) {
            // Closed first, so it is never moved aside while open.
            *file = None;
            *file = LogFile::open(path).ok();
        }
        if let Some(open) = file.as_mut() {
            open.append(line);
        }
    }
}

impl Log for Logger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        let level = if is_own(metadata.target()) {
            self.level
        } else {
            LevelFilter::Warn
        };
        metadata.level() <= level
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        if record.target() == CHECKS {
            let _ = io::stderr().write_all(format!("{}\n", record.args()).as_bytes());
            return;
        }
        let time = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        let line = line(&time, record);
        let _ = io::stderr().write_all(line.as_bytes());
        self.append(&line);
    }

    /// Every line is flushed as it is written.
    fn flush(&self) {}
}

/// The open log file and how many bytes it holds.
struct LogFile {
    file: File,
    size: u64,
}

impl LogFile {
    /// Opens the log, after moving a full one aside.
    fn open(path: &Path) -> io::Result<Self> {
        if fs::metadata(path).is_ok_and(|metadata| is_full(metadata.len())) {
            // When this fails, the lines go on into the full file.
            let _ = fs::rename(path, path.with_file_name(OLD_FILE_NAME));
        }
        let file = open(path)?;
        let size = file.metadata()?.len();
        Ok(Self { file, size })
    }

    fn append(&mut self, line: &str) {
        let written = self
            .file
            .write_all(line.as_bytes())
            .and_then(|()| self.file.flush());
        if written.is_ok() {
            self.size += line.len() as u64;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use log::Level;

    fn directory(name: &str) -> PathBuf {
        let directory =
            std::env::temp_dir().join(format!("singline-log-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        directory
    }

    #[test]
    fn lines_carry_the_time_the_level_and_the_module() {
        let warning = line(
            "2026-11-20T12:34:56.789Z",
            &Record::builder()
                .level(Level::Warn)
                .target("singline::spotify")
                .args(format_args!("the login stays only until quit"))
                .build(),
        );
        assert_eq!(
            warning,
            "2026-11-20T12:34:56.789Z WARN  singline::spotify: the login stays only until quit\n"
        );
        let error = line(
            "2026-11-20T12:34:56.789Z",
            &Record::builder()
                .level(Level::Error)
                .target("media_session::bridge::server")
                .args(format_args!("no port"))
                .build(),
        );
        assert_eq!(
            error,
            "2026-11-20T12:34:56.789Z ERROR media_session::bridge::server: no port\n"
        );
    }

    #[test]
    fn reads_the_last_lines_oldest_first() {
        let directory = directory("tail");
        let path = directory.join(FILE_NAME);
        assert!(last_lines(&path, 200).is_empty(), "no log yet");

        fs::create_dir_all(&directory).expect("created");
        fs::write(&path, "one\ntwo\n").expect("written");
        assert_eq!(last_lines(&path, 200), ["one", "two"]);

        let long: String = (1..=500).map(|number| format!("line {number}\n")).collect();
        fs::write(&path, long).expect("written");
        let last = last_lines(&path, 200);
        assert_eq!(last.len(), 200);
        assert_eq!(
            (last[0].as_str(), last[199].as_str()),
            ("line 301", "line 500")
        );
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn only_a_file_over_one_megabyte_is_full() {
        assert!(!is_full(0));
        assert!(!is_full(MAX_SIZE));
        assert!(is_full(MAX_SIZE + 1));
    }

    #[test]
    fn a_full_log_starts_over_and_keeps_the_last_one() {
        let directory = directory("rotation");
        let path = directory.join(FILE_NAME);
        let old = directory.join(OLD_FILE_NAME);
        fs::create_dir_all(&directory).expect("created");
        fs::write(&old, "the session before\n").expect("written");

        fs::write(&path, "small\n").expect("written");
        let small = LogFile::open(&path).expect("opened");
        assert_eq!(small.size, 6, "a small log goes on");
        drop(small);

        fs::write(&path, vec![b'x'; MAX_SIZE as usize + 1]).expect("written");
        let mut fresh = LogFile::open(&path).expect("opened");
        assert_eq!(fresh.size, 0);
        fresh.append("first line\n");
        assert_eq!(fresh.size, 11);
        assert_eq!(fs::read_to_string(&path).expect("read"), "first line\n");
        assert_eq!(
            fs::metadata(&old).expect("kept").len(),
            MAX_SIZE + 1,
            "the full log replaces the older one"
        );
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn the_logger_starts_a_new_file_once_one_is_full() {
        let directory = directory("logger");
        let path = directory.join("logs").join(FILE_NAME);
        let logger = Logger {
            level: LevelFilter::Info,
            path: Some(path.clone()),
            file: Mutex::new(None),
        };
        logger.append("one\n");
        logger.append(&"x".repeat(MAX_SIZE as usize));
        assert!(fs::read_to_string(&path)
            .expect("read")
            .starts_with("one\nxxx"));

        logger.append("two\n");
        assert_eq!(fs::read_to_string(&path).expect("read"), "two\n");
        let old = fs::read_to_string(directory.join("logs").join(OLD_FILE_NAME)).expect("kept");
        assert!(old.starts_with("one\nxxx"));
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn libraries_add_only_warnings_and_errors() {
        let logger = |level| Logger {
            level,
            path: None,
            file: Mutex::new(None),
        };
        let enabled = |logger: &Logger, level, target| {
            logger.enabled(&Metadata::builder().level(level).target(target).build())
        };
        let debug = logger(LevelFilter::Debug);
        assert!(enabled(&debug, Level::Debug, "singline::sing"));
        assert!(enabled(&debug, Level::Debug, CHECKS));
        assert!(enabled(&debug, Level::Debug, "media_session::bridge"));
        assert!(!enabled(&debug, Level::Debug, "rustls::client"));
        assert!(!enabled(&debug, Level::Info, "tao::platform_impl"));
        assert!(enabled(&debug, Level::Warn, "tao::platform_impl"));

        let normal = logger(LevelFilter::Info);
        assert!(enabled(&normal, Level::Info, "singline"));
        assert!(!enabled(&normal, Level::Debug, "singline::sing"));
        assert!(!enabled(&normal, Level::Debug, "singline_other"));
        assert!(enabled(&normal, Level::Error, "wry"));
    }
}
