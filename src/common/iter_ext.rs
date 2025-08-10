pub trait IteratorExt: Iterator {
    /// Returns true if the iterator can yield at least `n` items.
    ///
    /// Consumes up to `n` items from the iterator.
    fn has_at_least(&mut self, n: usize) -> bool {
        if n == 0 {
            return true;
        }
        // Advance exactly n-1 items, then check if the nth exists.
        self.nth(n - 1).is_some()
    }
}

impl<I: Iterator> IteratorExt for I {}
