//! An [`arc_swap::ArcSwap`]-based cell
//!
//! The purpose of [`ArcCell`] is to allow an [`std::sync::Arc`] to be used like cell, reading
//! what's held inside atomically but also allowing atomic swap operations. The [`ArcCell`] is NOT a
//! way to have multiple references to the same data, as [`arc_swap::ArcSwap`] paired with a normal
//! [`std::sync::Arc`] is already sufficient for that purpose.

use std::{ops::Deref, sync::Arc};

use arc_swap::{
    ArcSwapAny, Guard as InnerGuard,
    strategy::{DefaultStrategy, Strategy},
};

/// An utility to use an [ `Arc` ] similarly to a cell
///
/// # Invariants
/// The inner [`Arc`] is inaccessible outside this cell, therefore the strong count is always less
/// than or equal to 1 + number of outstanding [`Guard`]s
pub struct ArcCell<T, S: Strategy<Arc<T>> = DefaultStrategy>(ArcSwapAny<Arc<T>, S>);

impl<T: std::fmt::Debug, S: Strategy<Arc<T>>> std::fmt::Debug for ArcCell<T, S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("ArcCell").field(&self.0).finish()
    }
}

impl<T, S: Strategy<Arc<T>> + Default> ArcCell<T, S> {
    /// Construct a new [`ArcCell`]
    pub fn new(data: T) -> Self {
        Self::with_strategy(data, Default::default())
    }
}

impl<T, S: Strategy<Arc<T>>> ArcCell<T, S> {
    /// Construct a new [`ArcCell`] with the given [`arc_swap::strategy::Strategy`]
    pub fn with_strategy(data: T, strategy: S) -> Self {
        Self(ArcSwapAny::with_strategy(Arc::new(data), strategy))
    }

    /// Provides a temporary borrow of the object inside.
    ///
    /// Behaves the same as [`InnerGuard`], except it doesn't expose the inner [`Arc`] to avoid
    /// cheap clones
    ///
    /// # Warning
    /// While the reference is alive, the strong count of the inner [`Arc`] is increased
    pub fn load(&self) -> Guard<T, S> {
        Guard(self.0.load())
    }

    /// Exchanges the value inside this instance, returning the raw underlying [`Arc`].
    ///
    /// This differs from [`Self::swap`] as the [`Arc`] might have some outstanding references due
    /// to outstanding [`Self::load`]s.
    ///
    /// Any further [`Self::load`]s will return the newly stored value.
    pub fn raw_swap(&self, new: T) -> Arc<T> {
        self.0.swap(Arc::new(new))
    }

    /// Exchanges the value inside this instance.
    ///
    /// This function is potentially costly, as it will wait for the inner [`Arc`] to have exactly 1
    /// remaining strong reference before yielding the underlying value
    ///
    /// # Warning
    /// As with other "locking" mechanisms, holding a [`Guard`] and attempting this swap in the same
    /// thread WILL result in a deadlock.
    pub fn swap(&self, new: T) -> T {
        let mut arc = self.raw_swap(new);

        loop {
            match Arc::try_unwrap(arc) {
                Ok(inner) => return inner,
                Err(this) => {
                    arc = this;
                }
            }
        }
    }
}

impl<T: Clone, S: Strategy<Arc<T>>> ArcCell<T, S> {
    /// Exchanges the value inside this instance, returning a clone immediately
    ///
    /// This function differs from [`Self::swap`] as it fallbacks to cloning the held value if
    /// there's more than 1 strong reference to the underlying [`Arc`].
    #[allow(dead_code)] // currently unused in this library
    pub fn swap_immediately(&self, new: T) -> T {
        let arc = self.raw_swap(new);

        Arc::unwrap_or_clone(arc)
    }
}

impl<T: Clone, S: Strategy<Arc<T>> + Default> Clone for ArcCell<T, S> {
    /// This is the core of the [`ArcCell`] invariant: no clones of the underlying Arc
    fn clone(&self) -> Self {
        let inner = self.0.load();
        // we enforce that T is being cloned and not the Arc holding it
        let cloned = T::clone(&inner);

        Self::new(cloned)
    }
}

/// Guard for [`ArcCell`]
///
/// Enforces the [`ArcCell`] invariants by ensuring the inner [`Arc`] is not accessible directly
///
/// # Warning
/// WILL cause a deadlock if held while attempting to [`ArcCell::swap`] in the same thread
pub struct Guard<T, S: Strategy<Arc<T>>>(InnerGuard<Arc<T>, S>);

