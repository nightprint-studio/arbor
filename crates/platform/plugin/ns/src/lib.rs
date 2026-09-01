//! The `arbor.*` namespaces that belong to the **platform**, not to a product.
//!
//! A namespace lands here when two things are true: the state it drives lives in the shell
//! (so a headless backend can only reach it over the reverse channel), and nothing about it
//! is one product's business. Both were true of `arbor.job` while it sat in
//! `corvus-plugin-ns`, and the cost was concrete — a plugin hosted by Bennu could not report
//! progress, for a reason that was purely about where a file sat.
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
//! `arbor.cloud` used to sit beside it as an explicit staging post. It is gone: the cloud is
//! the `cloud-storage` plugin and its wasm providers, and what it needed from Arbor arrived
//! as capabilities that name no bucket (`arbor.ext.call_to_file`, `arbor.oauth`, `arbor.job`).
//! That is the bar for the next domain to land here.

pub mod job;
pub mod prelude;
pub mod proxy;
