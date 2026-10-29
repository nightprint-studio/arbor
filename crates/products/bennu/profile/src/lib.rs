//! Bennu profiler core. Everything public is re-exported through [`prelude`].

pub mod prelude;

mod model;
mod builder;
mod jfr_detect;
mod jfr_events;
mod jfr_helper;
mod cpuprofile;
mod firefox;
mod compare;
mod metrics;
mod target;
mod duration;
mod gzip;
