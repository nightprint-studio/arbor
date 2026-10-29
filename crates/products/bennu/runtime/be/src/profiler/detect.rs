//! What this machine can profile with: the project's JDK for JFR, `node`, `samply`.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use bennu_profile::prelude::*;
use serde::Serialize;

fn exe_name(base: &str) -> String {
    if cfg!(windows) { format!("{base}.exe") } else { base.to_string() }
}

/// `<home>/bin/<tool>` when it is a file.
fn tool(home: &Path, name: &str) -> Option<PathBuf> {
    let p = home.join("bin").join(exe_name(name));
    p.is_file().then_some(p)
}

/// The `jfr` tool of any installed JDK 12+ — it reads recordings made by a JDK 11 or an 8u262+.
fn other_jdks_jfr() -> Option<PathBuf> {
    ["25", "23", "21", "17", "22", "20", "19", "18", "16", "15", "14", "13", "12"]
        .iter()
        .filter_map(|v| bennu_classpath::prelude::find_jdk_home(v))
        .find_map(|h| tool(&h, "jfr"))
}

/// Where the project's JDK is and what it can do for JFR.
pub struct JdkProbe {
    pub home: Option<PathBuf>,
    pub jcmd: Option<PathBuf>,
    pub availability: JfrAvailability,
}

pub fn probe_jdk(root: &str) -> JdkProbe {
    let home = crate::build::resolve_java_home(root).or_else(|| {
        std::env::var_os("JAVA_HOME").map(PathBuf::from).filter(|p| tool(p, "java").is_some())
    });
    let Some(home) = home else {
        return JdkProbe {
            home: None,
            jcmd: None,
            availability: JfrAvailability {
                available: false,
                how_to_parse: None,
                reason: "No JDK was found for this project.".into(),
                remedy: Some("Install a JDK (11 or newer recommended) or set JAVA_HOME, then reopen the project.".into()),
                jdk: String::new(),
            },
        };
    };
    let java = tool(&home, "java");
    let jcmd = tool(&home, "jcmd");
    let Some(facts) = probe(&home) else {
        return JdkProbe {
            home: Some(home.clone()),
            jcmd,
            availability: JfrAvailability {
                available: false,
                how_to_parse: None,
                reason: format!("{} does not look like a JDK (no bin/java).", home.display()),
                remedy: Some("Point the project at a full JDK.".into()),
                jdk: String::new(),
            },
        };
    };
    let javac = tool(&home, "javac");
    let own_jfr = tool(&home, "jfr");
    let alt = if own_jfr.is_none() { other_jdks_jfr() } else { None };
    let availability = classify(
        &facts,
        java.as_deref().unwrap_or(Path::new("java")),
        javac.as_deref(),
        own_jfr.as_deref(),
        alt.as_deref(),
    );
    JdkProbe { home: Some(home), jcmd, availability }
}

/// A local JVM `jcmd -l` can see.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct LocalJvm {
    pub pid: u32,
    pub name: String,
}

/// Parse `jcmd -l` output, dropping jcmd itself.
pub fn parse_jcmd_list(text: &str) -> Vec<LocalJvm> {
    text.lines()
        .filter_map(|l| {
            let l = l.trim();
            let (pid, name) = l.split_once(' ').unwrap_or((l, ""));
            let pid: u32 = pid.parse().ok()?;
            let name = name.trim();
            let lower = name.to_ascii_lowercase();
            if lower.contains("sun.tools.jcmd") || lower.starts_with("jdk.jcmd") || lower.contains("jcmd -l") {
                return None;
            }
            Some(LocalJvm { pid, name: if name.is_empty() { "(unnamed JVM)".into() } else { name.to_string() } })
        })
        .collect()
}

pub fn run_jcmd(jcmd: &Path, args: &[String]) -> Result<String, String> {
    let mut c = Command::new(jcmd);
    c.args(args).stdin(Stdio::null());
    #[cfg(windows)]
    {
        use arbor_process_ext::prelude::NoWindowExt;
        c.no_window();
    }
    let out = c.output().map_err(|e| format!("cannot run jcmd: {e}"))?;
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    if out.status.success() { Ok(text) } else { Err(jcmd_failure(&text)) }
}

/// Plain-language reading of the ways `jcmd JFR.*` fails on an attach.
pub fn jcmd_failure(text: &str) -> String {
    let t = text.trim();
    if t.contains("Unknown diagnostic command") || t.contains("not a valid") {
        "The target JVM does not support Flight Recorder (OpenJ9, a JDK before 8u262, or a runtime without the jdk.jfr module).".into()
    } else if t.contains("Unable to open socket") || t.contains("AttachNotSupported") || t.contains("Permission denied") {
        "Cannot attach to that JVM: it runs as another user, in a container, or with attach disabled (-XX:+DisableAttachMechanism).".into()
    } else if t.contains("Commercial Features") || t.contains("UnlockCommercialFeatures") {
        "This Oracle JDK 8 build needs -XX:+UnlockCommercialFeatures (a commercial license) to record. Use JDK 8u262+ or 11+.".into()
    } else if t.is_empty() {
        "jcmd failed without a message.".into()
    } else {
        format!("jcmd failed: {t}")
    }
}

pub fn list_jvms(root: &str) -> Result<Vec<LocalJvm>, String> {
    let probe = probe_jdk(root);
    let jcmd = probe
        .jcmd
        .or_else(|| arbor_process_ext::prelude::locate_executable("jcmd", None, &[], &[]).map(PathBuf::from))
        .ok_or("jcmd was not found. It ships with a full JDK (not a JRE).")?;
    Ok(parse_jcmd_list(&run_jcmd(&jcmd, &["-l".into()])?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jcmd_list_drops_itself_and_unparsable_lines() {
        let out = "12345 com.acme.App --port 8080\n999 jdk.jcmd/sun.tools.jcmd.JCmd -l\n4242 \nnoise\n77 /opt/app/server.jar\n";
        let v = parse_jcmd_list(out);
        assert_eq!(v.iter().map(|j| j.pid).collect::<Vec<_>>(), vec![12345, 4242, 77]);
        assert_eq!(v[1].name, "(unnamed JVM)");
    }

    #[test]
    fn attach_failures_have_plain_messages() {
        assert!(jcmd_failure("Unknown diagnostic command").contains("OpenJ9"));
        assert!(jcmd_failure("java.io.IOException: Permission denied").contains("another user"));
    }
}
