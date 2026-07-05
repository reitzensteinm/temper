use crate::common::harness::{Environment, LogTest};
use std::sync::atomic::Ordering;

mod common;

// RC11: https://plv.mpi-sws.org/scfix/
// Model: https://plv.mpi-sws.org/scfix/rc11.cat
//
// Passing tests document where memlog agrees with RC11. Tests for RC11-only
// allowed outcomes are ignored unless the `rc11` feature is enabled.
//
// In the litmus comments below, paper variables x/y/z map to eg.a/eg.b/eg.c.
const ATTEMPTS: usize = 10_000;

fn assert_never_observes<F>(mut inner: F, forbidden: Vec<usize>)
where
    F: FnMut() -> Vec<usize>,
{
    for _ in 0..ATTEMPTS {
        assert_ne!(inner(), forbidden);
    }
}

fn assert_eventually_observes<F>(mut inner: F, expected: Vec<usize>, name: &str)
where
    F: FnMut() -> Vec<usize>,
{
    for _ in 0..ATTEMPTS {
        if inner() == expected {
            return;
        }
    }

    panic!("never observed RC11-allowed {name} result {expected:?}");
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

// RC11 allows this IRIW-acq-sc behavior:
//
// IRIW-acq-sc:
//   T0: x.store(1, SeqCst)
//   T1: a = x.load(Acquire) // 1
//       c = y.load(SeqCst)  // 0
//   T2: b = y.load(Acquire) // 1
//       d = x.load(SeqCst)  // 0
//   T3: y.store(1, SeqCst)
//
// RC11-allowed outcome: [T0, a * 10 + c, b * 10 + d, T3] =
// [0, 10, 10, 0].
//
// Current memlog forbids it because an SC load cannot read before an SC store
// to the same address once that store is present in the global operation log.
#[cfg(not(feature = "graph"))]
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

    let forbidden = vec![0, 10, 10, 0];

    assert_never_observes(inner, forbidden);
}

// This is the RC11 expectation for the same litmus test above. The original
// log backend models the older C++11 behavior and forbids this result, but the
// `rc11` feature enables graph and allows it by checking the SC event graph
// instead of requiring SC loads to observe the latest SC store to the same
// address.
#[test]
#[cfg_attr(
    not(feature = "rc11"),
    ignore = "enable the rc11 feature to run RC11-only IRIW-acq-sc"
)]
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

    assert_eventually_observes(inner, expected, "IRIW-acq-sc");
}

// Current memlog also forbids the RC11-allowed Z6.U outcome because it uses a
// single global operation log for SC operations.
//
// Z6.U:
//   T0: x.store(1, SeqCst)
//       y.store(1, Release)
//   T1: b = y.fetch_add(1, SeqCst) // 1, writes 2
//       c = y.load(Relaxed)        // 3
//   T2: y.store(3, SeqCst)
//       a = x.load(SeqCst)         // 0
//
// RC11-allowed outcome: [T0, b * 10 + c, a] = [0, 13, 0].
#[cfg(not(feature = "graph"))]
#[test]
fn memlog_forbids_rc11_z6_u() {
    fn inner() -> Vec<usize> {
        let mut lt = LogTest::default();

        lt.add(|mut eg: Environment| {
            eg.a.store(1, Ordering::SeqCst);
            eg.b.store(1, Ordering::Release);
            0
        });

        lt.add(|mut eg: Environment| {
            let b = eg.b.fetch_op(|v| v + 1, Ordering::SeqCst);
            let c = eg.b.load(Ordering::Relaxed);
            b * 10 + c
        });

        lt.add(|mut eg: Environment| {
            eg.b.store(3, Ordering::SeqCst);
            eg.a.load(Ordering::SeqCst)
        });

        lt.run()
    }

    let forbidden = vec![0, 13, 0];

    assert_never_observes(inner, forbidden);
}

