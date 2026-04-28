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

impl<T: Eq> VecSet<T> {
    pub fn new() -> Self {
        Self { inner: Vec::new() }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            inner: Vec::with_capacity(capacity),
        }
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
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

    //pub fn remove(&mut self, value: &T) -> bool {
    //    if let Some(pos) = self.inner.iter().position(|x| x == value) {
    //        self.inner.swap_remove(pos);
    //        true
    //    } else {
    //        false
    //    }
    //}

    pub fn take(&mut self, value: &T) -> Option<T> {
        if let Some(pos) = self.inner.iter().position(|x| x == value) {
            Some(self.inner.swap_remove(pos))
        } else {
            None
        }
    }

    pub fn get(&self, value: &T) -> Option<&T> {
        self.inner.iter().find(|x| *x == value)
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.inner.iter()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut T> {
        self.inner.iter_mut()
    }
}
