//! The suite.
//!
//! Everything in this crate is pure — text and a data struct in, a statement or a
//! refusal out — so there is nothing to mock and nothing to set up. The fixture is
//! a small Italian schema because the abbreviations it has to survive are the ones
//! real users type: accented values, account codes with leading zeros, and two
//! foreign keys from the same table to the same other one.

mod context;
mod expansion;
mod fixture;
mod parsing;
mod refusal;
mod rendering;
mod wire;
