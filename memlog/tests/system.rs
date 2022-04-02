use crate::common::harness::{Environment, LogTest};
use std::collections::HashSet;
use std::sync::atomic::Ordering;

mod common;

#[test]
fn test_harness() {
    let lt = LogTest::default();

    const ITERS: usize = 5;

    let fa = |mut eg: Environment| {
        let mut last = None;
        for _ in 0..=ITERS {
            let l = eg.a.load(Ordering::Relaxed);
            if let Some(v) = last {
                assert!(v <= l);
            }
            last = Some(l);
        }
    };
    let fb = |mut eg: Environment| {
        for x in 0..=ITERS {
            eg.a.store(x, Ordering::Relaxed);
        }
    };

    let fns: Vec<Box<dyn FnMut(Environment) + Send>> = vec![Box::new(fa), Box::new(fb)];

    lt.run(fns);
}

#[test]
fn test_intel_failure() {
    let mut results = HashSet::new();
    for _ in 0..100 {
        let lt = LogTest::default();

        let fa = |mut eg: Environment| {
            eg.a.store(1, Ordering::Relaxed);
            eg.b.load(Ordering::Relaxed)
        };
        let fb = |mut eg: Environment| {
            eg.b.store(1, Ordering::Relaxed);
            eg.a.load(Ordering::Relaxed)
        };

        let fns: Vec<Box<dyn FnMut(Environment) -> usize + Send>> =
            vec![Box::new(fa), Box::new(fb)];

        results.insert(lt.run(fns));
    }

    let mut expected = HashSet::new();

    for v in vec![vec![0, 0], vec![0, 1], vec![1, 0], vec![1, 1]] {
        expected.insert(v);
    }

    assert_eq!(results, expected);
}
