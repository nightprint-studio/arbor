//! `ProfileTarget` — how a process is started under, or attached to, a profiler.
//!
//! The seam between *what to measure with* (JFR flags, `NODE_OPTIONS`, a `samply record` wrapper)
//! and *who runs the process*. This crate only says what the process needs; the backend decides
//! where it comes from today (the Run console's spawner) and a future managed-process layer can
//! satisfy the same trait without any parser changing.

use std::path::{Path, PathBuf};

use crate::model::Lang;

/// Where a session writes its raw output.
#[derive(Debug, Clone)]
pub struct SessionPaths {
    /// Scratch directory for this session (deleted once the recording is parsed).
    pub dir: PathBuf,
    /// The raw recording file for the language (`.jfr`, a directory of `.cpuprofile`s, `.json`).
    pub recording: PathBuf,
}

/// What must be added to a launch, or run against a live process, to record.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize)]
pub struct StartPlan {
    /// JVM options placed before `-cp`.
    pub vm_args: Vec<String>,
    /// Environment variables for the child.
    pub env: Vec<(String, String)>,
    /// `argv` prefix to run the whole command under (`samply record …`); the program follows.
    pub wrapper: Vec<String>,
    /// For an attach: `jcmd <pid> <args…>` invocations to start the recording.
    pub attach_start: Vec<Vec<String>>,
    /// For an attach: invocations that stop it and write the file.
    pub attach_stop: Vec<Vec<String>>,
}

pub trait ProfileTarget {
    fn lang(&self) -> Lang;
    /// Human description for the recording's label.
    fn describe(&self) -> String;
    fn plan(&self, paths: &SessionPaths) -> Result<StartPlan, String>;
}

/// A JFR option value, quoted when it needs to be (spaces and commas end an option otherwise).
pub fn jfr_value(v: &str) -> String {
    if v.contains([' ', ',', '"']) { format!("\"{}\"", v.replace('"', "\\\"")) } else { v.to_string() }
}

const JFR_NAME: &str = "arbor-profile";

/// Launch a JVM with Flight Recorder on from the first instruction.
pub struct JfrLaunch {
    pub label: String,
}

impl ProfileTarget for JfrLaunch {
    fn lang(&self) -> Lang {
        Lang::Java
    }
    fn describe(&self) -> String {
        self.label.clone()
    }
    fn plan(&self, p: &SessionPaths) -> Result<StartPlan, String> {
        // `dumponexit` writes the file when the JVM ends — the normal end of a profiled run.
        // `settings=profile` is the ~10 ms sampling configuration shipped with every JDK that has JFR.
        Ok(StartPlan {
            vm_args: vec![format!(
                "-XX:StartFlightRecording=name={JFR_NAME},settings=profile,dumponexit=true,filename={}",
                jfr_value(&p.recording.display().to_string())
            )],
            ..StartPlan::default()
        })
    }
}

/// Attach to a running JVM through `jcmd`.
pub struct JfrAttach {
    pub pid: u32,
    pub label: String,
}

impl ProfileTarget for JfrAttach {
    fn lang(&self) -> Lang {
        Lang::Java
    }
    fn describe(&self) -> String {
        self.label.clone()
    }
    fn plan(&self, p: &SessionPaths) -> Result<StartPlan, String> {
        let pid = self.pid.to_string();
        Ok(StartPlan {
            attach_start: vec![vec![pid.clone(), "JFR.start".into(), format!("name={JFR_NAME}"), "settings=profile".into()]],
            attach_stop: vec![vec![
                pid,
                "JFR.stop".into(),
                format!("name={JFR_NAME}"),
                format!("filename={}", jfr_value(&p.recording.display().to_string())),
            ]],
            ..StartPlan::default()
        })
    }
}

/// Node: `NODE_OPTIONS=--cpu-prof` reaches every Node process the command starts (a dev server's
/// workers included); each writes one `.cpuprofile` into the session directory when it exits.
pub struct NodeCpuProf {
    pub label: String,
}

