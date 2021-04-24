# about

CORNERSTORE is an easy-to-use, in-memory cache with items which
can expire. It can safely be shared across threads.

is a friendly key-value store for perishable items.
Like corner stores in real life, this one is fast and convenient.
It is intended for read-heavy workloads. All key/value pairs can
be given an optional expiry time.

A `CornerStore` instance is thread-safe. It divides its data across
128 shards.

# usage

CORNERSTORE is a library. It does not have a command-line interface or
listen to a socket, such as what you might expect from memcached or Redis.

Start by importing `CornerStore` then creating
and instance with the `new()` method. For the convenience of, it can be useful
to also bring some types from `std::time` in local scope, as well as the `std::error::Error` trait.

```rust
use cornerstore::CornerStore;
use std::error::Error;
use std::time::{Duration, Instant};

// ...

fn main() -> Result<(), Box<dyn Error + '_>> {
    let mut store = CornerStore::new();

    // ...

    Ok(())
}
```

Some examples of the API:

* Storing an item that does not expire:

    ```rust
    let key = b"greeting";
    let value = b"hello";
    let expiry = None;
    store.set(key, expected_value, expiry)?;
    ```

* Storing an item that expires in one minute:

    ```rust
    let key = b"greeting";
    let value = b"hello from the future";
    let expiry = Some(Instant::now() + Duration::new(60, 0))
    store.set(key, expected_value, expiry)?;
    ```

* Retrieving an item:

    ```rust
    let key   = b"greeting";
    if Some(value) = store.get(key)? {
        // warning - prints raw bytes
        println!("{:?}", value);
    };
    ```

* Retrieving an item without checking the expiry date:

    ```rust
    let key   = b"greeting";
    if Some(value) = store.get_unchecked(key)? {
        // warning - prints raw bytes
        println!("{:?}", value);
    };
    ```

* Remove any expired perishable items from the store:

    ```rust
    store.evict()?;
    ```

## cargo features

- `safe-input`  
   If you know that your store will not be subjected to DDoS attacks,
   you can increase its performance by enabling `safe-input`. `safe-input` 
   uses the `fxcrate` for hashing, which is faster than Rust's default.

# goals

To act as a library for many client language implementations.

## help wanted

- Is it possible to avoid returning `Result<Option<K, V>>` when returning a value? Unwrapping twice is slightly icky.
- how to benchmark this thing? I've experimented a little bit with jonhoo's `bustle` crate, but it's hard to coerce `[u8]` streams to `f64`.

# legal

_Sorry about the legalese. It's unfortunate, but important._

## authorship and copyright

Original components of CORNERSTORE have been written by
Tim McNamara (@timClicks). Copyright these contributions
have been assigned to Fiorenza Limited (NZBN 9429042165200).

CORNERSTORE source and binary distributions are released
under the Apache 2 License. See the LICENCE file for your
rights and obligations under this licence.

## trade mark

CORNERSTORE is an unregistered trade mark of Fiorenza Limited
(NZBN 9429042165200).

## consumer projection

If you are using CORNERSTORE for your own use, you are entitled
to mandatory rights under the Consumer Guarantees Act. Please
bear in mind that you are getting software for free that you
have randomly downloaded from the Internet.
