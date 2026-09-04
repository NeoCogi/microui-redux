//! Transactional ownership for fallible native-resource construction.
//!
//! Driver APIs return plain copyable handles, so Rust cannot clean up a partially built object by
//! itself. `ResourceGuard` owns one acquired resource and its exact destruction operation until
//! the caller commits the resource into a longer-lived owner with [`ResourceGuard::into_inner`].

/// Runs `cleanup` unless the guarded resource is explicitly committed.
///
/// The callback is `FnOnce` because native resources must be destroyed at most once. Keeping the
/// callback beside the resource also makes this useful for both aggregate resources (a bound
/// buffer) and individual opaque handles (a shader module or semaphore).
pub struct ResourceGuard<T, F: FnOnce(T)> {
    /// Resource retained until either commit transfers it or Drop passes it to `cleanup`.
    resource: Option<T>,
    /// Exact one-shot destruction operation paired with the acquired resource.
    cleanup: Option<F>,
}

impl<T, F: FnOnce(T)> ResourceGuard<T, F> {
    /// Arms cleanup immediately after a successful native allocation.
    pub fn new(resource: T, cleanup: F) -> Self {
        // Store both values as Options so commit and Drop can move them exactly once without
        // requiring either an artificial Default value or a copyable native handle.
        Self {
            resource: Some(resource),
            cleanup: Some(cleanup),
        }
    }

    /// Borrows the resource while construction is still transactional.
    pub fn get(&self) -> &T {
        // A missing value means this consuming guard was already committed, which safe borrowing
        // cannot observe unless the implementation's ownership invariant is broken.
        self.resource.as_ref().expect("resource guard is always armed before commit")
    }

    /// Mutably borrows the resource while construction is still transactional.
    pub fn get_mut(&mut self) -> &mut T {
        // Vulkan aggregate construction updates layout and other bookkeeping before committing the
        // complete resource; the guard remains responsible for cleanup throughout that mutation.
        self.resource.as_mut().expect("resource guard is always armed before commit")
    }

    /// Transfers ownership to the completed object and permanently disarms cleanup.
    pub fn into_inner(mut self) -> T {
        // Taking the resource leaves Drop with no `(resource, cleanup)` pair, so the longer-lived
        // recipient becomes the sole owner without invoking the destruction callback.
        self.resource.take().expect("resource guard cannot be committed twice")
    }
}

impl<T, F: FnOnce(T)> Drop for ResourceGuard<T, F> {
    /// Cleans an acquired-but-uncommitted resource during ordinary errors and unwinding.
    fn drop(&mut self) {
        // Move both halves out before invoking arbitrary cleanup code, making re-entry or unwinding
        // unable to execute the callback a second time.
        if let (Some(resource), Some(cleanup)) = (self.resource.take(), self.cleanup.take()) {
            cleanup(resource);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, rc::Rc};

    use super::ResourceGuard;

    /// Simulates a three-stage native constructor and injects a failure after any acquisition.
    fn construct(fail_after: Option<u8>, cleanup_log: Rc<RefCell<Vec<u8>>>) -> Result<[u8; 3], ()> {
        let log = cleanup_log.clone();
        let first = ResourceGuard::new(1, move |value| log.borrow_mut().push(value));
        if fail_after == Some(1) {
            return Err(());
        }

        let log = cleanup_log.clone();
        let second = ResourceGuard::new(2, move |value| log.borrow_mut().push(value));
        if fail_after == Some(2) {
            return Err(());
        }

        let log = cleanup_log;
        let third = ResourceGuard::new(3, move |value| log.borrow_mut().push(value));
        if fail_after == Some(3) {
            return Err(());
        }

        Ok([first.into_inner(), second.into_inner(), third.into_inner()])
    }

    #[test]
    fn fault_injection_cleans_every_acquired_stage_in_reverse_order() {
        for (failure, expected) in [(1, vec![1]), (2, vec![2, 1]), (3, vec![3, 2, 1])] {
            let cleanup_log = Rc::new(RefCell::new(Vec::new()));
            assert_eq!(construct(Some(failure), cleanup_log.clone()), Err(()));
            assert_eq!(*cleanup_log.borrow(), expected, "failure after acquisition {failure}");
        }
    }

    #[test]
    fn committing_all_stages_disarms_cleanup() {
        let cleanup_log = Rc::new(RefCell::new(Vec::new()));
        assert_eq!(construct(None, cleanup_log.clone()), Ok([1, 2, 3]));
        assert!(cleanup_log.borrow().is_empty());
    }

    #[test]
    fn guarded_resource_can_be_updated_before_commit() {
        let mut guard = ResourceGuard::new(String::from("pending"), |_| panic!("committed resource was cleaned"));
        assert_eq!(guard.get(), "pending");
        guard.get_mut().push_str(" resource");
        assert_eq!(guard.into_inner(), "pending resource");
    }

    #[test]
    fn failed_replacement_preserves_installed_resource() {
        let cleanup_log = Rc::new(RefCell::new(Vec::new()));
        let mut installed = 4;

        let log = cleanup_log.clone();
        let pending = ResourceGuard::new(9, move |value| log.borrow_mut().push(value));
        // Simulate a later constructor stage failing before the replacement commits.
        drop(pending);

        assert_eq!(installed, 4);
        assert_eq!(*cleanup_log.borrow(), vec![9]);

        let log = cleanup_log.clone();
        let pending = ResourceGuard::new(10, move |value| log.borrow_mut().push(value));
        let previous = std::mem::replace(&mut installed, pending.into_inner());
        cleanup_log.borrow_mut().push(previous);

        assert_eq!(installed, 10);
        assert_eq!(*cleanup_log.borrow(), vec![9, 4]);
    }
}
