//! Panic hook: with no console in release builds a panic would close the
//! window silently, so save a crash log and offer to report it on GitHub.

const ISSUES: &str = "https://github.com/andy778/ftar-core/issues/new";
/// Longest panic text put in the URL (GitHub rejects very long URLs).
const MAX_BODY: usize = 1500;

/// Percent-encode everything except unreserved characters.
fn encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// A pre-filled "new bug report" link for a crash. Nothing is sent: the user
/// reviews the issue in the browser and submits it themselves.
pub(crate) fn issue_url(version: &str, os: &str, panic: &str, log: Option<&str>) -> String {
    let mut text: String = panic.chars().take(MAX_BODY).collect();
    if panic.chars().count() > MAX_BODY {
        text.push_str("\n[truncated]");
    }
    let mut what = format!("ftar crashed with this error:\n\n```\n{text}\n```\n");
    if let Some(log) = log {
        what.push_str(&format!(
            "\nThe full log (with backtrace) is at `{log}`; please attach it.\n"
        ));
    }
    let first = panic.lines().next().unwrap_or("panic");
    let title: String = format!("Crash: {first}").chars().take(100).collect();
    format!(
        "{ISSUES}?template=bug_report.yml&labels=bug&title={}&what={}&version={}&os={}&part=GUI",
        encode(&title),
        encode(&what),
        encode(version),
        encode(os)
    )
}

fn open_url(url: &str) {
    use std::process::Command;
    #[cfg(target_os = "windows")]
    let mut cmd = {
        // `start` would treat `&` in the URL as a command separator.
        let mut c = Command::new("rundll32");
        c.args(["url.dll,FileProtocolHandler", url]);
        c
    };
    #[cfg(target_os = "macos")]
    let mut cmd = {
        let mut c = Command::new("open");
        c.arg(url);
        c
    };
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let mut cmd = {
        let mut c = Command::new("xdg-open");
        c.arg(url);
        c
    };
    let _ = cmd.spawn();
}

pub(crate) fn install_panic_hook() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        default(info);
        let log = std::env::temp_dir().join("ftar-crash.log");
        let text = format!(
            "ftar {} crashed:\n{info}\n\n{}\n",
            env!("CARGO_PKG_VERSION"),
            std::backtrace::Backtrace::force_capture()
        );
        let saved = std::fs::write(&log, &text).is_ok();
        let note = if saved {
            format!("Details were saved to {}.", log.display())
        } else {
            "The crash details could not be saved.".to_string()
        };
        let report = rfd::MessageDialog::new()
            .set_level(rfd::MessageLevel::Error)
            .set_title("ftar crashed")
            .set_description(format!(
                "ftar hit an unexpected error and must close.\n\n{info}\n\n{note}\n\n\
                 Open a pre-filled bug report on GitHub? You can review it before sending."
            ))
            .set_buttons(rfd::MessageButtons::YesNo)
            .show();
        if report == rfd::MessageDialogResult::Yes {
            let log = saved.then(|| log.display().to_string());
            open_url(&issue_url(
                env!("CARGO_PKG_VERSION"),
                std::env::consts::OS,
                &info.to_string(),
                log.as_deref(),
            ));
        }
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_is_encoded_and_prefilled() {
        let u = issue_url(
            "0.3.0",
            "windows",
            "panicked at a&b.rs:1\nboom",
            Some("/tmp/x.log"),
        );
        assert!(u.starts_with(ISSUES));
        assert!(u.contains("version=0.3.0&os=windows&part=GUI"));
        assert!(u.contains("title=Crash%3A%20panicked%20at%20a%26b.rs%3A1"));
        assert!(!u.contains(' ') && !u.contains('\n'));
    }

    #[test]
    fn long_panic_is_truncated() {
        let u = issue_url("v", "o", &"x".repeat(10_000), None);
        assert!(u.len() < 3_000, "{}", u.len());
        assert!(u.contains("truncated"));
    }
}
