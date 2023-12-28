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
            let _ =
                eg.a.exchange_weak(0, 1, Ordering::AcqRel, Ordering::Acquire);
            eg.b.load(Ordering::Relaxed)
        });

        lt.add(|mut eg: Environment| {
            let _ =
                eg.b.exchange_weak(0, 1, Ordering::AcqRel, Ordering::Acquire);
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

// Same example, but in the comments. One half of the comment appears to be malformed, and the
// description doesn't match the code. This from the bottom.
#[test]
fn test_exchange_fence() {
    fn inner() -> Vec<usize> {
        let mut lt = LogTest::default();

        lt.add(|mut eg: Environment| {
            eg.a.store(1, Ordering::Relaxed);
            eg.fence(Ordering::AcqRel);
            eg.b.store(1, Ordering::Relaxed);
            0
        });

        lt.add(|mut eg: Environment| {
            // By perceiving the store to B, which is sequenced after the fence
            while eg.b.load(Ordering::Relaxed) == 0 {}
            // This fence now synchronizes with the release above, and the write to A must be visible
            eg.fence(Ordering::AcqRel);
            eg.a.load(Ordering::Relaxed)
        });

        lt.run()
    }

    assert!(run_until(inner, vec![vec![0, 1]]));
}

// https://stackoverflow.com/questions/71509935/how-does-mixing-relaxed-and-acquire-release-accesses-on-the-same-atomic-variable
#[test]
fn test_broken_release_chain() {
    fn inner() -> Vec<usize> {
        let mut lt = LogTest::default();

        lt.add(|mut eg: Environment| {
            eg.a.store(42, Ordering::Relaxed);
            eg.b.store(1, Ordering::Release);
            0
        });

        lt.add(|mut eg: Environment| {
            if eg.b.load(Ordering::Relaxed) == 1 {
                eg.b.store(2, Ordering::Relaxed);
            }
            0
        });

        lt.add(|mut eg: Environment| {
            let v = eg.b.load(Ordering::Acquire);
            let ov = eg.a.load(Ordering::Relaxed);

            v + ov
        });

        lt.run()
    }

    assert!(run_until(
        inner,
        vec![
            vec![0, 0, 0],
            vec![0, 0, 2], // Broken release chain
            vec![0, 0, 42],
            vec![0, 0, 43],
            vec![0, 0, 44]
        ]
    ));
}

// https://stackoverflow.com/questions/67693687/possible-orderings-with-memory-order-seq-cst-and-memory-order-release

#[test]
fn test_intel_failure_release() {
    fn intel_failure_inner() -> Vec<usize> {
        let mut lt = LogTest::default();

        lt.add(|mut eg: Environment| {
            eg.a.store(1, Ordering::Release);
            eg.b.load(Ordering::SeqCst)
        });

        lt.add(|mut eg: Environment| {
            eg.b.store(1, Ordering::Release);
            eg.a.load(Ordering::SeqCst)
        });

        lt.run()
    }

    assert!(run_until(
        intel_failure_inner,
        vec![vec![0, 0], vec![0, 1], vec![1, 0], vec![1, 1]]
    ));
}
