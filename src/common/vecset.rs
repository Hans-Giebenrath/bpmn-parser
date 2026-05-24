use std::slice::{Iter, IterMut};

#[derive(Debug, Clone)]
pub struct VecSet<T> {
    inner: Vec<T>,
}

impl<T> Default for VecSet<T> {
    fn default() -> Self {
        Self {
            inner: Default::default(),
        }
    }
}

impl<T: Eq> PartialEq for VecSet<T> {
    fn eq(&self, other: &Self) -> bool {
        self.len() == other.len() && self.iter().all(|e| other.contains(e))
    }
}

impl<T: Eq> Eq for VecSet<T> {}

impl<T: Eq> VecSet<T> {
    pub fn new() -> Self {
        Self { inner: Vec::new() }
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn clear(&mut self) {
        self.inner.clear();
    }

    pub fn contains(&self, value: &T) -> bool {
        self.inner.iter().any(|x| x == value)
    }

    pub fn insert(&mut self, value: T) -> bool {
        if self.contains(&value) {
            false
        } else {
            self.inner.push(value);
            true
        }
    }

    pub fn iter(&self) -> Iter<'_, T> {
        self.inner.iter()
    }
}
