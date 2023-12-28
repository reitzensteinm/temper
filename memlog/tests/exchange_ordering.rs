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
    fn inner(
        first_order: Ordering,
        second_order: Ordering,
        second_order_failure: Ordering,
    ) -> Vec<usize> {
        let mut lt = LogTest::default();

        lt.add(move |mut eg: Environment| {
            eg.a.store(1, Ordering::Relaxed);
            eg.b.store(1, first_order);
            0
        });

        lt.add(move |mut eg: Environment| {
            // Any RMW continues the release chain
            eg.c.store(10, Ordering::Relaxed);
            while eg
                .b
                .exchange_weak(1, 2, second_order, second_order_failure)
                .is_err()
            {}
            // Continue the release chain without necessarily seeing the store to a
            eg.a.load(Ordering::Relaxed)
        });

        lt.add(move |mut eg: Environment| {
            while eg.b.load(Ordering::Acquire) < 2 {}
            eg.a.load(Ordering::Relaxed) + eg.c.load(Ordering::Relaxed)
        });

        lt.run()
    }

    fn check(
        first_order: Ordering,
        second_order: Ordering,
        second_order_failure: Ordering,
        vals: Vec<Vec<usize>>,
    ) {
        assert!(run_until(
            || inner(first_order, second_order, second_order_failure),
            permutations(vals)
        ));
    }

    // Regardless of second thread ordering, no release on the first thread means all bets are off
    check(
        Ordering::Relaxed,
        Ordering::Relaxed,
        Ordering::Relaxed,
        vec![vec![0], vec![0, 1], vec![0, 1, 10, 11]],
    );

    // Release on the exchange means that thread 3 sees thread 2's store, but not necessary that of 1
    check(
        Ordering::Relaxed,
        Ordering::Release,
        Ordering::Relaxed,
        vec![vec![0], vec![0, 1], vec![10, 11]],
    );

    // Thread 2 may not see thread 1's write
    // Via a release chain, thread 3 always sees it, but may not see thread 2's write.
    // Would this ever happen on any platform and compiler combo?
    check(
        Ordering::Release,
        Ordering::Relaxed,
        Ordering::Relaxed,
        vec![vec![0], vec![0, 1], vec![1, 11]],
    );

    // Release on both means thread 3 sees all stores, but thread 2 still doesn't necessarily see 1
    check(
        Ordering::Release,
        Ordering::Release,
        Ordering::Relaxed,
        vec![vec![0], vec![0, 1], vec![11]],
    );

    // Thread 2 sees thread 1's store, as does thread 3 via the release chain
    // Thread 3 may not see thread 2's store
    check(
        Ordering::Release,
        Ordering::Acquire,
        Ordering::Acquire,
        vec![vec![0], vec![1], vec![1, 11]],
    );

    // All threads should see all stores
    check(
        Ordering::Release,
        Ordering::AcqRel,
        Ordering::Acquire,
        vec![vec![0], vec![1], vec![11]],
    );

    // All threads should see all stores
    check(
        Ordering::Release,
        Ordering::SeqCst,
        Ordering::SeqCst,
        vec![vec![0], vec![1], vec![11]],
    );
}

/* SeqLock
Based on blog post:
https://puzpuzpuz.dev/seqlock-based-atomic-memory-snapshots

SeqLocks enable atomic operations for multiple read and write threads.
 * Write access is protected by CAS to prevent multiple threads entering the critical section
 * Read access is optimistic, writes versioned. Readers ensure version did not change while reading.
 */

