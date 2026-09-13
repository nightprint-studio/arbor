//! What the engines are holding in memory — the building blocks of a backend's `__memory` answer.
//!
//! ## Estimates, and saying so
//!
//! There is no allocator keeping per-category statistics, so a structure is sized by walking it:
//! the `capacity` of every string and vector it owns (what is *allocated*, not what is in use — a
//! vector that grew and shrank keeps its high-water mark), plus a slot per hash-map bucket. The
//! allocator's own rounding is not counted, which is why [`MemoryEstimate::exact`] is reserved for
//! sums that are a real measurement of text held, and everything else says it is an estimate.
//!
//! Some structures are only **counted**. Sizing a deeply nested one — the parsed symbols of every
//! file, the Java model of every decoded type — would mean walking every member of it each time
//! somebody opens the breakdown, and a report that does that much work to say how much memory is in
//! use has answered its own question badly. The count is what moves when such a cache is the
//! problem, and the monitor shows the process's measured total beside the items, so what the items
//! do not cover is visible rather than hidden.

use std::collections::{HashMap, HashSet};
use std::mem::size_of;

/// One line of a memory breakdown, before the backend adds which project it belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryEstimate {
    /// What it is, in words a person reads.
    pub label: &'static str,
    /// Bytes, when the structure could be sized.
    pub bytes: Option<u64>,
    /// Entries, when that says something the bytes do not — or when bytes were not worth computing.
    pub count: Option<u64>,
    /// `bytes` is a sum of the text actually held, not an estimate of a structure.
    pub exact: bool,
}

impl MemoryEstimate {
    /// A structure sized by walking it — an estimate.
    pub fn sized(label: &'static str, count: usize, bytes: usize) -> Self {
        Self { label, bytes: Some(bytes as u64), count: Some(count as u64), exact: false }
    }

    /// Text held, summed — as exact as this gets.
    pub fn text(label: &'static str, count: usize, bytes: usize) -> Self {
        Self { label, bytes: Some(bytes as u64), count: Some(count as u64), exact: true }
    }

    /// A structure only counted — see the module doc for why some are.
    pub fn counted(label: &'static str, count: usize) -> Self {
        Self { label, bytes: None, count: Some(count as u64), exact: false }
    }
}

/// Heap owned by a list of strings.
pub(crate) fn string_vec(list: &Vec<String>) -> usize {
    list.capacity() * size_of::<String>() + list.iter().map(String::capacity).sum::<usize>()
}

/// Heap owned by a set of strings, a slot per bucket.
pub(crate) fn string_set(set: &HashSet<String>) -> usize {
    set.capacity() * (size_of::<String>() + 1) + set.iter().map(String::capacity).sum::<usize>()
}

/// Heap owned by a string-to-string map, a slot per bucket.
pub(crate) fn string_map(map: &HashMap<String, String>) -> usize {
    map.capacity() * (2 * size_of::<String>() + 1)
        + map.iter().map(|(k, v)| k.capacity() + v.capacity()).sum::<usize>()
}

/// Heap owned by a map from a string to a list of strings, a slot per bucket.
pub(crate) fn map_of_string_vecs(map: &HashMap<String, Vec<String>>) -> usize {
    map.capacity() * (size_of::<String>() + size_of::<Vec<String>>() + 1)
        + map.iter().map(|(k, v)| k.capacity() + string_vec(v)).sum::<usize>()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_list_counts_its_slots_and_its_text() {
        let mut list = Vec::with_capacity(4);
        list.push(String::from("abc"));
        let expected = 4 * size_of::<String>() + list[0].capacity();
        assert_eq!(string_vec(&list), expected);
    }

    #[test]
    fn an_empty_collection_owns_nothing_beyond_what_it_allocated() {
        assert_eq!(string_vec(&Vec::new()), 0);
        assert_eq!(string_map(&HashMap::new()), 0);
    }
}
