use crate::common::harness::{Environment, LogTest};
use memlog::log::MemorySystem;
use std::sync::atomic::Ordering;

mod common;

#[test]
fn test_harness() {
    let lt = LogTest::default();

    let fa = |mut eg: Environment| {
        let mut last = None;
        for x in 0..=5 {
            let l = eg.a.load(Ordering::Relaxed);
            if let Some(v) = last {
                assert!(v <= l);
            }
            last = Some(l);
        }
    };
    let fb = |mut eg: Environment| {
        for x in 0..=5 {
            eg.a.store(x, Ordering::Relaxed);
        }
    };

    let fns: Vec<Box<dyn FnMut(Environment) + Send>> = vec![Box::new(fa), Box::new(fb)];

    lt.run(fns);
}
