use crate::common::harness::{Environment, LogTest};
use crate::common::utils::run_until;
use std::sync::atomic::Ordering;

mod common;

// Todo: Tests derived on the C++ standard
// https://en.cppreference.com/w/cpp/atomic/atomic_thread_fence
// https://en.cppreference.com/w/cpp/atomic/memory_order

// https://en.cppreference.com/w/cpp/atomic/atomic_thread_fence
// Fence-atomic synchronization

/*
Fence-atomic synchronization

A release fence F in thread A synchronizes-with atomic acquire operation Y in thread B, if

* there exists an atomic store X (with any memory order)
* Y reads the value written by X (or the value would be written by release sequence headed by X if X were a release operation)
* F is sequenced-before X in thread A

In this case, all non-atomic and relaxed atomic stores that are sequenced-before F in thread A will happen-before all non-atomic and relaxed atomic loads from the same locations made in thread B after Y.
*/

#[test]
fn test_fence_atomic() {
    fn inner(use_release: bool) -> Vec<usize> {
        let mut lt = LogTest::default();

        lt.add(move |mut eg: Environment| {
            eg.a.store(1, Ordering::Relaxed); // Target
            eg.fence(Ordering::Release); // F Sequenced before X in thread A

            // Note X swaps between C and B here. The synchronizing load can be the X, or a
            // relaxed store released by X
            if use_release {
                // Optionally model release sequence headed by X
                eg.b.store(1, Ordering::Relaxed);
                eg.c.store(1, Ordering::Release); // Atomic store X
            } else {
                eg.b.store(1, Ordering::Release); // Atomic store X
            }
            0
        });

        lt.add(|mut eg: Environment| {
            // After loop, Y reads value written by X
            while eg.b.load(Ordering::Relaxed) == 0 {}

            // This should always see the store
            eg.a.load(Ordering::Relaxed)
        });

        lt.run()
    }

    assert!(run_until(|| inner(true), vec![vec![0, 1]]));
    assert!(run_until(|| inner(false), vec![vec![0, 1]]));
}

/*
Atomic-fence synchronization

An atomic release operation X in thread A synchronizes-with an acquire fence F in thread B, if

* there exists an atomic read Y (with any memory order)
* Y reads the value written by X (or by the release sequence headed by X)
* Y is sequenced-before F in thread B

In this case, all non-atomic and relaxed atomic stores that are sequenced-before X in thread A will happen-before all non-atomic and relaxed atomic loads from the same locations made in thread B after F.
*/

#[test]
fn test_atomic_fence() {
    fn inner(use_release: bool) -> Vec<usize> {
        let mut lt = LogTest::default();

        lt.add(move |mut eg: Environment| {
            eg.a.store(1, Ordering::Relaxed); // Target

            // Note X swaps between C and B here. The synchronizing load can be the X, or a
            // relaxed store released by X
            if use_release {
                // Optionally model release sequence headed by X
                eg.b.store(1, Ordering::Relaxed);
                eg.c.store(1, Ordering::Release); // Atomic store X
            } else {
                eg.b.store(1, Ordering::Release); // Atomic store X
            }
            0
        });

        lt.add(|mut eg: Environment| {
            while eg.b.load(Ordering::Relaxed) == 0 {} // Atomic Read Y
            eg.fence(Ordering::Acquire); // Fence F

            // This should always see the store
            eg.a.load(Ordering::Relaxed)
        });

        lt.run()
    }

    assert!(run_until(|| inner(true), vec![vec![0, 1]]));
    assert!(run_until(|| inner(false), vec![vec![0, 1]]));
}

/*
Fence-fence synchronization

A release fence FA in thread A synchronizes-with an acquire fence FB in thread B, if

* There exists an atomic object M,
* There exists an atomic write X (with any memory order) that modifies M in thread A
* FA is sequenced-before X in thread A
* There exists an atomic read Y (with any memory order) in thread B
* Y reads the value written by X (or the value would be written by release sequence headed by X if X were a release operation)
* Y is sequenced-before FB in thread B
* In this case, all non-atomic and relaxed atomic stores that are sequenced-before FA in thread A will happen-before all non-atomic and relaxed atomic loads from the same locations made in thread B after FB
*/

#[test]
fn test_fence_fence() {
    fn inner(use_release: bool) -> Vec<usize> {
        let mut lt = LogTest::default();

        lt.add(move |mut eg: Environment| {
            eg.a.store(1, Ordering::Relaxed); // Target
            eg.fence(Ordering::Release); // Fence FA

            // Note X swaps between C and B here. The synchronizing load can be the X, or a
            // relaxed store released by X
            if use_release {
                // Optionally model release sequence headed by X
                eg.b.store(1, Ordering::Relaxed);
                eg.c.store(1, Ordering::Release); // Atomic store X
            } else {
                eg.b.store(1, Ordering::Release); // Atomic store X
            }
            0
        });

        lt.add(|mut eg: Environment| {
            while eg.b.load(Ordering::Relaxed) == 0 {} // Atomic Read Y
            eg.fence(Ordering::Acquire); // Fence FB

            // This should always see the store
            eg.a.load(Ordering::Relaxed)
        });

        lt.run()
    }

    assert!(run_until(|| inner(true), vec![vec![0, 1]]));
    assert!(run_until(|| inner(false), vec![vec![0, 1]]));
}
