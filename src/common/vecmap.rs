#[derive(Debug, Clone)]
pub struct VecMap<K, V> {
    inner: Vec<(K, V)>,
}

impl<K, V> Default for VecMap<K, V> {
    fn default() -> Self {
        Self {
            inner: Default::default(),
        }
    }
}

impl<K: Eq, V: Eq> PartialEq for VecMap<K, V> {
    fn eq(&self, other: &Self) -> bool {
        self.len() == other.len() && self.inner.iter().all(|e| other.inner.contains(e))
    }
}

impl<K: Eq, V: Eq> Eq for VecMap<K, V> {}

impl<K: Eq, V> VecMap<K, V> {
    pub fn new() -> Self {
        Self { inner: Vec::new() }
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn clear(&mut self) {
        self.inner.clear();
    }

    pub fn contains(&self, value: &K) -> bool {
        self.inner.iter().any(|(x, _)| x == value)
    }

    pub fn insert(&mut self, key: K, value: V) -> bool {
        if self.contains(&key) {
            false
        } else {
            self.inner.push((key, value));
            true
        }
    }

    pub fn get(&self, value: &K) -> Option<&V> {
        self.inner.iter().find(|(x, _)| x == value).map(|(_, v)| v)
    }
}
