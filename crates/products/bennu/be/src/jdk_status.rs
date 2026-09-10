//! `jdk_status` domain — `bennu_jdk_status`.
//!
//! Reports how the project's JDK resolved (exact match / fallback / none) so the FE can
//! warn: a titlebar badge when NO JDK is installed at all (completion + navigation can't
//! resolve the standard library), and a Problems entry when a fallback JDK was used (the
//! exact level the project targets isn't installed). Read-only, off the project slot.

use bennu_core::prelude::BennuState;
use bennu_proto::prelude::JdkStatus;
use serde::{Deserialize, Serialize};

use crate::index_service::IndexService;

/// Args for [`bennu_jdk_status`].
#[derive(Deserialize)]
pub struct JdkStatusArgs {
    /// Absolute path to the open project's root.
    pub root: String,
}

/// Return the JDK resolution status for the project at `root`, or `None` when no project
/// owns `root`. Never errors.
#[arbor_rpc::handler]
fn bennu_jdk_status(_ctx: &BennuState, args: JdkStatusArgs) -> Result<Option<JdkStatus>, String> {
    Ok(IndexService::global().jdk_report(&args.root))
}

/// Args for [`bennu_module_jdk`].
#[derive(Deserialize)]
pub struct ModuleJdkArgs {
    /// Absolute path to the file whose module is being asked about.
    pub file: String,
}

/// The level of the module a file belongs to, on the wire.
#[derive(Debug, Clone, Serialize)]
pub struct ModuleJdkWire {
    /// The module's directory relative to the project root (`"."` for the root itself).
    pub module: String,
    /// The level as declared (`"1.8"`, `"21"`).
    pub version: String,
    /// Which key declared it — `maven.compiler.release`, `java.version`, …
    pub source: String,
    /// The level as a number, for the editor decisions that gate on it.
    pub major: Option<u32>,
}

/// The Java level of the MODULE the open file belongs to, or `None` when no pom above it declares
/// one (and the project's own answer stands).
///
/// A reactor part-way through a migration has one module on 21 and another still on 8, and that is
/// an ordinary state rather than a misconfiguration. It is also the only part of "which Java is
/// this" that can be answered per file honestly: the index and the classpath are one JDK by
/// construction, but whether a syntax exists yet is a question about the file in front of you.
#[arbor_rpc::handler]
fn bennu_module_jdk(_ctx: &BennuState, args: ModuleJdkArgs) -> Result<Option<ModuleJdkWire>, String> {
    Ok(IndexService::global().module_jdk_of(&args.file).map(|m| ModuleJdkWire {
        module: m.module,
        version: m.version,
        source: m.source,
        major: m.major,
    }))
}