impl<T, S: Strategy<Arc<T>>> Deref for Guard<T, S> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.0.deref().deref()
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use quanta::Instant;

    use super::*;

    #[test]
    fn load_increases_refcount() {
        let cell = ArcCell::<_>::new(1);

        // Arc+1
        let guard = cell.load();
        // Arc
        let inner = cell.raw_swap(2);

        assert_eq!(
            Arc::strong_count(&inner),
            2,
            "Should have 2 strong references, one in the `Guard` and one in the original `Arc`"
        );

        assert_eq!(*guard, *inner, "Holding the same value");
    }

    #[test]
    fn swap_changes_load_result() {
        let cell = ArcCell::<_>::new(1);
        let before_swap = cell.load();

        let previous_value = cell.raw_swap(2);
        let after_swap = cell.load();

        assert_eq!(
            *before_swap, *previous_value,
            "Guard before swap should be referencing previous value"
        );
        assert_ne!(
            *before_swap, *after_swap,
            "Guard after swap should be referencind a different value than the one before the swap"
        );
    }

    #[derive(Debug)]
    struct CloneCounter(pub usize);
    impl Clone for CloneCounter {
        fn clone(&self) -> Self {
            Self(self.0 + 1)
        }
    }

    #[test]
    fn swap_immediately_with_outstanding_loads_clones() {
        let cell = ArcCell::<_>::new(CloneCounter(0));
        let borrow = cell.load();

        let value = cell.swap_immediately(CloneCounter(42));

        assert_eq!(value.0, 1, "Should have cloned the inner value");
        assert_eq!((*borrow).0, 0, "Original should be unchanged");

        let new_value = cell.load();
        assert_eq!((*new_value).0, 42, "New value inserted")
    }

    #[test]
    fn swap_immediately_avoids_cloning_if_single_owner() {
        let cell = ArcCell::<_>::new(CloneCounter(0));

        let value = cell.swap_immediately(CloneCounter(42));

        assert_eq!(value.0, 0, "Should have NOT cloned the inner value");

        let new_value = cell.load();
        assert_eq!((*new_value).0, 42, "New value inserted")
    }

    #[test]
    fn swap_waits_for_single_owner() {
        let original_value = 0;
        let cell = ArcCell::<_>::new(original_value);
        let cell = Arc::new(cell);

        let borrow_duration = Duration::from_millis(1000);

        // long lived borrow
        let borrow = cell.load();

        let other_ref = std::thread::spawn(move || {
            assert_eq!(*borrow, original_value, "Should be the same value");

            std::thread::sleep(borrow_duration);
            assert_eq!(*borrow, original_value, "Should still be the same value");

            // explicitly drop the outstanding guard
            std::mem::drop(borrow);
        });

        let cell_clone = cell.clone();
        let observes_swap = std::thread::spawn(move || {
            let mut start = None;
            let mut counter = 0;
            loop {
                // this can affect the swap as well
                // but we keep dropping the value as well
                if *cell_clone.load() != original_value {
                    break;
                } else if start.is_none() {
                    // populate `now` after the first load
                    start = Some(Instant::now());
                }

                counter += 1;
            }

            (counter, start.unwrap().elapsed())
        });

        let now = Instant::now();
        let previous = cell.swap(42);
        let full_swap_time = now.elapsed();

        // let's account for some timing skew due to thread spawn
        assert!(
            full_swap_time >= (borrow_duration / 2),
            "Should have waited some time before returning"
        );
        assert_eq!(previous, original_value, "Previous value should match inserted");

        assert!(other_ref.is_finished(), "Other thread should have finished after swap returns");

        let (reads_before_swap, swap_time) = observes_swap.join().unwrap();
        assert!(
            swap_time < (full_swap_time.mul_f64(0.1)),
            "Swap should be observable by other references much before the swap call has finished"
        );
        assert!(reads_before_swap > 0, "Should have read at least once before swap was in effect");
    }

    #[test]
    fn swap_instantly_with_single_owner() {
        let cell = ArcCell::<_>::new(0);

        let now = Instant::now();
        let previous = cell.swap(42);

        let elapsed = now.elapsed();

        assert!(
            elapsed <= Duration::from_millis(100),
            "Should NOT have waited much time before returning"
        );
        assert_eq!(previous, 0, "Previous value should match inserted");
    }
}