// Z6.U: RC11 weakens the old C11 SC condition enough to allow this outcome.
// The second thread's SC RMW reads y=1 and writes y=2; its later relaxed load
// reads y=3, while the final SC load reads the initial x=0.
//
// See the litmus comment above `memlog_forbids_rc11_z6_u`.
#[test]
#[cfg_attr(
    not(feature = "rc11"),
    ignore = "enable the rc11 feature to run RC11-only Z6.U"
)]
fn rc11_allows_z6_u() {
    fn inner() -> Vec<usize> {
        let mut lt = LogTest::default();

        lt.add(|mut eg: Environment| {
            eg.a.store(1, Ordering::SeqCst);
            eg.b.store(1, Ordering::Release);
            0
        });

        lt.add(|mut eg: Environment| {
            let b = eg.b.fetch_op(|v| v + 1, Ordering::SeqCst);
            let c = eg.b.load(Ordering::Relaxed);
            b * 10 + c
        });

        lt.add(|mut eg: Environment| {
            eg.b.store(3, Ordering::SeqCst);
            eg.a.load(Ordering::SeqCst)
        });

        lt.run()
    }

    let expected = vec![0, 13, 0];

    assert_eventually_observes(inner, expected, "Z6.U");
}

// RWC+syncs: RC11 strengthens SC fences so they recover sequential consistency
// for relaxed atomics placed around them. The [0, 10, 0] result encodes
// a=1, b=0, c=0.
//
//   T0: x.store(1, Relaxed)
//   T1: a = x.load(Relaxed) // 1
//       fence(SeqCst)
//       b = y.load(Relaxed) // 0
//   T2: y.store(1, Relaxed)
//       fence(SeqCst)
//       c = x.load(Relaxed) // 0
//
// Forbidden outcome: [T0, a * 10 + b, c] = [0, 10, 0].
#[test]
fn rc11_forbids_rwc_syncs() {
    fn inner() -> Vec<usize> {
        let mut lt = LogTest::default();

        lt.add(|mut eg: Environment| {
            eg.a.store(1, Ordering::Relaxed);
            0
        });

        lt.add(|mut eg: Environment| {
            let a = eg.a.load(Ordering::Relaxed);
            eg.fence(Ordering::SeqCst);
            let b = eg.b.load(Ordering::Relaxed);
            a * 10 + b
        });

        lt.add(|mut eg: Environment| {
            eg.b.store(1, Ordering::Relaxed);
            eg.fence(Ordering::SeqCst);
            eg.a.load(Ordering::Relaxed)
        });

        lt.run()
    }

    let forbidden = vec![0, 10, 0];

    assert_never_observes(inner, forbidden);
}

// W+RWC: RC11's final SC-fence condition makes fences cumulative. After the
// acquire load of z observes the release store, the SC fence must carry the
// earlier relaxed write to x.
//
//   T0: x.store(1, Relaxed)
//       z.store(1, Release)
//   T1: a = z.load(Acquire) // 1
//       fence(SeqCst)
//       b = y.load(Relaxed) // 0
//   T2: y.store(1, Relaxed)
//       fence(SeqCst)
//       c = x.load(Relaxed) // 0
//
// Forbidden outcome: [T0, a * 10 + b, c] = [0, 10, 0].
#[test]
fn rc11_forbids_w_rwc() {
    fn inner() -> Vec<usize> {
        let mut lt = LogTest::default();

        lt.add(|mut eg: Environment| {
            eg.a.store(1, Ordering::Relaxed);
            eg.c.store(1, Ordering::Release);
            0
        });

        lt.add(|mut eg: Environment| {
            let z = eg.c.load(Ordering::Acquire);
            eg.fence(Ordering::SeqCst);
            let y = eg.b.load(Ordering::Relaxed);
            z * 10 + y
        });

        lt.add(|mut eg: Environment| {
            eg.b.store(1, Ordering::Relaxed);
            eg.fence(Ordering::SeqCst);
            eg.a.load(Ordering::Relaxed)
        });

        lt.run()
    }

    let forbidden = vec![0, 10, 0];

    assert_never_observes(inner, forbidden);
}
