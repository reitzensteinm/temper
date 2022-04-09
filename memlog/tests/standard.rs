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
    fn inner(release_chain: bool, fence: bool) -> Vec<usize> {
        let mut lt = LogTest::default();

        lt.add(move |mut eg: Environment| {
            eg.a.store(1, Ordering::Relaxed); // Target

            // Fence behind conditional to ensure when fence is missing, wrong values result
            if fence {
                eg.fence(Ordering::Release); // F Sequenced before X in thread A
            }

            if release_chain {
                // Atomic store X that would create a release chain to c were it Release
                eg.b.store(1, Ordering::Relaxed);
            } else {
                // Atomic store X that writes directly to c
                eg.c.store(1, Ordering::Relaxed);
            }
            0
        });

        lt.add(move |mut eg: Environment| {
            if release_chain {
                // If modelling a release chain, spin on an intermediate flag
                while eg.b.load(Ordering::Acquire) == 0 {}

                eg.c.store(1, Ordering::Release) // Continue release chain from store X
            }
            0
        });

        lt.add(|mut eg: Environment| {
            // After loop, Y reads value written by X, or a value that would be written by the release chain
            while eg.c.load(Ordering::Acquire) == 0 {}

            // This should always see the store
            eg.a.load(Ordering::Relaxed)
        });

        lt.run()
    }

    // Assert success when fences are present
    assert!(run_until(|| inner(true, true), vec![vec![0, 0, 1]]));
    assert!(run_until(|| inner(false, true), vec![vec![0, 0, 1]]));

    // Assert failure when fences are missing
    assert!(run_until(
        || inner(true, false),
        vec![vec![0, 0, 0], vec![0, 0, 1]]
    ));
    assert!(run_until(
        || inner(false, false),
        vec![vec![0, 0, 0], vec![0, 0, 1]]
    ));
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
    fn inner(release_chain: bool, fence: bool) -> Vec<usize> {
        let mut lt = LogTest::default();

        lt.add(move |mut eg: Environment| {
            eg.a.store(1, Ordering::Relaxed); // Target

            if release_chain {
                // Atomic store X that creates a release chain to c
                eg.b.store(1, Ordering::Release);
            } else {
                // Atomic store X that writes directly to c
                eg.c.store(1, Ordering::Release);
            }
            0
        });

        lt.add(move |mut eg: Environment| {
            if release_chain {
                // If modelling a release chain, spin on an intermediate flag
                while eg.b.load(Ordering::Acquire) == 0 {}

                eg.c.store(1, Ordering::Release) // Continue release chain from store X
            }
            0
        });

        lt.add(move |mut eg: Environment| {
            // Y reads value written by X, or a value that would be written by the release chain
            while eg.c.load(Ordering::Relaxed) == 0 {} // Atomic Read Y

            // Fence behind conditional to ensure when fence is missing, wrong values result
            if fence {
                eg.fence(Ordering::Acquire); // Fence F
            }

            // This should always see the store
            eg.a.load(Ordering::Relaxed)
        });

        lt.run()
    }

    // Assert success when fences are present
    assert!(run_until(|| inner(true, true), vec![vec![0, 0, 1]]));
    assert!(run_until(|| inner(false, true), vec![vec![0, 0, 1]]));

    // Assert failure when fences are missing
    assert!(run_until(
        || inner(true, false),
        vec![vec![0, 0, 0], vec![0, 0, 1]]
    ));
    assert!(run_until(
        || inner(false, false),
        vec![vec![0, 0, 0], vec![0, 0, 1]]
    ));
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
    fn inner(release_chain: bool, failure: usize) -> Vec<usize> {
        let mut lt = LogTest::default();

        lt.add(move |mut eg: Environment| {
            eg.a.store(1, Ordering::Relaxed); // Target

            // Fence behind conditional to ensure when fence is missing, wrong values result
            if failure != 1 {
                eg.fence(Ordering::Release); // Fence FA
            }

            if release_chain {
                // Atomic store X that would create a release chain to c were it release
                eg.b.store(1, Ordering::Relaxed);
            } else {
                // Atomic store X that writes directly to c
                eg.c.store(1, Ordering::Relaxed);
            }
            0
        });

        lt.add(move |mut eg: Environment| {
            if release_chain {
                // If modelling a release chain, spin on an intermediate flag
                while eg.b.load(Ordering::Acquire) == 0 {}

                eg.c.store(1, Ordering::Release) // Atomic store X
            }
            0
        });

        lt.add(move |mut eg: Environment| {
            while eg.c.load(Ordering::Relaxed) == 0 {} // Atomic Read Y

            // Fence behind conditional to ensure when fence is missing, wrong values result
            if failure != 2 {
                eg.fence(Ordering::Acquire); // Fence FB
            }

            // This should always see the store
            eg.a.load(Ordering::Relaxed)
        });

        lt.run()
    }

    // Assert success when fences are present
    assert!(run_until(|| inner(true, 0), vec![vec![0, 0, 1]]));
    assert!(run_until(|| inner(false, 0), vec![vec![0, 0, 1]]));

    // Assert failure when first fence is missing
    assert!(run_until(
        || inner(true, 1),
        vec![vec![0, 0, 0], vec![0, 0, 1]]
    ));
    assert!(run_until(
        || inner(false, 1),
        vec![vec![0, 0, 0], vec![0, 0, 1]]
    ));

    // Assert failure when first second fence is missing
    assert!(run_until(
        || inner(true, 2),
        vec![vec![0, 0, 0], vec![0, 0, 1]]
    ));
    assert!(run_until(
        || inner(false, 2),
        vec![vec![0, 0, 0], vec![0, 0, 1]]
    ));
}

