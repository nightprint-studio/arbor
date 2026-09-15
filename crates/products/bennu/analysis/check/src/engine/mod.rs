//! The validation pipeline: the aggregators, the check catalog, parse-error quarantine, the inspection policy and the javac coverage table.

pub mod check;
pub mod check_id;
pub mod incremental;
pub mod inspections;
pub mod javac;
pub mod quarantine;
