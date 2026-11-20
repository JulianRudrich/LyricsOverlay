//! The system Singline runs on, as the log and problem reports name it.

/// The system as the bug report form lists it: "macOS", "Windows" or "Linux".
pub fn name() -> &'static str {
    if cfg!(target_os = "macos") {
        "macOS"
    } else if cfg!(windows) {
        "Windows"
    } else {
        "Linux"
    }
}

/// The system's version and the processor architecture, such as
/// "macOS 15.2, aarch64", "Windows 11 (build 26100.2605), x86_64" or
/// "Ubuntu 24.04.1 LTS (Wayland), x86_64".
pub fn version() -> String {
    format!("{}, {}", release(), std::env::consts::ARCH)
}

#[cfg(target_os = "macos")]
fn release() -> String {
    let printed = std::process::Command::new("sw_vers")
        .arg("-productVersion")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).into_owned())
        .unwrap_or_default();
    macos_release(&printed)
}

#[cfg(windows)]
fn release() -> String {
    let version = windows_version::OsVersion::current();
    windows_release(
        version.major,
        version.minor,
        version.build,
        windows_version::revision(),
    )
}

#[cfg(not(any(target_os = "macos", windows)))]
fn release() -> String {
    let os_release = std::fs::read_to_string("/etc/os-release")
        .or_else(|_| std::fs::read_to_string("/usr/lib/os-release"))
        .unwrap_or_default();
    let session = std::env::var("XDG_SESSION_TYPE").unwrap_or_default();
    linux_release(pretty_name(&os_release).as_deref(), &session)
}

/// "macOS 15.2", from what `sw_vers -productVersion` printed.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn macos_release(product_version: &str) -> String {
    match product_version.trim() {
        "" => "macOS".to_owned(),
        number => format!("macOS {number}"),
    }
}

/// "Windows 11 (build 26100.2605)". Windows 11 still reports itself as 10.0;
/// its builds start at 22000.
#[cfg_attr(not(windows), allow(dead_code))]
fn windows_release(major: u32, minor: u32, build: u32, revision: u32) -> String {
    let name = match (major, minor) {
        (10, 0) if build >= 22_000 => "11".to_owned(),
        (10, 0) => "10".to_owned(),
        _ => format!("{major}.{minor}"),
    };
    match revision {
        0 => format!("Windows {name} (build {build})"),
        _ => format!("Windows {name} (build {build}.{revision})"),
    }
}

/// The distribution and the kind of desktop session, such as
/// "Ubuntu 24.04.1 LTS (Wayland)", as XWayland and Wayland behave differently.
#[cfg_attr(any(target_os = "macos", windows), allow(dead_code))]
fn linux_release(pretty_name: Option<&str>, session: &str) -> String {
    let name = pretty_name.unwrap_or("Linux");
    match session.trim() {
        "" => name.to_owned(),
        "x11" => format!("{name} (X11)"),
        "wayland" => format!("{name} (Wayland)"),
        other => format!("{name} ({other})"),
    }
}

/// The PRETTY_NAME of an os-release file, such as "Ubuntu 24.04.1 LTS".
#[cfg_attr(any(target_os = "macos", windows), allow(dead_code))]
fn pretty_name(os_release: &str) -> Option<String> {
    os_release.lines().find_map(|line| {
        let value = unquoted(line.trim().strip_prefix("PRETTY_NAME=")?.trim());
        (!value.is_empty()).then_some(value)
    })
}

/// An os-release value without its quotes and backslash escapes.
#[cfg_attr(any(target_os = "macos", windows), allow(dead_code))]
fn unquoted(value: &str) -> String {
    let inner = ['"', '\'']
        .into_iter()
        .find_map(|quote| value.strip_prefix(quote)?.strip_suffix(quote))
        .unwrap_or(value);
    let mut text = String::with_capacity(inner.len());
    let mut characters = inner.chars();
    while let Some(character) = characters.next() {
        text.push(match character {
            '\\' => characters.next().unwrap_or(character),
            _ => character,
        });
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_pretty_name_of_os_release() {
        let ubuntu = "NAME=\"Ubuntu\"\nVERSION_ID=\"24.04\"\nPRETTY_NAME=\"Ubuntu 24.04.1 LTS\"\nID=ubuntu\n";
        assert_eq!(pretty_name(ubuntu).as_deref(), Some("Ubuntu 24.04.1 LTS"));
        assert_eq!(
            pretty_name("# PRETTY_NAME=\"Not this\"\nPRETTY_NAME='Arch Linux'\n").as_deref(),
            Some("Arch Linux"),
            "comments are skipped and single quotes work"
        );
        assert_eq!(
            pretty_name("  PRETTY_NAME=Debian\n").as_deref(),
            Some("Debian")
        );
        assert_eq!(
            pretty_name(r#"PRETTY_NAME="Fedora \"Rawhide\" \\ 42""#).as_deref(),
            Some(r#"Fedora "Rawhide" \ 42"#)
        );
        assert_eq!(pretty_name("NAME=\"Gentoo\"\nPRETTY_NAME=\"\"\n"), None);
        assert_eq!(pretty_name(""), None);
    }

    #[test]
    fn linux_names_the_desktop_session() {
        assert_eq!(
            linux_release(Some("Ubuntu 24.04.1 LTS"), "wayland"),
            "Ubuntu 24.04.1 LTS (Wayland)"
        );
        assert_eq!(linux_release(Some("Debian"), "x11"), "Debian (X11)");
        assert_eq!(linux_release(None, "tty"), "Linux (tty)");
        assert_eq!(linux_release(None, " "), "Linux");
    }

    #[test]
    fn windows_11_is_told_apart_by_its_build() {
        assert_eq!(
            windows_release(10, 0, 26_100, 2_605),
            "Windows 11 (build 26100.2605)"
        );
        assert_eq!(
            windows_release(10, 0, 19_045, 0),
            "Windows 10 (build 19045)"
        );
        assert_eq!(windows_release(6, 3, 9_600, 0), "Windows 6.3 (build 9600)");
    }

    #[test]
    fn macos_names_its_version() {
        assert_eq!(macos_release("15.2\n"), "macOS 15.2");
        assert_eq!(macos_release(""), "macOS");
    }

    #[test]
    fn the_version_ends_with_the_architecture() {
        let version = version();
        assert!(
            version.ends_with(&format!(", {}", std::env::consts::ARCH)),
            "{version}"
        );
    }
}
