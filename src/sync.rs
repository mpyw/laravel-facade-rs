//! Lock helpers that ignore poisoning.
//!
//! A panic inside user code (for example a failed mock expectation) must not
//! break every facade call that follows it.

use std::sync::{PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard};

pub(crate) fn read<T>(lock: &RwLock<T>) -> RwLockReadGuard<'_, T> {
    lock.read().unwrap_or_else(PoisonError::into_inner)
}

pub(crate) fn write<T>(lock: &RwLock<T>) -> RwLockWriteGuard<'_, T> {
    lock.write().unwrap_or_else(PoisonError::into_inner)
}
