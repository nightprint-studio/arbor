//! Canonical entry point for `bennu-profile`'s public API.

pub use crate::builder::{FlameNode, FlameOptions, RawFrame, TreeBuilder, DEFAULT_MAX_DEPTH, DEFAULT_MAX_NODES};
pub use crate::compare::{compare, MethodDelta};
pub use crate::cpuprofile::{add_cpuprofile, url_to_path, CpuProfileResult};
pub use crate::duration::duration_ms;
pub use crate::firefox::{add_firefox_profile, FirefoxResult};
pub use crate::jfr_detect::{
    classify, parse_java_version, parse_release, probe, version_from_banner, JdkFacts, JfrAvailability, ParseHow,
    ReleaseInfo,
};
pub use crate::jfr_events::{
    parse_helper_line, parse_print_event, EventSplitter, JfrAggregator, JfrEvent, JfrFrame, WANTED_EVENTS,
};
pub use crate::gzip::{read_maybe_gzip, MAX_DECODED_BYTES};
pub use crate::jfr_helper::{HELPER_CLASS, HELPER_FILE, HELPER_SOURCE};
pub use crate::metrics::{
    dotted_class, java_file_metrics, path_file_metrics, DeclaredMethod, FileMetrics, LineMetric, MethodMetric,
};
pub use crate::model::*;
pub use crate::target::{
    files_with_ext, jfr_value, JfrAttach, JfrLaunch, NodeCpuProf, ProfileTarget, SamplyRecord, SessionPaths, StartPlan,
};
