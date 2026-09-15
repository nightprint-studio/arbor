//! `bennu-scheduling` — the work a project does on its own, at times nobody is watching.
//!
//! ## Two ways a scheduled job silently never runs
//!
//! 1. **The expression is not one.** A five-field Unix crontab line pasted into
//!    `@Scheduled(cron = …)`, an hour of `25`, a misspelled `@dayly`. Spring and Quartz both refuse
//!    it — at startup, in a stack trace nobody reads until the report has not arrived for a week.
//! 2. **Scheduling was never switched on.** `@Scheduled` on a bean of a project with no
//!    `@EnableScheduling` and no Spring Boot autoconfiguration is an annotation with nothing behind
//!    it. Nothing fails. Nothing logs. The method is simply never called, and the class looks
//!    completely correct.
//!
//! The second is the one worth building the crate for, because there is no error to search for:
//! the symptom is *"il report non arriva più"*, and the cause is one missing annotation in a
//! configuration class somebody deleted six months ago.
//!
//! ## And the thing that helps every day
//!
//! Nobody reads cron. [`cron::describe`] puts `0 0 2 * * ?` into words under the caret — *every day
//! at 02:00* — at the moment it is being written rather than at the incident review.

// Cron expressions: valid or not, and what they say in words.
pub mod cron;
// The extension itself.
pub mod ext;
pub mod prelude;
// Finding scheduled work in a source.
pub mod jobs;
