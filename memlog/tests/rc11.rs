use crate::common::harness::{Environment, LogTest};
use std::sync::atomic::Ordering;

mod common;

// RC11: https://plv.mpi-sws.org/scfix/
// Model: https://plv.mpi-sws.org/scfix/rc11.cat
//
// Passing tests document where memlog agrees with RC11. The RC11 SC changes
// that C++20 adopted (IRIW-acq-sc, Z6.U, RWC+syncs, W+RWC) are covered in
// cxx20.rs; this file keeps the RC11-only rules.
//
// In the litmus comments below, paper variables x/y map to eg.a/eg.b.
const ATTEMPTS: usize = 10_000;

fn assert_never_observes<F>(mut inner: F, forbidden: Vec<usize>)
where
    F: FnMut() -> Vec<usize>,
{
    for _ in 0..ATTEMPTS {
        assert_ne!(inner(), forbidden);
    }
}

// RC11's NO-THIN-AIR rule rejects executions with cycles in `sb | rf`.
//
// LB:
//   T0: a = x.load(Relaxed) // 1
//       y.store(1, Relaxed)
//   T1: b = y.load(Relaxed) // 1
//       x.store(1, Relaxed)
//
// Forbidden outcome: [a, b] = [1, 1].
//
// The forbidden [1, 1] result would require:
//   T1 store b --rf--> T0 load b --sb--> T0 store a
//   T0 store a --rf--> T1 load a --sb--> T1 store b
#[test]
fn rc11_forbids_lb() {
    fn inner() -> Vec<usize> {
        let mut lt = LogTest::default();

        lt.add(|mut eg: Environment| {
            let b = eg.b.load(Ordering::Relaxed);
            eg.a.store(1, Ordering::Relaxed);
            b
        });

        lt.add(|mut eg: Environment| {
            let a = eg.a.load(Ordering::Relaxed);
            eg.b.store(1, Ordering::Relaxed);
            a
        });

        lt.run()
    }

    let forbidden = vec![1, 1];

    assert_never_observes(inner, forbidden);
}

// LB+deps is the standard out-of-thin-air example from the RC11 paper. The
// forbidden [1, 1] result would require each relaxed load to read from the
// other thread's later conditional relaxed store.
//
// LB+deps:
//   T0: a = x.load(Relaxed) // 1
//       if a != 0 { y.store(a, Relaxed) }
//   T1: b = y.load(Relaxed) // 1
//       if b != 0 { x.store(b, Relaxed) }
//
// Forbidden outcome: [a, b] = [1, 1].
#[test]
fn rc11_forbids_lb_deps() {
    fn inner() -> Vec<usize> {
        let mut lt = LogTest::default();

        lt.add(|mut eg: Environment| {
            let x = eg.a.load(Ordering::Relaxed);
            if x != 0 {
                eg.b.store(x, Ordering::Relaxed);
            }
            x
        });

        lt.add(|mut eg: Environment| {
            let y = eg.b.load(Ordering::Relaxed);
            if y != 0 {
                eg.a.store(y, Ordering::Relaxed);
            }
            y
        });

        lt.run()
    }

    let forbidden = vec![1, 1];

    assert_never_observes(inner, forbidden);
}

// SB: the classic all-SC store-buffering outcome is forbidden.
//
//   T0: x.store(1, SeqCst)
//       a = y.load(SeqCst) // 0
//   T1: y.store(1, SeqCst)
//       b = x.load(SeqCst) // 0
//
// Forbidden outcome: [a, b] = [0, 0].
#[test]
fn rc11_forbids_sb() {
    fn inner() -> Vec<usize> {
        let mut lt = LogTest::default();

        lt.add(|mut eg: Environment| {
            eg.a.store(1, Ordering::SeqCst);
            eg.b.load(Ordering::SeqCst)
        });

        lt.add(|mut eg: Environment| {
            eg.b.store(1, Ordering::SeqCst);
            eg.a.load(Ordering::SeqCst)
        });

        lt.run()
    }

    let forbidden = vec![0, 0];

    assert_never_observes(inner, forbidden);
}

// 2+2W: the relaxed reads cannot observe the earlier SC stores in opposite
// modification-order directions.
//
//   T0: x.store(1, SeqCst)
//       y.store(2, SeqCst)
//       a = y.load(Relaxed) // 1
//   T1: y.store(1, SeqCst)
//       x.store(2, SeqCst)
//       b = x.load(Relaxed) // 1
//
// Forbidden outcome: [a, b] = [1, 1].
#[test]
fn rc11_forbids_two_plus_two_w() {
    fn inner() -> Vec<usize> {
        let mut lt = LogTest::default();

        lt.add(|mut eg: Environment| {
            eg.a.store(1, Ordering::SeqCst);
            eg.b.store(2, Ordering::SeqCst);
            eg.b.load(Ordering::Relaxed)
        });

        lt.add(|mut eg: Environment| {
            eg.b.store(1, Ordering::SeqCst);
            eg.a.store(2, Ordering::SeqCst);
            eg.a.load(Ordering::Relaxed)
        });

        lt.run()
    }

    let forbidden = vec![1, 1];

    assert_never_observes(inner, forbidden);
}
