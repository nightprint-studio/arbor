//! `bennu-jaxrs` — JAX-RS resources, read the way the runtime reads them.
//!
//! ## The URL is written in three places
//!
//! `@ApplicationPath("api")` on one class, `@Path("orders")` on another, `@Path("{id}")` on a method
//! — or the first of them in a `web.xml` nobody has opened in years. The URL a request hits appears
//! nowhere as one string, and every question about it (which method answers `GET /api/orders/42`,
//! does this `@PathParam` name a variable the route has) starts with putting it back together by
//! hand. This crate does the joining once, puts the routes in the shared Endpoints panel beside
//! Spring's and Struts', and checks the mistakes that only surface when a request arrives — or when
//! the application refuses to deploy.
//!
//! ## Conservative by construction
//!
//! Every value this crate reasons from is either read with certainty or not used: a `@Path` that is
//! a constant, an interface implemented twice, a project with two applications — each makes the
//! checks that depend on it go quiet rather than guess.

// Which annotations are JAX-RS's, and what their arguments certainly say.
pub mod known;
// Path templates: joining, variables, route identity.
pub mod paths;
// The owned model a scan produces and a buffer is read against.
pub mod model;
// The application path, and which resources the application deploys.
pub mod prefix;
// Reading resource methods out of a parsed file, interface inheritance included.
pub mod extract;
// Building the model, re-reading a buffer.
pub mod index;
// The diagnostics.
pub mod checks;
// Catalog rows, gutter, hover, completion, navigation.
pub mod intel;
// The extension itself.
pub mod ext;
pub mod prelude;
