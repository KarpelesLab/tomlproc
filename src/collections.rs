//! The maps and sets the crate is built on.
//!
//! With `std` these hash; without it they are the ordered trees from `alloc`.
//! Both are only ever used with owned string or path keys, which implement
//! everything either one asks for.

#[cfg(not(feature = "std"))]
pub(crate) use alloc::collections::{BTreeMap as Map, BTreeSet as Set};
#[cfg(feature = "std")]
pub(crate) use std::collections::{HashMap as Map, HashSet as Set};
