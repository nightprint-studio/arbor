//! The no-dependency tier: a screenshot straight from the Chromium-based browser's own command line.
//!
//! `chrome --headless --screenshot=<file> --window-size=w,h <url>` loads the page, lets it run for a virtual
//! time budget, and writes a PNG. That is all it can do — no selector, no full page, no console, no network
//! log — which is why this is the fallback and the tool says what was left out.
//!
//! The browser gets a **throw-away profile directory**. Without one, a headless launch against the user's
//! default profile hands the request to their running browser and exits, so no screenshot would ever appear
//! (and the profile would be touched).

use std::path::Path;

/// How the capture is asked for.
pub(super) struct Shot<'a> {
    pub url: &'a str,
    pub out: &'a Path,
    pub profile: &'a Path,
    pub width: u32,
    pub height: u32,
    pub dark: bool,
    /// Let the page run this long (virtual time) before the picture is taken.
    pub settle_ms: u64,
}

/// The default settle time: enough for a SPA to fetch its first data and render, short enough to be quick.
pub(super) const DEFAULT_SETTLE_MS: u64 = 3_000;
const MAX_SETTLE_MS: u64 = 10_000;

/// The settle time for a `wait_for` that is a number of milliseconds; a selector cannot be waited on here.
pub(super) fn settle_ms(wait_for: Option<&str>) -> u64 {
    wait_for.and_then(|w| w.trim().parse::<u64>().ok()).map(|ms| ms.clamp(0, MAX_SETTLE_MS)).unwrap_or(DEFAULT_SETTLE_MS)
}

/// The command line, one argument per element.
pub(super) fn args(shot: &Shot) -> Vec<String> {
    let mut a = vec![
        "--headless=new".to_string(),
        "--disable-gpu".into(),
        "--hide-scrollbars".into(),
        "--no-first-run".into(),
        "--no-default-browser-check".into(),
        "--disable-extensions".into(),
        "--disable-background-networking".into(),
        "--force-device-scale-factor=1".into(),
        format!("--user-data-dir={}", shot.profile.display()),
        format!("--window-size={},{}", shot.width, shot.height),
        format!("--virtual-time-budget={}", shot.settle_ms),
        format!("--screenshot={}", shot.out.display()),
    ];
    if shot.dark {
        a.push("--blink-settings=preferredColorScheme=1".into());
    }
    // Last, and after nothing that could be read as a second URL.
    a.push(shot.url.to_string());
    a
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn shot<'a>(out: &'a PathBuf, profile: &'a PathBuf, dark: bool) -> Shot<'a> {
        Shot { url: "http://localhost:3000/", out, profile, width: 1280, height: 800, dark, settle_ms: 3_000 }
    }

    #[test]
    fn the_command_line_asks_for_a_headless_shot_of_the_window_size_with_its_own_profile() {
        let (out, profile) = (PathBuf::from("/tmp/o.png"), PathBuf::from("/tmp/p"));
        let a = args(&shot(&out, &profile, false));
        assert!(a.contains(&"--headless=new".to_string()));
        assert!(a.contains(&"--window-size=1280,800".to_string()));
        assert!(a.iter().any(|x| x.starts_with("--screenshot=") && x.ends_with("o.png")));
        assert!(a.iter().any(|x| x.starts_with("--user-data-dir=")), "never the user's own profile");
        assert_eq!(a.last().unwrap(), "http://localhost:3000/");
        assert!(!a.iter().any(|x| x.contains("preferredColorScheme")));
    }

    #[test]
    fn dark_mode_is_a_blink_setting() {
        let (out, profile) = (PathBuf::from("o.png"), PathBuf::from("p"));
        assert!(args(&shot(&out, &profile, true)).contains(&"--blink-settings=preferredColorScheme=1".to_string()));
    }

    #[test]
    fn only_a_number_of_milliseconds_can_be_waited_for_here() {
        assert_eq!(settle_ms(None), DEFAULT_SETTLE_MS);
        assert_eq!(settle_ms(Some("#app.ready")), DEFAULT_SETTLE_MS);
        assert_eq!(settle_ms(Some("1500")), 1500);
        assert_eq!(settle_ms(Some("999999")), MAX_SETTLE_MS);
    }
}
