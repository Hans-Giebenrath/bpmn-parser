extern crate alloc;
use alloc::vec::Vec;
use core::slice::Iter;

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

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
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

    pub fn remove(&mut self, value: &T) {
        self.inner.retain(|e| e != value);
    }

    pub fn iter(&self) -> Iter<'_, T> {
        self.inner.iter()
    }

    pub fn intersect(&self, other: &Self) -> Vec<T>
    where
        T: Clone,
    {
        self.inner
            .iter()
            .filter(|e| other.contains(e))
            .cloned()
            .collect()
    }
}

impl<T: Eq> FromIterator<T> for VecSet<T> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        let mut set = Self::new();

        for value in iter {
            set.insert(value);
        }

        set
    }
}

impl<T: Eq> Extend<T> for VecSet<T> {
    fn extend<I>(&mut self, iter: I)
    where
        I: IntoIterator<Item = T>,
    {
        iter.into_iter().for_each(|item| {
            let _ = self.insert(item);
        });
    }
}
