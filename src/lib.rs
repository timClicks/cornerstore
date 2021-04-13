//! A reasonably fast, parallel, in-memory key/value store with items that can
//! expire. Designed to support read-heavy workloads.
//!
//! CornerStore uses locks and shards to retain safe, shared access to
//! internal state.
//!
//! Writes (via set) are slower, so that reads (get) can be made faster.
//! Among other implementation details, CornerStore will pre-calculate hash
//! function values to speed up comparisons later on.
//!
//! ## References
//!
//! - https://doc.rust-lang.org/nightly/nightly-rustc/rustc_data_structures/sharded/index.html

#[global_allocator]
static GLOBAL: jemallocator::Jemalloc = jemallocator::Jemalloc;

use std::sync::RwLock;
use std::{collections::hash_map::DefaultHasher, error::Error};
use std::{
    collections::{BTreeMap, HashMap},
    time::Instant,
};

use std::hash::{Hash, Hasher};

const SHARDS: usize = 128;

type Bytes = Vec<u8>; // we can tolerate

/// HiddenKey is a pre-calculated hash of the key
/// provided by the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct HiddenKey(u64);

impl HiddenKey {
    #[inline]
    fn new(key: &[u8]) -> Self {
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        let ident = hasher.finish();

        HiddenKey(ident)
    }

    #[inline]
    fn shard(&self) -> usize {
        // avoid high bits and low bits, which are used by
        // the hashbrown crate (used for Rust's hashmap)

        // returns a value in the range 0..=127
        ((self.0 & 0xff000) >> 13) as usize
    }
}

#[derive(Debug, Clone)]
struct KeyValuePair {
    key: Vec<u8>,
    value: Vec<u8>,
    expiry: Option<Instant>,
}

/// Keys and values are untyped byte-streams of arbitrary length
#[derive(Debug)]
pub struct CornerStore {
    //  TODO: make lock more granular
    /// Map times to 1 or more keys. Using BTreeMap because we'll want
    /// to take ranges of values.
    expiry_times: RwLock<BTreeMap<Instant, Vec<HiddenKey>>>,

    /// Timestamp of when expired items were evicted from the cache  
    created_at: Instant,

    data: Vec<RwLock<HashMap<HiddenKey, KeyValuePair>>>,
}

impl CornerStore {
    pub fn new() -> Self {
        let mut store = Vec::with_capacity(SHARDS);
        for _ in 0..SHARDS {
            store.push(RwLock::new(HashMap::new()));
        }
        CornerStore {
            data: store,
            created_at: Instant::now(),
            expiry_times: RwLock::new(BTreeMap::new()),
        }
    }

    /// Get an item, but only if it has not expired
    pub fn get(&self, key: &[u8]) -> Result<Option<Bytes>, Box<dyn Error + '_>> {
        let hidden_key = HiddenKey::new(&key);

        let shard = &self.data[hidden_key.shard()];
        if let Some(kv_pair) = shard.read()?.get(&hidden_key) {
            if let Some(expiry) = kv_pair.expiry {
                if expiry <= Instant::now() {
                    return Ok(None);
                }
            }
            Ok(Some(kv_pair.value.clone()))
        } else {
            Ok(None)
        }
    }

    /// Retrieve a key/value paid, but only if they have not expired
    pub fn get_key_value(&self, key: &[u8]) -> Result<Option<(Bytes, Bytes)>, Box<dyn Error + '_>> {
        let hidden_key = HiddenKey::new(&key);
        let shard = &self.data[hidden_key.shard()];

        if let Some(kv_pair) = shard.read()?.get(&hidden_key) {
            Ok(Some((kv_pair.key.clone(), kv_pair.value.clone())))
        } else {
            Ok(None)
        }
    }

    /// Retrieve a value, even if it is stale
    pub fn get_unchecked(&self, key: &[u8]) -> Result<Option<Bytes>, Box<dyn Error + '_>> {
        let hidden_key = HiddenKey::new(&key);
        let shard = &self.data[hidden_key.shard()];

        if let Some(kv_pair) = shard.read()?.get(&hidden_key) {
            Ok(Some(kv_pair.value.clone()))
        } else {
            Ok(None)
        }
    }

    /// Retrieve a key/value paid, even if they are stale
    pub fn get_key_value_unchecked(
        &self,
        key: &[u8],
    ) -> Result<Option<(Bytes, Bytes)>, Box<dyn Error + '_>> {
        let hidden_key = HiddenKey::new(&key);
        let shard = &self.data[hidden_key.shard()];

        if let Some(kv_pair) = shard.read()?.get(&hidden_key) {
            Ok(Some((kv_pair.key.clone(), kv_pair.value.clone())))
        } else {
            Ok(None)
        }
    }

    /// Sets key to value, overwriting any previous value. Providing an optional `expiry`
    /// time treats the key/value pair as perishable.
    pub fn set(
        &mut self,
        key: &[u8],
        val: &[u8],
        expiry: Option<Instant>,
    ) -> Result<(), Box<dyn Error + '_>> {
        // willing to take the hit allocating on insertion
        let key = key.to_vec();
        let hidden_key = HiddenKey::new(&key);
        let value = val.to_vec();

        let kv_pair = KeyValuePair { key, value, expiry };

        if let Some(time) = expiry {
            self.expiry_times
                .write()?
                .entry(time)
                .or_insert_with(|| vec![hidden_key]);
        }

        {
            let shard = hidden_key.shard();
            let _ = self.data[shard].write()?.insert(hidden_key, kv_pair);
        }

        Ok(())
    }

    pub fn update(
        &mut self,
        key: &[u8],
        val: &[u8],
        expiry: Option<Instant>,
    ) -> Result<(), Box<dyn Error + '_>> {
        self.set(key, val, expiry)?;
        Ok(())
    }

    /// Removes the key/value pair from the store.
    pub fn remove(&mut self, key: &[u8]) -> Result<(), Box<dyn Error + '_>> {
        let hidden_key = HiddenKey::new(&key);

        let mut expiry = None;
        {
            let shard = &self.data[hidden_key.shard()];
            let mut lock = shard.write()?;
            if let Some(kv_pair) = lock.get_mut(&hidden_key) {
                expiry = kv_pair.expiry.clone(); // copying out of this scope to avoid deadlock
                lock.remove(&hidden_key);
            }
        }

        if let Some(expiry) = expiry {
            if let Some(keys) = self.expiry_times.write()?.get_mut(&expiry) {
                keys.retain(|&x| x != hidden_key);
            }
        }

        Ok(())
    }

    /// Remove any expired perishable items from the store
    pub fn evict(&mut self) -> Result<(), Box<dyn Error + '_>> {
        let now = Instant::now();

        let mut times_to_remove = vec![];
        let mut items_to_remove: Vec<HiddenKey> = vec![];

        for (expiry, items) in self.expiry_times.read()?.range(self.created_at..now) {
            // Avoid deleting things while holding the read lock - potential deadlock
            times_to_remove.push(expiry.clone());
            items_to_remove.extend(items);
        }

        for item in &items_to_remove {
            self.data[item.shard()].write()?.remove(&item);
        }
        for expiry in &times_to_remove {
            &mut self.expiry_times.write()?.remove(expiry);
        }

        Ok(())
    }
}

