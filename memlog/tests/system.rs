use crate::common::harness::{Environment, LogTest};
use std::sync::atomic::Ordering;

mod common;

#[test]
fn test_harness() {
    let lt = LogTest::default();

    const ITERS: usize = 100;

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
