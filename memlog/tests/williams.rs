/*
Tests the model against examples that demonstrate the behaviour described in C++ Concurrency in
Action by Anthony Williams. The implementations differ significantly.
*/
use crate::common::harness::{Environment, LogTest};
use crate::common::utils::run_until;
use std::sync::atomic::Ordering;

mod common;

// Listing 5.4
#[test]
fn test_5_4() {
    fn inner() -> Vec<usize> {
        let mut lt = LogTest::default();

        lt.add(|mut eg: Environment| {
            eg.a.store(1, Ordering::SeqCst);
            0
        });

        lt.add(|mut eg: Environment| {
            eg.b.store(1, Ordering::SeqCst);
            0
        });

        lt.add(|mut eg: Environment| {
            while eg.a.load(Ordering::SeqCst) == 0 {}
            eg.b.load(Ordering::SeqCst)
        });

        lt.add(|mut eg: Environment| {
            while eg.b.load(Ordering::SeqCst) == 0 {}
            eg.a.load(Ordering::SeqCst)
        });

        lt.run()
    }

    // The two reader threads should always see the same
    assert!(run_until(
        inner,
        vec![vec![0, 0, 1, 1], vec![0, 0, 0, 1], vec![0, 0, 1, 0]]
    ));
}