#[test]
fn test_seqlock() {
    fn intel_failure_inner() -> Vec<usize> {
        let mut lt = LogTest::default();

        let write_fn = |mut eg: Environment| loop {
            let version = eg.a.load(Ordering::Acquire);
            if version & 1 == 1 {
                continue;
            }

            if eg
                .a
                .exchange_weak(version, version + 1, Ordering::Relaxed, Ordering::Relaxed)
                .is_err()
            {
                continue;
            }

            eg.fence(Ordering::Release);

            let old_b = eg.b.load(Ordering::Relaxed);
            let old_c = eg.c.load(Ordering::Relaxed);

            eg.b.store(old_b + 1, Ordering::Relaxed);
            eg.c.store(old_c + 1, Ordering::Relaxed);

            eg.a.store(version + 2, Ordering::Release);
            return 0;
        };

        lt.add(write_fn);
        lt.add(write_fn);

        lt.add(|mut eg: Environment| loop {
            let version = eg.a.load(Ordering::Acquire);
            if version & 1 == 1 {
                continue;
            }

            let b = eg.b.load(Ordering::Relaxed);
            let c = eg.c.load(Ordering::Relaxed);

            eg.fence(Ordering::Acquire);

            let current_version = eg.a.load(Ordering::Relaxed);

            if current_version == version {
                return b + c;
            }
        });

        lt.run()
    }

    // Read should either see 0, 1 or 2 atomic writes
    // Reading a partial write will result in an odd number
    assert!(run_until(
        intel_failure_inner,
        vec![vec![0, 0, 0], vec![0, 0, 2], vec![0, 0, 4]]
    ));
}

#[test]
fn acquire_chain_test() {
    enum AcquireChainStrategy {
        WeakExchangeFence,
        AcqRelExchange,
        StoreRelease,
    }

    fn tiny_test(strategy: AcquireChainStrategy) -> Vec<usize> {
        let mut lt = LogTest::default();

        lt.add(move |mut eg: Environment| {
            match strategy {
                AcquireChainStrategy::WeakExchangeFence => {
                    // Fence is required for correctness
                    while eg
                        .a
                        .exchange_weak(0, 1, Ordering::Relaxed, Ordering::Relaxed)
                        .is_err()
                    {}
                    eg.fence(Ordering::Release);
                }
                AcquireChainStrategy::AcqRelExchange => {
                    // AcqRel only guarantees Acquire on load, Release on store.
                    // Relaxed stores below are _not_ guaranteed to not be reordered before this store
                    // See https://en.cppreference.com/w/cpp/atomic/memory_order - memory_order_acq_rel
                    while eg
                        .a
                        .exchange_weak(0, 1, Ordering::AcqRel, Ordering::Acquire)
                        .is_err()
                    {}
                }
                AcquireChainStrategy::StoreRelease => {
                    // Isn't even an edge case. This should obviously not work.
                    // Exists as a regression test against a memlog bug where relaxed stores
                    // and loads combined with an Acquire fence were erroneously creating a
                    // release chain and providing additional guarantees.
                    eg.a.store(1, Ordering::Release);
                }
            }

            eg.b.store(1, Ordering::Relaxed);
            eg.b.store(2, Ordering::Relaxed);

            eg.a.store(2, Ordering::Release);

            0
        });

        lt.add(|mut eg: Environment| {
            let a = eg.a.load(Ordering::Acquire);
            let b = eg.b.load(Ordering::Relaxed);

            eg.fence(Ordering::Acquire);

            let a_2 = eg.a.load(Ordering::Relaxed);

            if a_2 == a && a != 1 {
                b
            } else {
                0
            }
        });

        lt.run()
    }

    // AcqRel does not provide the required guarantees according to the C++ memory model
    // If an implementation generates an atomic CAS operation, it will incidentally work
    // However, something like Load-Link/Store-Conditional could generate different barriers
    // for load and store, meaning that a write may be reordered between them.
    // The standard only guarantees Acquire on load, Release on store.
    assert!(run_until(
        || tiny_test(AcquireChainStrategy::AcqRelExchange),
        vec![vec![0, 0], vec![0, 1], vec![0, 2]]
    ));

    // Regression test against previous release chain/fence bug
    assert!(run_until(
        || tiny_test(AcquireChainStrategy::StoreRelease),
        vec![vec![0, 0], vec![0, 1], vec![0, 2]]
    ));

    // Should work correctly - no partial writes observed.
    assert!(run_until(
        || tiny_test(AcquireChainStrategy::WeakExchangeFence),
        vec![vec![0, 0], vec![0, 2]]
    ));
}
