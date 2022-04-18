use crate::common::harness::{Environment, LogTest};
use crate::common::utils::{permutations, run_until};
use std::sync::atomic::Ordering;

mod common;

/* Release Chain Testing
There are subtle edge cases here. There are two simultaneous guarantees:
 * The ordering of the exchange is equivalent to the same load/store pair, with AcqRel mapping to Acquire / Release
 * If the exchange is reading from a Release store, and another thread loads the result with Acquire, the release
   chain is makes Thread 3 synchronize with Thread 1. If the exchange is Relaxed, this does not make Thread 2
   synchronize with Thread 1, or Thread 3 synchronize with Thread 1.
If you're looking to find bugs in this project, this is not a bad place.
 */

#[test]
fn release_acquire_three_threads() {
    fn inner(first_order: Ordering, second_order: Ordering) -> Vec<usize> {
        let mut lt = LogTest::default();

        lt.add(move |mut eg: Environment| {
            eg.a.store(1, Ordering::Relaxed);
            eg.b.store(1, first_order);
            0
        });

        lt.add(move |mut eg: Environment| {
            // Any RMW continues the release chain
            eg.c.store(10, Ordering::Relaxed);
            while !eg.b.exchange_weak(1, 2, second_order) {}
            // Continue the release chain without necessarily seeing the store to a
            eg.a.load(Ordering::Relaxed)
        });

        lt.add(move |mut eg: Environment| {
            while eg.b.load(Ordering::Acquire) < 2 {}
            eg.a.load(Ordering::Relaxed) + eg.c.load(Ordering::Relaxed)
        });

        lt.run()
    }

    fn check(first_order: Ordering, second_order: Ordering, vals: Vec<Vec<usize>>) {
        assert!(run_until(
            || inner(first_order, second_order),
            permutations(vals)
        ));
    }

    // Regardless of second thread ordering, no release on the first thread means all bets are off
    check(
        Ordering::Relaxed,
        Ordering::Relaxed,
        vec![vec![0], vec![0, 1], vec![0, 1, 10, 11]],
    );

    // Release on the exchange means that thread 3 sees thread 2's store, but not necessary that of 1
    check(
        Ordering::Relaxed,
        Ordering::Release,
        vec![vec![0], vec![0, 1], vec![10, 11]],
    );

    // Thread 2 may not see thread 1's write
    // Via a release chain, thread 3 always sees it, but may not see thread 2's write.
    // Would this ever happen on any platform and compiler combo?
    check(
        Ordering::Release,
        Ordering::Relaxed,
        vec![vec![0], vec![0, 1], vec![1, 11]],
    );

    // Release on both means thread 3 sees all stores, but thread 2 still doesn't necessarily see 1
    check(
        Ordering::Release,
        Ordering::Release,
        vec![vec![0], vec![0, 1], vec![11]],
    );

    // Thread 2 sees thread 1's store, as does thread 3 via the release chain
    // Thread 3 may not see thread 2's store
    check(
        Ordering::Release,
        Ordering::Acquire,
        vec![vec![0], vec![1], vec![1, 11]],
    );

    // All threads should see all stores
    check(
        Ordering::Release,
        Ordering::AcqRel,
        vec![vec![0], vec![1], vec![11]],
    );

    // All threads should see all stores
    check(
        Ordering::Release,
        Ordering::SeqCst,
        vec![vec![0], vec![1], vec![11]],
    );
}
