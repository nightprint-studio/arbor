//! The `arbor.*` namespaces that belong to the **platform**, not to a product.
//!
//! A namespace lands here when two things are true: the state it drives lives in the shell
//! (so a headless backend can only reach it over the reverse channel), and nothing about it
//! is one product's business. Both were true of `arbor.job` and `arbor.cloud` while they sat
//! in `corvus-plugin-ns`, and the cost was concrete — a plugin hosted by Bennu could not
//! report progress or browse a bucket, for a reason that was purely about where a file sat.
//!
//! The shared half is [`proxy::HostProxy`]: one round-trip helper, so a domain module only
//! adds its vocabulary. Each domain is a pair — `host.rs` (the `__<domain>_*` forwards) and
//! `ns.rs` (the Lua surface built on them).
//!
//! ```ignore
//! use arbor_plugin_ns::prelude::{JobHostOps, JobInstaller};
//!
//! let jobs = JobInstaller::new(JobHostOps::new(app.host_caller()));
//! app.api_installer(my_api_installer(vec![Arc::new(jobs)]));
//! ```
//!
//! Note on [`cloud`]: it is here as a **staging post**, not as a home. The cloud is being
//! moved out of Arbor and into the `cloud-storage` plugin and its WASI providers; when that
//! lands the module leaves with it, and this crate is `arbor.job` plus whatever else has
//! earned the same argument.

pub mod cloud;
pub mod job;
pub mod prelude;
pub mod proxy;
