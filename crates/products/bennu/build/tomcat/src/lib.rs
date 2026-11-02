//! `bennu-tomcat` — running a web application on Apache Tomcat, minus the process and the network.
//!
//! The backend starts the JVM and downloads the archive; everything it needs to *decide* lives here
//! so it can be tested without either:
//!
//! * [`catalog`] — the release lines (Tomcat 8.5 to 11), which servlet namespace each speaks, where
//!   a release is published, and how to read the directory listing and checksum files there;
//! * [`install`] — what an unpacked Tomcat is: its version, and so its namespace and Java floor;
//! * [`context`] — the application's `context.xml`, generated from the run configuration;
//! * [`base`] — the `CATALINA_BASE` directory a configuration gets: `server.xml`, the context, the
//!   driver jars, and the files copied from the installation;
//! * [`launch`] — the JVM command line, secrets handed over as system properties and masked in what
//!   is displayed;
//! * [`drivers`] — which classpath jar is the JDBC driver a data source names;
//! * [`scan`] — the JNDI names and servlet namespace a project's own files declare.
//!
//! Everything public is re-exported from [`prelude`].

pub mod base;
pub mod catalog;
pub mod context;
pub mod drivers;
pub mod install;
pub mod launch;
pub mod prelude;
pub mod scan;
