//! `bennu-mockito` — Mockito used wrongly, caught in the test that is wrong.
//!
//! ## Why a mocking library needs checks at all
//!
//! Mockito keeps its state in a thread-local, and most of its misuse is only noticed when that state
//! is next looked at — by the **next** Mockito call. So the stack trace names a line that is fine,
//! often in a different test method, occasionally in a different test class that happened to run
//! next on the same thread. The broken line compiles and reads naturally. That is the whole case for
//! reading it statically: the defect is local, and the report of it is not.
//!
//! ## What is judged
//!
//! 1. **An unfinished stubbing** — `when(repo.find(1));` with nothing chained, or
//!    `doReturn(x).when(repo);` that never names the method. `UnfinishedStubbingException`.
//! 2. **An unfinished verification** — `verify(repo);`, `then(repo).should();`. It verifies nothing
//!    and throws `UnfinishedVerificationException` later.
//! 3. **Matchers mixed with plain values** — `verify(repo).save(any(), 5)`. When one argument is a
//!    matcher all of them must be: `InvalidUseOfMatchersException`. Alt+Enter wraps the values in
//!    `eq(…)`.
//! 4. **`@Mock` fields nothing initialises** — a JUnit test with `@Mock` fields and no extension,
//!    runner or `openMocks`. The fields are null and the first stubbing throws a
//!    `NullPointerException`. Alt+Enter adds the extension (or the runner, on JUnit 4).
//!
//! ## The bar
//!
//! Every name here is resolved through the file's imports — `when`, `any` and `not` are ordinary
//! identifiers that other libraries declare too — and every check goes silent the moment anything
//! it depends on is out of sight: a superclass, a class-level annotation it does not know, a value
//! computed by a call that might itself be a matcher.

// Recognising the shapes: stubbing starts, stubbers, verifications.
mod shapes;
// The one parse of a buffer, and name resolution on top of it.
mod file;
// Which class declares which method.
mod owners;
// Check 1 and 2: statements that start something and never finish it.
mod unfinished;
// Check 3: matchers beside plain values.
mod matchers;
// Whether an argument is certainly a plain value.
mod values;
// The `eq(…)` fix for check 3.
mod eq_fix;
// Check 4 and its fix: fields nothing initialises.
mod init;
// Turning computed edits into what the seam carries.
mod edits;
// The extension itself.
pub mod ext;
pub mod prelude;
