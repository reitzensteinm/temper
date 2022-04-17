use crate::common::harness::{Environment, LogTest};
use crate::common::utils::{permutations, run_until};
use std::sync::atomic::Ordering;

mod common;

// https://en.cppreference.com/w/cpp/atomic/memory_order
// Atomic synchronization

#[test]
fn explanation_relaxed_ordering_impossible() {
    fn inner() -> Vec<usize> {
        let mut lt = LogTest::default();

        lt.add(move |mut eg: Environment| {
            let v = eg.b.load(Ordering::Relaxed);
            eg.a.store(v, Ordering::Relaxed);
            v
        });

        lt.add(move |mut eg: Environment| {
            let v = eg.a.load(Ordering::Relaxed);
            eg.b.store(42, Ordering::Relaxed);
            v
        });

        lt.run()
    }

    // a = b = 42 is valid here.
    // memlog *cannot* simulate this
    assert!(run_until(inner, vec![vec![0, 0], vec![42, 0]]));
}

// This tests "out of thin air values", where 42 is locked behind a circular dependency
// It's not even remotely possible with memlog, but listed here for completion's sake
#[test]
fn explanation_world_not_insane() {
    fn inner() -> Vec<usize> {
        let mut lt = LogTest::default();

        lt.add(move |mut eg: Environment| {
            let v = eg.a.load(Ordering::Relaxed);
            if v == 42 {
                eg.b.store(v, Ordering::Relaxed);
            }
            v
        });

        lt.add(move |mut eg: Environment| {
            let v = eg.b.load(Ordering::Relaxed);
            if v == 42 {
                eg.a.store(v, Ordering::Relaxed);
            }
            v
        });

        lt.run()
    }

    assert!(run_until(inner, vec![vec![0, 0]]));
}

// Incrementing counters test
#[test]
fn explanation_relaxed_increment() {
    let mut lt = LogTest::default();

    lt.add(move |mut eg: Environment| {
        for _ in 0..50 {
            // Todo: Fetch update never fails!
            eg.a.fetch_update(|v| Some(v + 1), Ordering::Relaxed)
                .unwrap();
        }
        eg.a.load(Ordering::Relaxed)
    });

    lt.add(move |mut eg: Environment| {
        for _ in 0..50 {
            // Todo: Fetch update never fails!
            eg.a.fetch_update(|v| Some(v + 1), Ordering::Relaxed)
                .unwrap();
        }
        eg.a.load(Ordering::Relaxed)
    });

    let res = lt.run();

    // At least one thread should see the final value
    assert_eq!(res[0].max(res[1]), 100);
}

// Release Acquire
#[test]
fn release_acquire_two_threads() {
    fn inner() -> Vec<usize> {
        let mut lt = LogTest::default();

        lt.add(move |mut eg: Environment| {
            // Todo: Convert this to non atomic type
            eg.a.store(1, Ordering::Relaxed);
            eg.b.store(1, Ordering::Release);
            0
        });

        lt.add(move |mut eg: Environment| {
            while eg.b.load(Ordering::Acquire) == 0 {}
            eg.a.load(Ordering::Relaxed)
        });

        lt.run()
    }

    assert!(run_until(inner, vec![vec![0, 1]]));
}

#[test]
fn release_acquire_three_threads() {
    fn inner() -> Vec<usize> {
        let mut lt = LogTest::default();

        lt.add(move |mut eg: Environment| {
            // Todo: Convert this to non atomic type
            eg.a.store(1, Ordering::Relaxed);
            eg.b.store(1, Ordering::Release);
            0
        });

        lt.add(move |mut eg: Environment| {
            // Any RMW continues the release chain
            while !eg.b.exchange_weak(1, 2, Ordering::Relaxed) {}
            // Continue the release chain without necessarily seeing the store to a
            eg.a.load(Ordering::Relaxed)
        });

        lt.add(move |mut eg: Environment| {
            while eg.b.load(Ordering::Acquire) < 2 {}
            eg.a.load(Ordering::Relaxed)
        });

        lt.run()
    }

    assert!(run_until(inner, vec![vec![0, 0, 1], vec![0, 1, 1]]));
}

// Sequential Consistency

/*
Each memory_order_seq_cst operation B that loads from atomic variable M, observes one of the following:
 * the result of the last operation A that modified M, which appears before B in the single total order
 * OR, if there was such an A, B may observe the result of some modification on M that is not memory_order_seq_cst and does not happen-before A
 * OR, if there wasn't such an A, B may observe the result of some unrelated modification of M that is not memory_order_seq_cst
*/

#[test]
fn seq_cst_basic() {
    fn inner() -> Vec<usize> {
        let mut lt = LogTest::default();

        lt.add(move |mut eg: Environment| {
            eg.a.store(1, Ordering::Relaxed); // Unrelated modification of M that is not memory_order_seq_cst
            eg.a.store(2, Ordering::SeqCst); // Operation A
            eg.a.store(3, Ordering::Relaxed); // Does not happen before A
            0
        });

        lt.add(move |mut eg: Environment| {
            eg.a.load(Ordering::SeqCst) // Operation B
        });

        lt.run_sequential()
    }

    assert!(run_until(inner, permutations(vec![vec![0], vec![2, 3]])));
}