// /// C API.. in a pretty bad state

// /// Indicates that the function returned successfully
// pub const CNR_OK: isize = 0;

// /// Create an empty, in-process cache. Returns a pointer
// /// to the new instance.
// #[no_mangle]
// pub extern "C" fn cnr_init() -> *const CornerStore {
//     let s = CornerStore::new();
//     &s as *const _
// }

// #[no_mangle]
// pub extern "C" fn cnr_free(s: *mut CornerStore) {
//     unsafe {
//         drop_in_place(s);
//     }
// }

// /// Returns a [libc error code]
// ///
// ///
// /// - 1: success
// /// - `libc::EINVAL` (invalid argument) indicates that
// ///
// /// [libc error code]: https://www.gnu.org/software/libc/manual/html_node/Error-Codes.html
// #[no_mangle]
// pub extern "C" fn cnr_set(
//     shuttle: *mut CornerStore,
//     key: *mut u8,
//     key_len: usize,
//     val: *mut u8,
//     val_len: usize,
//     expiry: *const i64,
// ) -> isize {
//     if key.is_null() || val.is_null() || key_len == 0 || val_len == 0 {
//         return libc::EINVAL as isize;
//     }

//     let key = unsafe { std::slice::from_raw_parts(key, key_len) };

//     let val = unsafe { std::slice::from_raw_parts(val, val_len) };

//     CNR_OK
// }

#[cfg(test)]
mod tests {
    use std::time::Duration;
    use super::*;

    #[test]
    fn test_can_store_data() {
        let mut store = CornerStore::new();

        let key = b"greeting";
        let expected_value = b"hello";
        store.set(key, expected_value, None);

        let actual_value = store.get(key).unwrap();
        assert_eq!(actual_value.unwrap(), expected_value.to_vec())
    }

    #[test]
    fn test_expired_data_is_not_returned() {

        let mut store = CornerStore::new();

        let past = Instant::now() - Duration::new(1, 0);

        let key = b"greeting";
        let value = b"hello";
        let expected_value: Result<_, Box<dyn Error>> = Ok(None);
        store.set(key, value, Some(past));

        let actual_value = store.get(key);
        assert_eq!(actual_value.unwrap(), expected_value.unwrap());

        let expected_unchecked_value: Result<_, Box<dyn Error>> = Ok(Some(value.to_vec()));
        let actual_unchecked_value = store.get_unchecked(key);
        assert_eq!(expected_unchecked_value.unwrap(), actual_unchecked_value.unwrap());

        store.evict();

        let actual_value: Result<_, Box<dyn Error>> = store.get(key);
        assert_eq!(actual_value.unwrap(), None);
    }

    #[test]
    fn test_fresh_data_is_returned() {
        let mut store = CornerStore::new();

        let future = Instant::now() + Duration::new(1, 0);

        let key = b"greeting";
        let value = b"hello";
        let expected_value: Result<_, Box<dyn Error>> = Ok(Some(value.to_vec()));
        store.set(key, value, Some(future));

        let actual_value = store.get(key);
        assert_eq!(actual_value.unwrap(), expected_value.unwrap())
    }
}
