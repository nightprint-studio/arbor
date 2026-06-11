//! The closed standard library of pattern combinators and transforms, grouped
//! by role. Free functions (constructors / generators) live in the submodules;
//! transforms are inherent methods on [`crate::pattern::Pattern`] and become
//! available crate-wide as soon as their module is compiled.
//!
//! | Group | Module | Items |
//! |---|---|---|
//! | Composition | [`compose`] | `pure` `silence` `stack`/`par` `fastcat`/`seq` `slowcat`/`cat` `arrange` `cycles` `tracks` `track` |
//! | Time/structure | [`time`] | `fast` `slow` `rev` `every` `off` `late` `early` |
//! | Rhythm/probability | [`rhythm`] | `degrade` `degrade_by` `sometimes` `sometimes_by` |
//! | Voice/mix | [`voice`] | `gain` `pan` `room` `lpf` `hpf` `shift` `speed` `crush` `shape` `vel` `inst` `art` `scale` `jux` + [`voice::Param`] |
//! | Generative | [`generative`] | `rand` `choose` |
//! | File sources | [`source`] | `sample` `audio` (markers) |

pub mod compose;
pub mod generative;
pub mod rhythm;
pub mod source;
pub mod time;
pub mod voice;
