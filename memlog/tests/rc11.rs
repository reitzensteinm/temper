use crate::common::harness::{Environment, LogTest};
use crate::common::utils::run_until;
use std::sync::atomic::Ordering;

mod common;

// RC11: https://plv.mpi-sws.org/scfix/
// Model: https://plv.mpi-sws.org/scfix/rc11.cat
//
// Passing tests document where memlog agrees with RC11. Ignored tests document
// RC11-allowed outcomes that current memlog is too strong to produce.

// RC11's NO-THIN-AIR rule rejects executions with cycles in `sb | rf`.
//
// The forbidden [1, 1] result would require:
//   T1 store b --rf--> T0 load b --sb--> T0 store a
//   T0 store a --rf--> T1 load a --sb--> T1 store b
#[test]
fn relaxed_load_buffering_cannot_read_from_each_other() {
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

    assert!(run_until(
        inner,
        vec![vec![0, 0], vec![0, 1], vec![1, 0]]
    ));
}

// RC11 allows this IRIW-acq-sc behavior:
//   T1 observes a=1 with Acquire, then observes b=0 with SeqCst.
//   T2 observes b=1 with Acquire, then observes a=0 with SeqCst.
//
// Current memlog forbids it because an SC load cannot read before an SC store
// to the same address once that store is present in the global operation log.
#[test]
fn memlog_forbids_rc11_iriw_acq_sc() {
    fn inner() -> Vec<usize> {
        let mut lt = LogTest::default();

        lt.add(|mut eg: Environment| {
            eg.a.store(1, Ordering::SeqCst);
            0
        });

        lt.add(|mut eg: Environment| {
            let a = eg.a.load(Ordering::Acquire);
            let b = eg.b.load(Ordering::SeqCst);
            a * 10 + b
        });

        lt.add(|mut eg: Environment| {
            let b = eg.b.load(Ordering::Acquire);
            let a = eg.a.load(Ordering::SeqCst);
            b * 10 + a
        });

        lt.add(|mut eg: Environment| {
            eg.b.store(1, Ordering::SeqCst);
            0
        });

        lt.run()
    }

    let expected = vec![0, 10, 10, 0];

    for _ in 0..10_000 {
        assert_ne!(inner(), expected);
    }
}

// This is the RC11 expectation for the same litmus test above. It is ignored
// because current memlog models the older C++11 behavior and forbids this
// result.
#[test]
#[ignore = "current memlog is stricter than RC11 for IRIW-acq-sc"]
fn rc11_allows_iriw_acq_sc() {
    fn inner() -> Vec<usize> {
        let mut lt = LogTest::default();

        lt.add(|mut eg: Environment| {
            eg.a.store(1, Ordering::SeqCst);
            0
        });

        lt.add(|mut eg: Environment| {
            let a = eg.a.load(Ordering::Acquire);
            let b = eg.b.load(Ordering::SeqCst);
            a * 10 + b
        });

        lt.add(|mut eg: Environment| {
            let b = eg.b.load(Ordering::Acquire);
            let a = eg.a.load(Ordering::SeqCst);
            b * 10 + a
        });

        lt.add(|mut eg: Environment| {
            eg.b.store(1, Ordering::SeqCst);
            0
        });

        lt.run()
    }

    let expected = vec![0, 10, 10, 0];

    for _ in 0..10_000 {
        if inner() == expected {
            return;
        }
    }

    panic!("never observed RC11-allowed IRIW-acq-sc result {expected:?}");
}