impl ProfileTarget for NodeCpuProf {
    fn lang(&self) -> Lang {
        Lang::Node
    }
    fn describe(&self) -> String {
        self.label.clone()
    }
    fn plan(&self, p: &SessionPaths) -> Result<StartPlan, String> {
        // `--cpu-prof-dir` takes the path as one argv word inside NODE_OPTIONS, which splits on
        // spaces: a directory with a space must be quoted for it.
        let dir = p.dir.display().to_string();
        let dir = if dir.contains(' ') { format!("\"{dir}\"") } else { dir };
        Ok(StartPlan {
            env: vec![("NODE_OPTIONS".into(), format!("--cpu-prof --cpu-prof-dir={dir}"))],
            ..StartPlan::default()
        })
    }
}

/// Rust (and any native binary): run it under `samply record --save-only`.
pub struct SamplyRecord {
    pub samply: PathBuf,
    pub label: String,
    /// Sampling rate in Hz (samply's default is 1000).
    pub rate_hz: Option<u32>,
}

impl ProfileTarget for SamplyRecord {
    fn lang(&self) -> Lang {
        Lang::Rust
    }
    fn describe(&self) -> String {
        self.label.clone()
    }
    fn plan(&self, p: &SessionPaths) -> Result<StartPlan, String> {
        let mut w = vec![
            self.samply.display().to_string(),
            "record".into(),
            "--save-only".into(),
            "--no-open".into(),
            // `.json.gz` is samply's own default; the importer reads gzip and plain JSON by magic bytes.
            "--output".into(),
            p.recording.display().to_string(),
            // Resolve names while samply still has the binary and its debug info next to it.
            "--presymbolicate".into(),
        ];
        if let Some(r) = self.rate_hz {
            w.push("--rate".into());
            w.push(r.to_string());
        }
        Ok(StartPlan { wrapper: w, ..StartPlan::default() })
    }
}

/// Newest file with the given extension under `dir` (non-recursive) — how a session finds what a
/// tool wrote when the tool picks the file name.
pub fn files_with_ext(dir: &Path, ext: &str) -> Vec<PathBuf> {
    let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut v: Vec<PathBuf> = rd
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case(ext)))
        .collect();
    v.sort();
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(dir: &str) -> SessionPaths {
        SessionPaths { dir: dir.into(), recording: Path::new(dir).join("rec.jfr") }
    }

    #[test]
    fn jfr_launch_quotes_paths_with_spaces() {
        let plan = JfrLaunch { label: "x".into() }.plan(&paths("/tmp/my dir")).unwrap();
        let a = &plan.vm_args[0];
        assert!(a.starts_with("-XX:StartFlightRecording=name=arbor-profile,settings=profile,dumponexit=true,filename=\""));
        assert!(a.ends_with("rec.jfr\""));
        let plain = JfrLaunch { label: "x".into() }.plan(&paths("/tmp/d")).unwrap();
        assert!(!plain.vm_args[0].contains('"'));
    }

    #[test]
    fn attach_builds_jcmd_invocations() {
        let plan = JfrAttach { pid: 4242, label: "x".into() }.plan(&paths("/t")).unwrap();
        assert_eq!(plan.attach_start[0][..2], ["4242".to_string(), "JFR.start".to_string()]);
        assert_eq!(plan.attach_stop[0][1], "JFR.stop");
    }

    #[test]
    fn node_and_samply_plans() {
        let n = NodeCpuProf { label: "n".into() }.plan(&paths("/t")).unwrap();
        assert_eq!(n.env[0].1, "--cpu-prof --cpu-prof-dir=/t");
        let s = SamplyRecord { samply: "/bin/samply".into(), label: "r".into(), rate_hz: Some(500) }.plan(&paths("/t")).unwrap();
        assert!(s.wrapper.contains(&"--save-only".to_string()));
        assert!(s.wrapper.windows(2).any(|w| w == ["--rate", "500"]));
    }
}
