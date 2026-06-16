//! The closed standard library of pattern combinators and transforms, grouped
//! by role. Free functions (constructors / generators) live in the submodules;
//! transforms are inherent methods on [`crate::pattern::Pattern`] and become
//! available crate-wide as soon as their module is compiled.
//!
//! | Group | Module | Items |
//! |---|---|---|
//! | Composition | [`compose`] | `pure` `silence` `stack`/`par` `fastcat`/`seq` `slowcat`/`cat` `timecat` `polymeter` `arrange` `cycles` `tracks` `track` |
//! | Time/structure | [`time`] | `fast` `slow` `rev` `every` `off` `late` `early` |
//! | Structural | [`structural`] | `within` `inside` `iter` `palindrome` `chunk` `swing_by` |
//! | Patternised args | [`patterned`] | `inner_join_with` `fast_with` `slow_with` `euclid_with` |
//! | Rhythm/probability | [`rhythm`] | `degrade` `degrade_by` `sometimes` `sometimes_by` `euclid` |
//! | Voice/mix | [`voice`] | `gain` `pan` `room` `lpf` `hpf` `shift` `speed` `crush` `shape` `vel` `inst` `art` `scale` `jux` + [`voice::Param`] |
//! | Generative | [`generative`] | `rand` `choose` |
//! | Signals | [`signal`] | `sine` `saw` `isaw` `tri` `square` + `Pattern::<f64>::range` |
//! | File sources | [`source`] | `sample` `audio` (markers) |

pub mod compose;
pub mod generative;
pub mod patterned;
pub mod rhythm;
pub mod signal;
pub mod source;
pub mod structural;
pub mod time;
pub mod voice;