/* If there was a memory_order_seq_cst std::atomic_thread_fence operation X sequenced-before B, then B observes one of the following:

* the last memory_order_seq_cst modification of M that appears before X in the single total order
* some unrelated modification of M that appears later in M's modification order
*/

#[test]
fn seq_cst_fence() {
    fn inner() -> Vec<usize> {
        let mut lt = LogTest::default();

        lt.add(move |mut eg: Environment| {
            eg.a.store(1, Ordering::Relaxed); // Unrelated modification of M that is not memory_order_seq_cst
            eg.a.store(2, Ordering::SeqCst); // Operation A
            eg.a.store(3, Ordering::Relaxed); // Does not happen before A

            0
        });

        lt.add(move |mut eg: Environment| {
            eg.fence(Ordering::SeqCst);
            eg.a.load(Ordering::Relaxed) // Operation B
        });

        lt.run_sequential()
    }

    assert!(run_until(inner, permutations(vec![vec![0], vec![2, 3]])));
}

// Todo: More of the standard

/*
For a pair of atomic operations on M called A and B, where A writes and B reads M's value,
if there are two memory_order_seq_cst std::atomic_thread_fences X and Y,
and if A is sequenced-before X, Y is sequenced-before B,
and X appears before Y in the Single Total Order, then B observes either:
 * the effect of A
 * some unrelated modification of M that appears after A in M's modification order
*/

#[test]
fn seq_cst_two_fence() {
    fn inner() -> Vec<usize> {
        let mut lt = LogTest::default();

        lt.add(move |mut eg: Environment| {
            eg.a.store(1, Ordering::Relaxed); // Unrelated modification of M that is not memory_order_seq_cst
            eg.a.store(2, Ordering::Relaxed); // Operation A
            eg.fence(Ordering::SeqCst);
            eg.a.store(3, Ordering::Relaxed); // Does not happen before A

            0
        });

        lt.add(move |mut eg: Environment| {
            eg.fence(Ordering::SeqCst);
            eg.a.load(Ordering::Relaxed) // Operation B
        });

        lt.run_sequential()
    }

    assert!(run_until(inner, permutations(vec![vec![0], vec![2, 3]])));
}

/* The single total order might not be consistent with happens-before.
This allows more efficient implementation of memory_order_acquire and memory_order_release on some CPUs.
It can produce surprising results when memory_order_acquire and memory_order_release are mixed with memory_order_seq_cst.

For example, with x and y initially zero,
is allowed to produce r1 == 1 && r2 == 3 && r3 == 0,
(Note: this checks stated outcome is possible, I have not verified other results I assume to be valid)
*/

#[test]
fn sto_happens_before() {
    fn inner() -> Vec<usize> {
        let mut lt = LogTest::default();

        lt.add(move |mut eg: Environment| {
            eg.a.store(1, Ordering::SeqCst);
            eg.b.store(1, Ordering::Release);
            0
        });

        lt.add(move |mut eg: Environment| {
            let a = loop {
                if let Ok(v) = eg.b.fetch_update(|v| Some(v + 1), Ordering::SeqCst) {
                    break v;
                }
            };

            let b = eg.b.load(Ordering::Relaxed);

            a + b
        });

        lt.add(move |mut eg: Environment| {
            eg.b.store(3, Ordering::SeqCst);
            eg.a.load(Ordering::SeqCst)
        });

        lt.run()
    }

    assert!(run_until(
        inner,
        permutations(vec![vec![0], vec![1, 3, 4, 7], vec![0, 1]])
    ));
}

/* This example demonstrates a situation where sequential ordering is necessary.
Any other ordering may trigger the assert because it would be possible for the
threads c and d to observe changes to the atomics x and y in opposite order. */

#[test]
fn test_seq_cst() {
    fn inner() -> Vec<usize> {
        let mut lt = LogTest::default();

        lt.add(move |mut eg: Environment| {
            eg.a.store(1, Ordering::SeqCst);
            0
        });

        lt.add(move |mut eg: Environment| {
            eg.b.store(1, Ordering::SeqCst);
            0
        });

        lt.add(move |mut eg: Environment| {
            while eg.a.load(Ordering::SeqCst) == 0 {}
            eg.b.load(Ordering::SeqCst)
        });

        lt.add(move |mut eg: Environment| {
            while eg.b.load(Ordering::SeqCst) == 0 {}
            eg.a.load(Ordering::SeqCst)
        });

        lt.run()
    }

    assert!(run_until(
        inner,
        vec![vec![0, 0, 0, 1], vec![0, 0, 1, 0], vec![0, 0, 1, 1]]
    ));
}
