//! Port of `LongObjectMap.kt`: open addressing with linear probing, same hash mixing, same initial capacity,
//! same growth and the same backward-shift removal, so the slot layout (and therefore the iteration order of
//! [`LongObjectMap::values`]) is identical to the Kotlin map for the same sequence of operations.
//!
//! The Kotlin `filled` array is folded into the `Option` of each value slot: `Some` means filled.

pub struct LongObjectMap<V> {
    keys: Vec<i64>,
    values: Vec<Option<V>>,
    mask: usize,
    size: usize,
}

impl<V> LongObjectMap<V> {
    pub fn new(expected: usize) -> Self {
        let capacity = Self::table_size(expected);
        LongObjectMap {
            keys: vec![0; capacity],
            values: (0..capacity).map(|_| None).collect(),
            mask: capacity - 1,
            size: 0,
        }
    }

    fn table_size(expected: usize) -> usize {
        let mut size = 16;
        while size * 3 / 4 < expected {
            size *= 2;
        }
        size
    }

    #[inline]
    fn mix(key: i64) -> usize {
        let h = key.wrapping_mul(-0x61c8864680b583eb);
        let folded = h ^ ((h as u64) >> 32) as i64;
        (folded ^ ((folded as u64) >> 16) as i64) as i32 as u32 as usize
    }

    #[inline]
    fn capacity(&self) -> usize {
        self.mask + 1
    }

    pub fn len(&self) -> usize {
        self.size
    }

    /// Slot holding `key`, or the empty slot where it would be inserted.
    #[inline]
    fn find(&self, key: i64) -> (usize, bool) {
        let mut pos = Self::mix(key) & self.mask;
        while self.values[pos].is_some() {
            if self.keys[pos] == key {
                return (pos, true);
            }
            pos = (pos + 1) & self.mask;
        }
        (pos, false)
    }

    #[inline]
    pub fn get(&self, key: i64) -> Option<&V> {
        match self.find(key) {
            (pos, true) => self.values[pos].as_ref(),
            _ => None,
        }
    }

    #[inline]
    pub fn get_mut(&mut self, key: i64) -> Option<&mut V> {
        match self.find(key) {
            (pos, true) => self.values[pos].as_mut(),
            _ => None,
        }
    }

    pub fn put(&mut self, key: i64, value: V) -> Option<V> {
        let (pos, found) = self.find(key);
        if found {
            return self.values[pos].replace(value);
        }
        self.insert_at(pos, key, value);
        None
    }

    fn insert_at(&mut self, pos: usize, key: i64, value: V) {
        self.keys[pos] = key;
        self.values[pos] = Some(value);
        self.size += 1;
        if self.size > self.capacity() * 3 / 4 {
            self.rehash(self.capacity() * 2);
        }
    }

    /// Kotlin's `getOrPut`: `get(key) ?: create().also { put(key, it) }`.
    pub fn get_or_insert_with(&mut self, key: i64, create: impl FnOnce() -> V) -> &mut V {
        let (mut pos, found) = self.find(key);
        if !found {
            self.insert_at(pos, key, create());
            pos = self.find(key).0;
        }
        self.values[pos].as_mut().unwrap()
    }

    #[allow(dead_code)]
    pub fn remove(&mut self, key: i64) -> Option<V> {
        let (pos, found) = self.find(key);
        if !found {
            return None;
        }
        let old = self.values[pos].take();
        self.size -= 1;
        self.shift_keys(pos);
        old
    }

    /// Values in slot order, like `forEachValue`.
    pub fn values(&self) -> impl Iterator<Item = &V> {
        self.values.iter().filter_map(Option::as_ref)
    }

    pub fn slot_count(&self) -> usize {
        self.capacity()
    }

    #[inline]
    pub fn value_at(&self, slot: usize) -> Option<&V> {
        self.values[slot].as_ref()
    }

    #[allow(dead_code)]
    fn shift_keys(&mut self, start: usize) {
        let mut last = start;
        let mut pos = start;
        loop {
            pos = (pos + 1) & self.mask;
            loop {
                if self.values[pos].is_none() {
                    self.values[last] = None;
                    return;
                }
                let slot = Self::mix(self.keys[pos]) & self.mask;
                let stays = if last <= pos { last >= slot || slot > pos } else { last >= slot && slot > pos };
                if stays {
                    break;
                }
                pos = (pos + 1) & self.mask;
            }
            self.keys[last] = self.keys[pos];
            self.values[last] = self.values[pos].take();
            last = pos;
        }
    }

    fn rehash(&mut self, new_capacity: usize) {
        let old_keys = std::mem::replace(&mut self.keys, vec![0; new_capacity]);
        let old_values = std::mem::replace(&mut self.values, (0..new_capacity).map(|_| None).collect());
        self.mask = new_capacity - 1;
        for (key, value) in old_keys.into_iter().zip(old_values) {
            if let Some(value) = value {
                let mut pos = Self::mix(key) & self.mask;
                while self.values[pos].is_some() {
                    pos = (pos + 1) & self.mask;
                }
                self.keys[pos] = key;
                self.values[pos] = Some(value);
            }
        }
    }
}
