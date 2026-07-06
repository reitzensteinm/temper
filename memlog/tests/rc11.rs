use crate::common::harness::{Environment, LogTest};
use crate::common::utils::{permutations, run_until};
use std::sync::atomic::Ordering;

mod common;

/*
https://plv.mpi-sws.org/scfix/
https://plv.mpi-sws.org/scfix/rc11.cat
https://plv.mpi-sws.org/scfix/paper.pdf

RC11 repaired the C++11 SC semantics and added a formal no-thin-air rule,
acyclic(sb | rf), which C++20 did not adopt. The SC changes that C++20 did
adopt (IRIW-acq-sc, Z6.U, RWC+syncs, W+RWC) are covered in cxx20.rs; this
file keeps the RC11-only rules. Paper variables x/y map to eg.a/eg.b.
*/

/*
https://plv.mpi-sws.org/scfix/paper.pdf

LB: RC11's no-thin-air rule rejects executions with cycles in sb | rf.

This combination of loaded values is forbidden. Each relaxed load would have
to read from the store that is only justified by its own result.

    T0: b = y.load(Relaxed) // 1
        x.store(1, Relaxed)
    T1: a = x.load(Relaxed) // 1
        y.store(1, Relaxed)
*/

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
    let mut allowed = permutations(vec![vec![0, 1], vec![0, 1]]);
    allowed.retain(|v| *v != forbidden);

    assert!(run_until(inner, allowed));
}

/*
https://plv.mpi-sws.org/scfix/paper.pdf

LB+deps: the standard out-of-thin-air example from the RC11 paper.

This combination of loaded values is forbidden. A 1 would appear out of thin
air: each conditional store only runs if the other one already happened.

    T0: a = x.load(Relaxed) // 1
        if a != 0 { y.store(a, Relaxed) }
    T1: b = y.load(Relaxed) // 1
        if b != 0 { x.store(b, Relaxed) }
*/

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

    // Any 1 would be out of thin air, so [0, 0] is the only allowed outcome.
    assert!(run_until(inner, vec![vec![0, 0]]));
}

/*
https://plv.mpi-sws.org/scfix/rc11.cat

SB: the classic store-buffering litmus test.

This combination of loaded values is forbidden. The SC operations are totally
ordered, so one of the stores comes first and its thread's load sees the
other store.

    T0: x.store(1, SeqCst)
        a = y.load(SeqCst) // 0
    T1: y.store(1, SeqCst)
        b = x.load(SeqCst) // 0
*/

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
    let mut allowed = permutations(vec![vec![0, 1], vec![0, 1]]);
    allowed.retain(|v| *v != forbidden);

    assert!(run_until(inner, allowed));
}

/*
https://plv.mpi-sws.org/scfix/rc11.cat

2+2W: SC stores to two locations cannot be ordered in opposite directions.

This combination of loaded values is forbidden. The relaxed reads cannot
observe the earlier SC stores in opposite modification-order directions.

    T0: x.store(1, SeqCst)
        y.store(2, SeqCst)
        a = y.load(Relaxed) // 1
    T1: y.store(1, SeqCst)
        x.store(2, SeqCst)
        b = x.load(Relaxed) // 1
*/

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

    // Each read is sequenced after its own thread's store, so it sees that
    // store or a later one in modification order.
    let forbidden = vec![1, 1];
    let mut allowed = permutations(vec![vec![1, 2], vec![1, 2]]);
    allowed.retain(|v| *v != forbidden);

    assert!(run_until(inner, allowed));
}
