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

/// What to ask of Flight Recorder beyond the `profile` settings.
#[derive(Debug, Clone, Copy, Default)]
pub struct JfrTuning {
    /// Interval of the execution sampler in milliseconds; `None` keeps the settings' own (10 ms). A shorter one
    /// catches shorter methods and costs more overhead — and below the platform's timer granularity it is rounded.
    pub sampling_ms: Option<u32>,
    /// The JDK takes event settings as options (`jdk.ExecutionSample#period=2ms`): 17 and newer. On an older one
    /// `sampling_ms` is ignored rather than passed to a JVM that would refuse to start.
    pub event_settings: bool,
}

impl JfrTuning {
    /// The `name#setting=value` words for the chosen interval: the sampler's period, and the same figure as the
    /// threshold below which a socket or file wait is not reported — so a 2 ms interval also sees a 2 ms query.
    pub fn event_options(&self) -> Vec<String> {
        let Some(ms) = self.sampling_ms.filter(|_| self.event_settings) else { return Vec::new() };
        let ms = ms.max(1);
        let mut v = vec![format!("jdk.ExecutionSample#period={ms}ms")];
        for event in ["jdk.SocketRead", "jdk.SocketWrite", "jdk.FileRead", "jdk.FileWrite"] {
            v.push(format!("{event}#threshold={ms}ms"));
        }
        v
    }
}

/// Stack frames kept per event. JFR's default is 64, which a Spring request outgrows: the project's own frame
/// sits beyond the cut and an allocation or a wait can no longer be traced to it.
const STACK_DEPTH: u32 = 256;

/// Launch a JVM with Flight Recorder on from the first instruction.
pub struct JfrLaunch {
    pub label: String,
    pub tuning: JfrTuning,
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
        let mut start = format!(
            "-XX:StartFlightRecording=name={JFR_NAME},settings=profile,dumponexit=true,filename={}",
            jfr_value(&p.recording.display().to_string())
        );
        for option in self.tuning.event_options() {
            start.push(',');
            start.push_str(&option);
        }
        Ok(StartPlan {
            // The depth is a recorder option, not a recording one: it can only be set when the JVM starts.
            vm_args: vec![format!("-XX:FlightRecorderOptions=stackdepth={STACK_DEPTH}"), start],
            ..StartPlan::default()
        })
    }
}

/// Attach to a running JVM through `jcmd`.
pub struct JfrAttach {
    pub pid: u32,
    pub label: String,
    pub tuning: JfrTuning,
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
            attach_start: vec![{
                let mut step = vec![pid.clone(), "JFR.start".into(), format!("name={JFR_NAME}"), "settings=profile".into()];
                step.extend(self.tuning.event_options());
                step
            }],
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

    fn launch(dir: &str, tuning: JfrTuning) -> StartPlan {
        JfrLaunch { label: "x".into(), tuning }.plan(&paths(dir)).unwrap()
    }

    /// The `-XX:StartFlightRecording=…` argument, wherever the plan puts it.
    fn start_arg(plan: &StartPlan) -> &String {
        plan.vm_args.iter().find(|a| a.starts_with("-XX:StartFlightRecording=")).unwrap()
    }

    #[test]
    fn jfr_launch_quotes_paths_with_spaces() {
        let plan = launch("/tmp/my dir", JfrTuning::default());
        let a = start_arg(&plan);
        assert!(a.starts_with("-XX:StartFlightRecording=name=arbor-profile,settings=profile,dumponexit=true,filename=\""));
        assert!(a.ends_with("rec.jfr\""));
        assert!(!start_arg(&launch("/tmp/d", JfrTuning::default())).contains('"'));
    }

    #[test]
    fn a_launch_asks_for_deep_stacks_before_the_recording_starts() {
        let plan = launch("/t", JfrTuning::default());
        assert_eq!(plan.vm_args[0], "-XX:FlightRecorderOptions=stackdepth=256");
    }

    #[test]
    fn a_sampling_interval_becomes_event_settings_only_where_the_jdk_takes_them() {
        let new = launch("/t", JfrTuning { sampling_ms: Some(2), event_settings: true });
        let a = start_arg(&new);
        assert!(a.contains(",jdk.ExecutionSample#period=2ms"), "{a}");
        assert!(a.contains(",jdk.SocketRead#threshold=2ms"), "{a}");
        // An older JDK would refuse to start with an option it does not know: the interval is dropped instead.
        let old = launch("/t", JfrTuning { sampling_ms: Some(2), event_settings: false });
        assert!(!start_arg(&old).contains("jdk.ExecutionSample"));
        // And no interval asked for means the settings' own.
        assert!(!start_arg(&launch("/t", JfrTuning { sampling_ms: None, event_settings: true })).contains("#period"));
    }

    #[test]
    fn attach_builds_jcmd_invocations() {
        let plan = JfrAttach { pid: 4242, label: "x".into(), tuning: JfrTuning { sampling_ms: Some(5), event_settings: true } }
            .plan(&paths("/t"))
            .unwrap();
        assert_eq!(plan.attach_start[0][..2], ["4242".to_string(), "JFR.start".to_string()]);
        assert!(plan.attach_start[0].iter().any(|w| w == "jdk.ExecutionSample#period=5ms"));
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
