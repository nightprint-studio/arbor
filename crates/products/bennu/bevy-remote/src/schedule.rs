//! A queue of things to do at a later moment — how a key press gets its release, and a drag its steps.
//!
//! Writing "down" and "up" in the same frame would make the game see a key that was never held
//! (`pressed()` is false by the time it looks), so every held input is two entries: now, and `hold`
//! seconds later. Time is the caller's clock in seconds; this only orders and releases.

#[derive(Debug)]
pub struct Queue<T> {
    items: Vec<(f64, u64, T)>,
    next_seq: u64,
}

impl<T> Default for Queue<T> {
    fn default() -> Self {
        Self { items: Vec::new(), next_seq: 0 }
    }
}

impl<T> Queue<T> {
    /// Run `item` when the clock reaches `at`. Entries due at the same instant run in the order pushed.
    pub fn push(&mut self, at: f64, item: T) {
        self.items.push((at, self.next_seq, item));
        self.next_seq += 1;
    }

    /// Everything due at `now`, oldest first, removed from the queue.
    pub fn drain_due(&mut self, now: f64) -> Vec<T> {
        let mut due: Vec<(f64, u64, T)> = Vec::new();
        let mut keep = Vec::with_capacity(self.items.len());
        for entry in self.items.drain(..) {
            if entry.0 <= now { due.push(entry) } else { keep.push(entry) }
        }
        self.items = keep;
        due.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        due.into_iter().map(|(_, _, item)| item).collect()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::Queue;

    #[test]
    fn items_come_out_when_due_in_time_then_push_order() {
        let mut q = Queue::default();
        q.push(2.0, "late");
        q.push(1.0, "b");
        q.push(1.0, "c");
        q.push(0.5, "a");
        assert_eq!(q.drain_due(0.4), Vec::<&str>::new());
        assert_eq!(q.drain_due(1.0), ["a", "b", "c"]);
        assert_eq!(q.len(), 1);
        assert_eq!(q.drain_due(5.0), ["late"]);
        assert!(q.is_empty());
    }

    #[test]
    fn clear_drops_everything() {
        let mut q = Queue::default();
        q.push(1.0, 1);
        q.clear();
        assert!(q.drain_due(10.0).is_empty());
    }
}
