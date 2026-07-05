use crate::common::harness::{Environment, LogTest};
use crate::common::utils::{permutations, run_until};
use std::sync::atomic::Ordering;

mod common;

/*
https://www.open-std.org/jtc1/sc22/wg21/docs/papers/2018/p0668r5.html
https://www.open-std.org/jtc1/sc22/wg21/docs/papers/2018/p0982r1.html
https://plv.mpi-sws.org/scfix/paper.pdf

C++20 revised the memory model: the seq_cst total order need only agree with
strongly-happens-before, allowing mixed seq_cst/acquire/release behaviors that
match real hardware (P0668R5); seq_cst fences were strengthened (P0668R5); and
same-thread non-RMW writes no longer extend release sequences (P0982R1).
RC11's formal no-thin-air rule, acyclic(sb | rf), was not adopted.
*/

/*
https://plv.mpi-sws.org/scfix/paper.pdf

P0668R5: C++20 no longer requires the SC order to agree with all
happens-before edges.

This combination of loaded values is allowed. Each reader sees its own side's
store but not the other's, which no interleaving can produce.

    T0: x.store(1, SeqCst)
    T1: a = x.load(Acquire) // 1
        c = y.load(SeqCst)  // 0
    T2: b = y.load(Acquire) // 1
        d = x.load(SeqCst)  // 0
    T3: y.store(1, SeqCst)

The log backend does not currently simulate this outcome; graph does.
*/

#[test]
#[cfg_attr(
    not(feature = "graph"),
    ignore = "current log backend is stricter than C++20 for IRIW-acq-sc"
)]
fn cxx20_allows_iriw_acq_sc() {
    fn inner() -> Vec<usize> {
        let mut lt = LogTest::default();

        lt.add(|mut eg: Environment| {
            eg.a.store(1, Ordering::SeqCst);
            0
        });

        lt.add(|mut eg: Environment| {
            let x = eg.a.load(Ordering::Acquire);
            let y = eg.b.load(Ordering::SeqCst);
            x * 10 + y
        });

        lt.add(|mut eg: Environment| {
            let y = eg.b.load(Ordering::Acquire);
            let x = eg.a.load(Ordering::SeqCst);
            y * 10 + x
        });

        lt.add(|mut eg: Environment| {
            eg.b.store(1, Ordering::SeqCst);
            0
        });

        lt.run()
    }

    // With one store per variable there are no coherence constraints, so all
    // 16 combinations are allowed; only [0, 10, 10, 0] needs weak memory.
    assert!(run_until(
        inner,
        permutations(vec![
            vec![0],
            vec![0, 1, 10, 11],
            vec![0, 1, 10, 11],
            vec![0]
        ]),
    ));
}

/*
https://www.open-std.org/jtc1/sc22/wg21/docs/papers/2018/p0668r5.html

P0668R5: Z6.U is the proposal's main example for why C++20 weakens the old
seq_cst/happens-before interaction.

This combination of loaded values is allowed. The release store to y reaches
T1 through the RMW, but that no longer forces x's store into the SC order.

    T0: x.store(1, SeqCst)
        y.store(1, Release)
    T1: b = y.fetch_add(1, SeqCst) // 1, writes 2
        c = y.load(Relaxed)        // 3
    T2: y.store(3, SeqCst)
        a = x.load(SeqCst)         // 0

The log backend does not currently simulate this outcome; graph does.
*/

#[test]
#[cfg_attr(
    not(feature = "graph"),
    ignore = "current log backend is stricter than C++20 for Z6.U"
)]
fn cxx20_allows_z6_u() {
    fn inner() -> Vec<usize> {
        let mut lt = LogTest::default();

        lt.add(|mut eg: Environment| {
            eg.a.store(1, Ordering::SeqCst);
            eg.b.store(1, Ordering::Release);
            0
        });

        lt.add(|mut eg: Environment| {
            let y = eg.b.fetch_op(|v| v + 1, Ordering::SeqCst);
            let later_y = eg.b.load(Ordering::Relaxed);
            y * 10 + later_y
        });

        lt.add(|mut eg: Environment| {
            eg.b.store(3, Ordering::SeqCst);
            eg.a.load(Ordering::SeqCst)
        });

        lt.run()
    }

    // b is the RMW's read: 0 (initial), 1 (T0's store), or 3 (T2's store).
    // c must follow the RMW's write (b + 1) in modification order, so its
    // options depend on b. All 12 combinations with a in {0, 1} are allowed.
    let mut allowed = vec![];
    for (b, cs) in [(0, [1, 3]), (1, [2, 3]), (3, [4, 1])] {
        for c in cs {
            for a in [0, 1] {
                allowed.push(vec![0, b * 10 + c, a]);
            }
        }
    }

    assert!(run_until(inner, allowed));
}