/*
Example under "Notes"

Test ensures that, after seeing the write to C, the second thread can use an Acquire fence to
synchronize with the release fence and see all of the first thread's prior stores
*/
#[test]
fn test_fence_fence_example_a() {
    fn inner(failure: usize) -> Vec<usize> {
        let mut lt = LogTest::default();

        lt.add(move |mut eg: Environment| {
            eg.a.store(1, Ordering::Relaxed); // Relaxed write a
            eg.b.store(1, Ordering::Relaxed); // Relaxed write b

            // Fence behind conditional to ensure when fence is missing, wrong values result
            if failure != 1 {
                eg.fence(Ordering::Release);
            }
            eg.c.store(1, Ordering::Relaxed); // Target
            0
        });

        lt.add(move |mut eg: Environment| {
            if eg.c.load(Ordering::Relaxed) == 1 {
                // Fence behind conditional to ensure when fence is missing, wrong values result
                if failure != 2 {
                    eg.fence(Ordering::Acquire);
                }
                // After a fence, we must see writes to a and b
                eg.a.load(Ordering::Relaxed) + eg.b.load(Ordering::Relaxed)
            } else {
                0
            }
        });

        lt.run()
    }

    // When fences are present, if thread two perceives the write to c, it should see both a & b
    assert!(run_until(|| inner(0), vec![vec![0, 0], vec![0, 2]]));
    // Without first fence, all bets are off
    assert!(run_until(
        || inner(1),
        vec![vec![0, 0], vec![0, 1], vec![0, 2]]
    ));
    // Without second fence, all bets are off
    assert!(run_until(
        || inner(2),
        vec![vec![0, 0], vec![0, 1], vec![0, 2]]
    ));
}

#[test]
fn test_fence_fence_example_b() {
    fn inner(failure: usize) -> Vec<usize> {
        let mut lt = LogTest::default();

        lt.add(move |mut eg: Environment| {
            // Transaction 1
            // Write data to a, bump write pointer c
            eg.a.store(1, Ordering::Relaxed);
            eg.c.store(1, Ordering::Release);

            // Transaction 2
            // Write data to b, bump write pointer c
            eg.b.store(1, Ordering::Relaxed);
            eg.c.store(2, Ordering::Release);
            0
        });

        lt.add(move |mut eg: Environment| {
            // Wait for write pointer to indicate Transaction 1 ready
            while eg.c.load(Ordering::Relaxed) < 1 {}

            // Fence behind conditional to ensure when fence is missing, wrong values result
            if failure != 1 {
                eg.fence(Ordering::Acquire);
            }

            eg.a.load(Ordering::Relaxed)
        });

        lt.add(move |mut eg: Environment| {
            // Wait for write pointer to indicate Transaction 2 ready
            while eg.c.load(Ordering::Relaxed) < 2 {}

            // Fence behind conditional to ensure when fence is missing, wrong values result
            if failure != 2 {
                eg.fence(Ordering::Acquire);
            }

            eg.a.load(Ordering::Relaxed)
        });

        lt.run()
    }

    // When fences are present, if thread two perceives the write to c, it should see both a & b
    assert!(run_until(|| inner(0), vec![vec![0, 1, 1]]));

    // Drop fence from first reader. It is now not guaranteed to perceive the store to a
    assert!(run_until(|| inner(1), vec![vec![0, 0, 1], vec![0, 1, 1]]));
    // Drop fence from second reader. It is now not guaranteed to perceive the store to b
    assert!(run_until(|| inner(2), vec![vec![0, 1, 0], vec![0, 1, 1]]));
}
