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
fn test_same_thread_reads() {
    let mut lt = LogTest::default();
    const ITERS: usize = 100;

    lt.add(|mut eg: Environment| {
        for x in 0..=ITERS {
            eg.a.store(x, Ordering::Relaxed);
            assert_eq!(x, eg.a.load(Ordering::Relaxed));
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

#[test]
fn test_acq_rel() {
    fn acq_rel() -> Vec<isize> {
        let mut lt = LogTest::default();

        lt.add(|mut eg: Environment| {
            eg.b.store(1, Ordering::Relaxed);
            eg.a.store(1, Ordering::Release);
            0
        });

        lt.add(|mut eg: Environment| {
            let a = eg.a.load(Ordering::Acquire);
            let b = eg.b.load(Ordering::Relaxed);
            // The acquire on A should synchronize with the other thread's release on A
            // If the value of (b-a) is negative, this thread has seen the write to A but not B
            (b as isize) - (a as isize)
        });

        lt.run()
    }

    assert!(run_until(acq_rel, vec![vec![0, 0], vec![0, 1]]));
}

#[test]
// With Release/Acquire ordering, threads 3 and 4 can perceive the writes
// to threads 1 and 2 in different orders
fn test_orders() {
    fn orders() -> Vec<usize> {
        let mut lt = LogTest::default();

        lt.add(|mut eg: Environment| {
            eg.a.store(1, Ordering::Release);
            0
        });

        lt.add(|mut eg: Environment| {
            eg.b.store(1, Ordering::Release);
            0
        });

        lt.add(|mut eg: Environment| {
            while eg.a.load(Ordering::Acquire) == 0 {}
            eg.b.load(Ordering::Acquire)
        });

        lt.add(|mut eg: Environment| {
            while eg.b.load(Ordering::Acquire) == 0 {}
            eg.a.load(Ordering::Acquire)
        });

        lt.run()
    }

    assert!(run_until(
        orders,
        vec![
            vec![0, 0, 0, 0],
            vec![0, 0, 1, 0],
            vec![0, 0, 0, 1],
            vec![0, 0, 1, 1]
        ]
    ));
}