/*
https://www.open-std.org/jtc1/sc22/wg21/docs/papers/2018/p0668r5.html

P0668R5: C++20 strengthened seq_cst fences interleaved with relaxed operations.

This combination of loaded values is forbidden. seq_cst fences are totally
ordered, and the relaxed operations cannot reorder with them.

    T0: x.store(1, Relaxed)
    T1: a = x.load(Relaxed) // 1
        fence(SeqCst)
        b = y.load(Relaxed) // 0
    T2: y.store(1, Relaxed)
        fence(SeqCst)
        c = x.load(Relaxed) // 0
*/

#[test]
fn cxx20_forbids_rwc_syncs() {
    fn inner() -> Vec<usize> {
        let mut lt = LogTest::default();

        lt.add(|mut eg: Environment| {
            eg.a.store(1, Ordering::Relaxed);
            0
        });

        lt.add(|mut eg: Environment| {
            let x = eg.a.load(Ordering::Relaxed);
            eg.fence(Ordering::SeqCst);
            let y = eg.b.load(Ordering::Relaxed);
            x * 10 + y
        });

        lt.add(|mut eg: Environment| {
            eg.b.store(1, Ordering::Relaxed);
            eg.fence(Ordering::SeqCst);
            eg.a.load(Ordering::Relaxed)
        });

        lt.run()
    }

    let forbidden = vec![0, 10, 0];
    let mut allowed = permutations(vec![vec![0], vec![0, 1, 10, 11], vec![0, 1]]);
    allowed.retain(|v| *v != forbidden);

    assert!(run_until(inner, allowed));
}

/*
https://www.open-std.org/jtc1/sc22/wg21/docs/papers/2018/p0668r5.html

P0668R5: the strengthened seq_cst fence rule is cumulative through
happens-before.

This combination of loaded values is forbidden just as in RWC+syncs: the
release/acquire sync on z carries T0's store into the fence ordering.

    T0: x.store(1, Relaxed)
        z.store(1, Release)
    T1: a = z.load(Acquire) // 1
        fence(SeqCst)
        b = y.load(Relaxed) // 0
    T2: y.store(1, Relaxed)
        fence(SeqCst)
        c = x.load(Relaxed) // 0
*/

#[test]
fn cxx20_forbids_w_rwc() {
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
    let mut allowed = permutations(vec![vec![0], vec![0, 1, 10, 11], vec![0, 1]]);
    allowed.retain(|v| *v != forbidden);

    assert!(run_until(inner, allowed));
}

/*
https://www.open-std.org/jtc1/sc22/wg21/docs/papers/2018/p0982r1.html

P0982R1: C++20 removed same-thread non-RMW writes from release sequences.

This combination of loaded values is allowed. Reading the later relaxed store
with Acquire no longer synchronizes with the earlier Release store, so data
may still read 0.

    T0: data.store(1, Relaxed)
        flag.store(1, Release)
        flag.store(2, Relaxed)
    T1: while flag.load(Acquire) < 2 {}
        data.load(Relaxed) // may be 0
*/

#[test]
fn cxx20_same_thread_relaxed_store_does_not_extend_release_sequence() {
    fn inner() -> Vec<usize> {
        let mut lt = LogTest::default();

        lt.add(|mut eg: Environment| {
            eg.a.store(1, Ordering::Relaxed);
            eg.b.store(1, Ordering::Release);
            eg.b.store(2, Ordering::Relaxed);
            0
        });

        lt.add(|mut eg: Environment| {
            while eg.b.load(Ordering::Acquire) < 2 {}
            eg.a.load(Ordering::Relaxed)
        });

        lt.run()
    }

    assert!(run_until(inner, vec![vec![0, 0], vec![0, 1]]));
}

/*
https://www.open-std.org/jtc1/sc22/wg21/docs/papers/2018/p0982r1.html

P0982R1 kept the original motivation for release sequences: RMWs still extend
a release sequence even when the RMW itself is Relaxed.

T2 acquires through the RMW's write, so it must observe data = 1. T1's
relaxed RMW does not acquire, so its own data read may still be 0.

    T0: data.store(1, Relaxed)
        flag.store(1, Release)
    T1: flag.compare_exchange_weak(1, 2, Relaxed, Relaxed)
        data.load(Relaxed) // may be 0
    T2: while flag.load(Acquire) < 2 {}
        data.load(Relaxed) // 1
*/

#[test]
fn cxx20_relaxed_rmw_still_extends_release_sequence() {
    fn inner() -> Vec<usize> {
        let mut lt = LogTest::default();

        lt.add(|mut eg: Environment| {
            eg.a.store(1, Ordering::Relaxed);
            eg.b.store(1, Ordering::Release);
            0
        });

        lt.add(|mut eg: Environment| {
            while eg
                .b
                .exchange_weak(1, 2, Ordering::Relaxed, Ordering::Relaxed)
                .is_err()
            {}
            eg.a.load(Ordering::Relaxed)
        });

        lt.add(|mut eg: Environment| {
            while eg.b.load(Ordering::Acquire) < 2 {}
            eg.a.load(Ordering::Relaxed)
        });

        lt.run()
    }

    assert!(run_until(inner, vec![vec![0, 0, 1], vec![0, 1, 1]]));
}
