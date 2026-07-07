use crate::common::harness::{Environment, LogTest};
use crate::common::utils::run_until;
use std::sync::atomic::Ordering;

mod common;

/*
Release sequences broke at the second relaxed RMW: T3 could read flag = 3 yet
still observe data = 0.

    T0: data.store(1, Relaxed)
        flag.store(1, Release)
    T1: flag.compare_exchange_weak(1, 2, Relaxed, Relaxed)
    T2: flag.compare_exchange_weak(2, 3, Relaxed, Relaxed)
    T3: while flag.load(Acquire) < 3 {}
        data.load(Relaxed) // 1
*/

#[test]
fn test_release_sequence_extends_through_two_relaxed_rmws() {
    fn inner() -> Vec<usize> {
        let mut lt = LogTest::default();

        lt.add(|mut eg: Environment| {
            eg.a.store(1, Ordering::Relaxed);
            eg.b.store(1, Ordering::Release);
            0
        });

        lt.add(|mut eg: Environment| {
            while eg
                .b
                .exchange_weak(1, 2, Ordering::Relaxed, Ordering::Relaxed)
                .is_err()
            {}
            0
        });

        lt.add(|mut eg: Environment| {
            while eg
                .b
                .exchange_weak(2, 3, Ordering::Relaxed, Ordering::Relaxed)
                .is_err()
            {}
            0
        });

        lt.add(|mut eg: Environment| {
            while eg.b.load(Ordering::Acquire) < 3 {}
            eg.a.load(Ordering::Relaxed)
        });

        lt.run()
    }

    assert!(run_until(inner, vec![vec![0, 0, 0, 1]]));
}

/*
An RMW dropped the fence payload of the store it read, breaking the
hypothetical release sequence headed by a fence-published store: T2 could
read flag = 2 yet still observe data = 0.

    T0: data.store(1, Relaxed)
        fence(Release)
        flag.store(1, Relaxed)
    T1: flag.compare_exchange_weak(1, 2, Relaxed, Relaxed)
    T2: while flag.load(Acquire) < 2 {}
        data.load(Relaxed) // 1

The fenceless variant checks the weak outcome remains possible.
*/

#[test]
fn test_fence_atomic_rmw_chain() {
    fn inner(fence: bool) -> Vec<usize> {
        let mut lt = LogTest::default();

        lt.add(move |mut eg: Environment| {
            eg.a.store(1, Ordering::Relaxed); // Target

            if fence {
                eg.fence(Ordering::Release); // F sequenced before X in thread A
            }

            eg.b.store(1, Ordering::Relaxed); // Atomic store X
            0
        });

        lt.add(|mut eg: Environment| {
            // RMW in the hypothetical release sequence headed by X
            while eg
                .b
                .exchange_weak(1, 2, Ordering::Relaxed, Ordering::Relaxed)
                .is_err()
            {}
            0
        });

        lt.add(|mut eg: Environment| {
            // Y reads the value written by the release sequence headed by X
            while eg.b.load(Ordering::Acquire) < 2 {}

            // This should always see the store
            eg.a.load(Ordering::Relaxed)
        });

        lt.run()
    }

    // Assert success when the fence is present
    assert!(run_until(|| inner(true), vec![vec![0, 0, 1]]));

    // Assert failure when the fence is missing
    assert!(run_until(
        || inner(false),
        vec![vec![0, 0, 0], vec![0, 0, 1]]
    ));
}
