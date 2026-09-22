extern crate alloc;
use alloc::vec::Vec;

#[derive(Debug, Clone)]
pub struct VecMap<K, V> {
    inner: Vec<(K, V)>,
}

impl<K, V> IntoIterator for VecMap<K, V> {
    type Item = (K, V);

    type IntoIter = alloc::vec::IntoIter<(K, V)>;

    fn into_iter(self) -> Self::IntoIter {
        self.inner.into_iter()
    }
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

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn clear(&mut self) {
        self.inner.clear();
    }

    pub fn contains_key(&self, value: &K) -> bool {
        self.inner.iter().any(|(x, _)| x == value)
    }

    pub fn insert(&mut self, key: K, value: V) -> bool {
        if self.contains_key(&key) {
            false
        } else {
            self.inner.push((key, value));
            true
        }
    }

    pub fn get(&self, value: &K) -> Option<&V> {
        self.inner.iter().find(|(x, _)| x == value).map(|(_, v)| v)
    }

    pub fn get_mut(&mut self, value: &K) -> Option<&mut V> {
        self.inner
            .iter_mut()
            .find(|(x, _)| x == value)
            .map(|(_, v)| v)
    }

    pub fn remove(&mut self, value: &K) -> Option<V> {
        for idx in 0..self.inner.len() {
            if self.inner[idx].0 == *value {
                return Some(self.inner.swap_remove(idx).1);
            }
        }
        None
    }

    pub fn keys(&self) -> impl Iterator<Item = &K> {
        self.inner.iter().map(|(k, _)| k)
    }

    pub fn values(&self) -> impl Iterator<Item = &V> {
        self.inner.iter().map(|(_, v)| v)
    }

    pub fn iter(&self) -> impl Iterator<Item = &(K, V)> {
        self.inner.iter()
    }

    pub fn entry(&mut self, key: K) -> Entry<'_, K, V> {
        Entry { map: self, key }
    }
}

pub struct Entry<'a, K, V> {
    map: &'a mut VecMap<K, V>,
    key: K,
}

impl<'a, K: Eq, V> Entry<'a, K, V> {
    pub fn or_default(self) -> &'a mut V
    where
        V: Default,
    {
        // First see if the key already exists.
        if let Some(idx) = self.map.inner.iter().position(|(k, _)| *k == self.key) {
            return &mut self.map.inner[idx].1;
        }

        // Otherwise, insert a default value.
        self.map.inner.push((self.key, V::default()));
        &mut self.map.inner.last_mut().unwrap().1
    }

    pub fn or_insert_with<F>(self, default: F) -> &'a mut V
    where
        F: FnOnce() -> V,
    {
        if let Some(idx) = self.map.inner.iter().position(|(k, _)| *k == self.key) {
            return &mut self.map.inner[idx].1;
        }

        self.map.inner.push((self.key, default()));
        &mut self.map.inner.last_mut().unwrap().1
    }
}

impl<K: Eq, V> FromIterator<(K, V)> for VecMap<K, V> {
    fn from_iter<I>(iter: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
    {
        let mut result = Self::default();
        for (k, v) in iter {
            let _: bool = result.insert(k, v);
        }
        result
    }
}

impl<K: Eq, V> Extend<(K, V)> for VecMap<K, V> {
    fn extend<I>(&mut self, iter: I)
    where
        I: IntoIterator<Item = (K, V)>,
    {
        iter.into_iter().for_each(|(key, value)| {
            let _: bool = self.insert(key, value);
        });
    }
}

impl<K, V> core::ops::Index<&K> for VecMap<K, V>
where
    K: PartialEq,
{
    type Output = V;

    fn index(&self, key: &K) -> &Self::Output {
        self.inner
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v)
            .expect("no entry found for key")
    }
}
