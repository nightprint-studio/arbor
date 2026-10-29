//! What a backend leaves behind when it dies.
//!
//! A backend is a child process whose stdout is the protocol channel and whose stderr nobody read:
//! launched from the desktop it went nowhere, so a panic, a failed allocation or a stack overflow ended
//! in the shell saying "backend for 'bennu' is not running" and nothing else. Now the shell points each
//! backend's stderr at `arbor/logs/<program>-be.log` ([`open_backend_log`]), the backend writes its panics
//! there with a timestamp, the thread and a backtrace ([`install_panic_hook`]), and the shell adds a line
//! when it sees the channel close ([`note`]). A kill from outside (the OS out of memory) leaves no message
//! of its own, but the disconnect line still says *when*.
//!
//! One place for the three, so the logs of every backend look alike.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// Past this size the log is moved to `<name>.1` (replacing the previous one) when a backend starts.
const MAX_LOG_BYTES: u64 = 2 * 1024 * 1024;

/// `arbor/logs` — next to the profiles, outside any project.
pub fn arbor_logs_dir() -> PathBuf {
    crate::paths::arbor_config_dir().join("logs")
}

/// Where a backend's log is: `arbor/logs/<program>-be.log`.
pub fn backend_log_path(program: &str) -> PathBuf {
    arbor_logs_dir().join(format!("{program}-be.log"))
}

/// Open (appending) the log of `program`'s backend, to hand to the child as its stderr. A header line says
/// when this run began. `None` when the folder or the file cannot be made: the backend then runs as it did
/// before, and a log that cannot be written must not stop it from starting.
pub fn open_backend_log(program: &str) -> Option<File> {
    let path = backend_log_path(program);
    std::fs::create_dir_all(path.parent()?).ok()?;
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > MAX_LOG_BYTES) {
        let _ = std::fs::rename(&path, path.with_extension("log.1"));
    }
    let mut file = OpenOptions::new().create(true).append(true).open(&path).ok()?;
    let _ = writeln!(file, "--- {program}-be started {} ---", utc_stamp());
    Some(file)
}

/// Append one timestamped line to `program`'s backend log (the shell, when it sees the backend go).
pub fn note(program: &str, message: &str) {
    let Some(dir) = backend_log_path(program).parent().map(PathBuf::from) else { return };
    let _ = std::fs::create_dir_all(dir);
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(backend_log_path(program)) {
        let _ = writeln!(file, "[{}] {message}", utc_stamp());
    }
}

/// Make a panic say what happened: when, on which thread, where, what, and the stack — to **stderr**, which
/// for a backend is its log file (stdout is the protocol channel and must stay clean). The default hook
/// prints a backtrace only when `RUST_BACKTRACE` is set, which nobody sets on a desktop launch.
pub fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        let thread = std::thread::current();
        let place = info.location().map(|l| format!("{}:{}", l.file(), l.line())).unwrap_or_else(|| "?".to_string());
        let what = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| (*s).to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "(not a string)".to_string());
        let stack = std::backtrace::Backtrace::force_capture();
        eprintln!("[{}] panic on thread '{}' at {place}: {what}\n{stack}", utc_stamp(), thread.name().unwrap_or("unnamed"));
    }));
}

/// `2026-10-29 16:43:49Z`. No date library for one line of a log.
pub fn utc_stamp() -> String {
    let seconds = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    stamp_of(seconds)
}

fn stamp_of(seconds: i64) -> String {
    let (year, month, day) = civil_from_days(seconds.div_euclid(86_400));
    let in_day = seconds.rem_euclid(86_400);
    format!("{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02}Z", in_day / 3600, in_day % 3600 / 60, in_day % 60)
}

/// Days since 1970-01-01 to a proleptic Gregorian (year, month, day).
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z.rem_euclid(146_097);
    let year_of_era = (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 { shifted_month + 3 } else { shifted_month - 9 };
    (year_of_era + era * 400 + i64::from(month <= 2), month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_unix_time_reads_as_a_utc_date() {
        assert_eq!(stamp_of(0), "1970-01-01 00:00:00Z");
        assert_eq!(stamp_of(1_700_000_000), "2023-11-14 22:13:20Z");
        assert_eq!(stamp_of(951_782_400), "2000-02-29 00:00:00Z", "a leap day");
    }

    #[test]
    fn a_log_is_named_after_its_program() {
        let path = backend_log_path("bennu");
        assert!(path.ends_with("logs/bennu-be.log") || path.ends_with("logs\\bennu-be.log"), "{path:?}");
    }
}
