use crate::common::harness::{Environment, LogTest};
use crate::common::utils::run_until;
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
