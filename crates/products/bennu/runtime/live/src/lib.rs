//! Live runs: start the project's services, drive them over loopback HTTP, watch everything on one
//! timeline, and check it with scenarios — for the assistant and for the user, the same machinery.
//!
//! The crate is std-only on purpose (threads, `TcpStream`): a managed process is a child plus two
//! reader threads and a supervisor, and every rule worth testing (ring buffers, the recipe grammar,
//! readiness, correlation tracing, assertions) is a pure function over plain data.

pub mod approval;
pub mod clock;
pub mod http;
pub mod jsonpath;
pub mod logaccess;
pub mod logbuf;
pub mod logfacets;
pub mod logfile;
pub mod logread;
pub mod logrecord;
pub mod logsql;
pub mod logtime;
pub mod prelude;
pub mod process;
pub mod queue;
pub mod ready;
pub mod recipe;
pub mod scenario;
pub mod timeline;
pub mod tree;
