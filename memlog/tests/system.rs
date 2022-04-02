use crate::common::harness::{Environment, LogTest};
use crate::common::utils::run_until;
use std::sync::atomic::Ordering;

mod common;

#[test]
fn test_harness() {
    let mut lt = LogTest::default();

    const ITERS: usize = 100;

    lt.add(|mut eg: Environment| {
        let mut last = None;
        for _ in 0..=ITERS {
            let l = eg.a.load(Ordering::Relaxed);
            if let Some(v) = last {
                assert!(v <= l);
            }
            last = Some(l);
        }
    });

    lt.add(|mut eg: Environment| {
        for x in 0..=ITERS {
            eg.a.store(x, Ordering::Relaxed);
        }
    });

    lt.run();
}

#[test]
fn test_intel_failure() {
    fn intel_failure_inner() -> Vec<usize> {
        let mut lt = LogTest::default();

        lt.add(|mut eg: Environment| {
            eg.a.store(1, Ordering::Relaxed);
            eg.b.load(Ordering::Relaxed)
        });

        lt.add(|mut eg: Environment| {
            eg.b.store(1, Ordering::Relaxed);
            eg.a.load(Ordering::Relaxed)
        });

        lt.run()
    }

    assert!(run_until(
        intel_failure_inner,
        vec![vec![0, 0], vec![0, 1], vec![1, 0], vec![1, 1]]
    ));
}
