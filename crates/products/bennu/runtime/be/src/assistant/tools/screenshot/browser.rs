//! Finding a Chromium-based browser already installed on this machine: Chrome, Edge, Brave or Chromium.
//!
//! Nothing is downloaded and nothing is bundled. The places a browser lands are known per platform —
//! `Program Files` and the per-user folder on Windows (the registry's "App Paths" entry as a fallback, for
//! an install somewhere unusual), `/Applications` on macOS, the usual command names on `PATH` on Linux —
//! so the search is a list of candidates checked for existence. `CHROME_PATH` (the convention Puppeteer
//! and Playwright tooling already honour) wins over all of them.

use std::path::PathBuf;

/// A browser that can take a headless screenshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Browser {
    pub path: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Os {
    Windows,
    MacOs,
    Linux,
}

impl Os {
    pub(super) fn current() -> Os {
        if cfg!(windows) {
            Os::Windows
        } else if cfg!(target_os = "macos") {
            Os::MacOs
        } else {
            Os::Linux
        }
    }
}

/// The installed locations to try, most likely first. Pure: `var` reads an environment variable.
pub(super) fn candidates(os: Os, var: &dyn Fn(&str) -> Option<String>) -> Vec<PathBuf> {
    match os {
        Os::Windows => {
            let roots: Vec<String> = [("ProgramFiles", "C:\\Program Files"), ("ProgramFiles(x86)", "C:\\Program Files (x86)"), ("LOCALAPPDATA", "")]
                .iter()
                .filter_map(|(name, default)| var(name).or_else(|| (!default.is_empty()).then(|| (*default).to_string())))
                .collect();
            let rel = [
                "Google\\Chrome\\Application\\chrome.exe",
                "Microsoft\\Edge\\Application\\msedge.exe",
                "BraveSoftware\\Brave-Browser\\Application\\brave.exe",
                "Chromium\\Application\\chrome.exe",
            ];
            // Browser first, root second: every Chrome before any Edge, whichever folder it is in.
            rel.iter().flat_map(|r| roots.iter().map(move |root| PathBuf::from(format!("{}\\{r}", root.trim_end_matches('\\'))))).collect()
        }
        Os::MacOs => {
            let apps = ["Google Chrome.app/Contents/MacOS/Google Chrome", "Microsoft Edge.app/Contents/MacOS/Microsoft Edge", "Brave Browser.app/Contents/MacOS/Brave Browser", "Chromium.app/Contents/MacOS/Chromium"];
            let mut bases = vec!["/Applications".to_string()];
            bases.extend(var("HOME").map(|h| format!("{}/Applications", h.trim_end_matches('/'))));
            apps.iter().flat_map(|a| bases.iter().map(move |b| PathBuf::from(format!("{b}/{a}")))).collect()
        }
        Os::Linux => ["/usr/bin/google-chrome", "/usr/bin/google-chrome-stable", "/usr/bin/chromium", "/usr/bin/chromium-browser", "/usr/bin/microsoft-edge", "/usr/bin/brave-browser", "/snap/bin/chromium", "/opt/google/chrome/chrome"]
            .iter()
            .map(PathBuf::from)
            .collect(),
    }
}

/// The command names to look for on `PATH` (Linux, and anywhere the browser is installed non-standardly).
pub(super) const PATH_NAMES: [&str; 8] = ["google-chrome", "google-chrome-stable", "chromium", "chromium-browser", "microsoft-edge", "msedge", "brave-browser", "chrome"];

/// The browser to use, or `None`. Checks the disk; never starts anything.
pub(super) fn find() -> Option<Browser> {
    let var = |name: &str| std::env::var(name).ok().filter(|v| !v.trim().is_empty());
    if let Some(p) = var("CHROME_PATH").map(PathBuf::from).filter(|p| p.is_file()) {
        return Some(Browser { path: p });
    }
    if let Some(p) = candidates(Os::current(), &var).into_iter().find(|p| p.is_file()) {
        return Some(Browser { path: p });
    }
    if let Some(p) = PATH_NAMES.iter().find_map(|n| arbor_process_ext::prelude::locate_executable(n, None, &[], &[])) {
        return Some(Browser { path: PathBuf::from(p) });
    }
    registered_path().map(|path| Browser { path })
}

