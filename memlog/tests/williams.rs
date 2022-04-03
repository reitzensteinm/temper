/*
Tests the model against examples that demonstrate the behaviour described in C++ Concurrency in
Action by Anthony Williams. The implementations differ significantly.
*/
use crate::common::harness::{Environment, LogTest};
use crate::common::utils::run_until;
use std::sync::atomic::Ordering;

mod common;

// Listing 5.4
// Tests to ensure a global ordering for SeqCst operations
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

    // 0,0 should not be possible, as it would imply the reader threads experienced different orders
    assert!(run_until(
        inner,
        vec![vec![0, 0, 1, 1], vec![0, 0, 0, 1], vec![0, 0, 1, 0]]
    ));
}

// Listing 5.5
// Relaxed stores can be perceived in either order
#[test]
fn test_5_5() {
    fn inner() -> Vec<usize> {
        let mut lt = LogTest::default();

        lt.add(|mut eg: Environment| {
            eg.a.store(1, Ordering::Relaxed);
            eg.b.store(1, Ordering::Relaxed);
            0
        });

        lt.add(|mut eg: Environment| {
            while eg.b.load(Ordering::Relaxed) == 0 {}
            eg.a.load(Ordering::Relaxed)
        });

        lt.run()
    }

    assert!(run_until(inner, vec![vec![0, 0], vec![0, 1]]));
}

// Listing 5.6
// Threads should immediately see their own writes
// Threads should only ever see forward progress from other threads
#[test]
fn test_5_6() {
    let mut lt = LogTest::default();

    lt.add(|mut eg: Environment| {
        let mut last = None;
        for x in 0..5 {
            eg.a.store(x, Ordering::Relaxed);
            let r = eg.b.load(Ordering::Relaxed);

            if let Some(l) = last {
                assert!(r >= l);
            }

            last = Some(r);

            assert_eq!(eg.a.load(Ordering::Relaxed), x);
        }
    });

    lt.add(|mut eg: Environment| {
        let mut last = None;
        for x in 0..5 {
            eg.b.store(x, Ordering::Relaxed);
            let r = eg.a.load(Ordering::Relaxed);

            if let Some(l) = last {
                assert!(r >= l);
            }

            last = Some(r);

            assert_eq!(eg.b.load(Ordering::Relaxed), x);
        }
    });

    lt.run();
}

// Listing 5.7
// With Release/Acquire ordering, threads 3 and 4 can perceive the writes
// to threads 1 and 2 in different orders, the 0,0 case
#[test]
fn test_5_7() {
    fn inner() -> Vec<usize> {
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
        inner,
        vec![
            vec![0, 0, 0, 0],
            vec![0, 0, 1, 0],
            vec![0, 0, 0, 1],
            vec![0, 0, 1, 1]
        ]
    ));
}

// Listing 5.8
// Acquire and Release synchronizing on A means that if the second thread reads 1 from A,
// it must also read 1 from B
#[test]
fn test_5_8() {
    fn inner() -> Vec<isize> {
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

    assert!(run_until(inner, vec![vec![0, 0], vec![0, 1]]));
}

// Listing 5.9
// Acquire and Release are transitive
#[test]
fn test_5_9() {
    fn inner() -> Vec<usize> {
        let mut lt = LogTest::default();

        lt.add(|mut eg: Environment| {
            eg.a.store(1, Ordering::Relaxed);
            eg.b.store(1, Ordering::Release);
            0
        });

        lt.add(|mut eg: Environment| {
            while eg.b.load(Ordering::Acquire) == 0 {}
            eg.c.store(1, Ordering::Release);
            0
        });

        lt.add(|mut eg: Environment| {
            while eg.c.load(Ordering::Acquire) == 0 {}
            eg.a.load(Ordering::Relaxed)
        });

        lt.run()
    }

    assert!(run_until(inner, vec![vec![0, 0, 1]]));
}

// Todo: Figure out AcqRel semantics for CAS

// Listing 5.10
// Here, we decide to be more strict than the model described in the book. This is in line with
// the Temper design philosophy, where the models can be more strict than those they emulate
// A Relaxed exchange_weak in Thread 2 can break the happens before relationship between
// Thread 1 and Thread 3, even though it should be a NOP unless Thread 1 happens before Thread 2
#[test]
fn test_5_10() {
    fn inner(exchange_order: Ordering) -> Vec<usize> {
        let mut lt = LogTest::default();

        lt.add(|mut eg: Environment| {
            eg.a.store(1, Ordering::Relaxed);
            eg.b.store(1, Ordering::Release);
            0
        });

        lt.add(move |mut eg: Environment| {
            eg.b.exchange_weak(1, 1, exchange_order);
            0
        });

        lt.add(|mut eg: Environment| {
            while eg.b.load(Ordering::Acquire) == 0 {}
            eg.a.load(Ordering::Relaxed)
        });

        lt.run()
    }

    assert!(run_until(|| inner(Ordering::AcqRel), vec![vec![0, 0, 1]]));
    assert!(run_until(
        || inner(Ordering::Relaxed),
        vec![vec![0, 0, 0], vec![0, 0, 1]]
    ));
}
