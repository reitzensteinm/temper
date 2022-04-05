use crate::common::harness::{Environment, LogTest};
use crate::common::utils::run_until;
use std::sync::atomic::Ordering;

mod common;

// https://stackoverflow.com/questions/47520748/c-memory-model-do-seq-cst-loads-synchronize-with-seq-cst-stores
#[test]
fn test_seq_cst_sync() {
    fn inner() -> Vec<usize> {
        let mut lt = LogTest::default();

        lt.add(|mut eg: Environment| {
            eg.a.store(1, Ordering::Relaxed);

            if eg.b.load(Ordering::SeqCst) == 1 {
                eg.c.store(1, Ordering::Relaxed)
            }
            0
        });

        lt.add(|mut eg: Environment| {
            eg.b.store(1, Ordering::SeqCst);

            if eg.c.load(Ordering::Relaxed) == 1 {
                eg.a.load(Ordering::Relaxed)
            } else {
                2
            }
        });

        lt.run()
    }

    // Thread 2 doesn't synchronize with Thread 1, meaning the store to A may not be available in time
    // This defies all reason. I doubt you could replicate it on a real machine. But we simulate it.
    assert!(run_until(inner, vec![vec![0, 2], vec![0, 0], vec![0, 1]]));
}

// https://stackoverflow.com/questions/52606524/what-exact-rules-in-the-c-memory-model-prevent-reordering-before-acquire-opera
#[test]
fn test_exchange() {
    fn inner() -> Vec<usize> {
        let mut lt = LogTest::default();

        lt.add(|mut eg: Environment| {
            eg.a.exchange_weak(0, 1, Ordering::AcqRel);
            eg.b.load(Ordering::Relaxed)
        });

        lt.add(|mut eg: Environment| {
            eg.b.exchange_weak(0, 1, Ordering::AcqRel);
            eg.a.load(Ordering::Relaxed)
        });

        lt.run()
    }

    // The threads do not establish any synchronizes with relationship with each other. All bets are off.
    assert!(run_until(
        inner,
        vec![vec![0, 0], vec![0, 1], vec![1, 0], vec![1, 1]]
    ));
}

// Same example, but in the comments
#[test]
fn test_exchange_fence() {
    fn inner() -> Vec<usize> {
        let mut lt = LogTest::default();

        lt.add(|mut eg: Environment| {
            eg.a.store(1, Ordering::Relaxed);
            eg.fence(Ordering::AcqRel);
            eg.b.load(Ordering::Relaxed)
        });

        lt.add(|mut eg: Environment| {
            eg.b.store(1, Ordering::Relaxed);
            eg.fence(Ordering::AcqRel);
            eg.a.load(Ordering::Relaxed)
        });

        lt.run()
    }

    // The threads do not establish any synchronizes with relationship with each other. All bets are off.
    assert!(run_until(inner, vec![vec![0, 1], vec![1, 0], vec![1, 1]]));
}