/// The registry's "App Paths" entry for Chrome or Edge: where an installer that ignored `Program Files` put it.
#[cfg(windows)]
fn registered_path() -> Option<PathBuf> {
    use arbor_process_ext::prelude::NoWindowExt;
    ["chrome.exe", "msedge.exe", "brave.exe"].iter().find_map(|exe| {
        ["HKLM", "HKCU"].iter().find_map(|hive| {
            let key = format!("{hive}\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\App Paths\\{exe}");
            let mut cmd = std::process::Command::new("reg");
            cmd.args(["query", &key, "/ve"]);
            cmd.no_window();
            let out = cmd.output().ok().filter(|o| o.status.success())?;
            registered_value(&String::from_utf8_lossy(&out.stdout))
        })
    })
}

#[cfg(not(windows))]
fn registered_path() -> Option<PathBuf> {
    None
}

/// The path in a `reg query … /ve` answer: `    (Default)    REG_SZ    C:\Program Files\…\chrome.exe`.
#[cfg_attr(not(windows), allow(dead_code))]
pub(super) fn registered_value(output: &str) -> Option<PathBuf> {
    output
        .lines()
        .find_map(|l| l.split_once("REG_SZ").map(|(_, v)| v.trim().trim_matches('"').to_string()))
        .map(PathBuf::from)
        .filter(|p| p.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_env(_: &str) -> Option<String> {
        None
    }

    #[test]
    fn windows_tries_chrome_everywhere_before_edge() {
        let list = candidates(Os::Windows, &|n| match n {
            "ProgramFiles" => Some("D:\\Programs".into()),
            "LOCALAPPDATA" => Some("C:\\Users\\me\\AppData\\Local".into()),
            _ => None,
        });
        let first_edge = list.iter().position(|p| p.to_string_lossy().contains("msedge")).unwrap();
        let last_chrome = list.iter().rposition(|p| p.to_string_lossy().contains("Google")).unwrap();
        assert!(last_chrome < first_edge);
        assert!(list.iter().any(|p| p.to_string_lossy() == "D:\\Programs\\Google\\Chrome\\Application\\chrome.exe"));
        assert!(list.iter().any(|p| p.to_string_lossy().starts_with("C:\\Program Files (x86)")), "defaults stand in for a missing variable");
        assert!(list.iter().any(|p| p.to_string_lossy().starts_with("C:\\Users\\me\\AppData\\Local")));
    }

    #[test]
    fn macos_looks_in_both_applications_folders() {
        let list = candidates(Os::MacOs, &|n| (n == "HOME").then(|| "/Users/me/".to_string()));
        assert_eq!(list[0], PathBuf::from("/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"));
        assert!(list.contains(&PathBuf::from("/Users/me/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge")));
        assert!(candidates(Os::MacOs, &no_env).iter().all(|p| p.starts_with("/Applications")));
    }

    #[test]
    fn linux_has_the_usual_packages_and_the_snap() {
        let list = candidates(Os::Linux, &no_env);
        assert!(list.contains(&PathBuf::from("/usr/bin/chromium")) && list.contains(&PathBuf::from("/snap/bin/chromium")));
        assert!(PATH_NAMES.contains(&"google-chrome"));
    }

    #[test]
    fn the_registry_answer_is_read_for_its_value() {
        let missing = "\r\nHKEY_LOCAL_MACHINE\\SOFTWARE\\x\r\n    (Default)    REG_SZ    C:\\definitely\\not\\here\\chrome.exe\r\n";
        assert_eq!(registered_value(missing), None, "a path that does not exist is not a browser");
        assert_eq!(registered_value("nothing useful"), None);
    }
}
