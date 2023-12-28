use crate::common::harness::{Environment, LogTest, Value};
use crate::common::utils::{permutations, run_until, run_until_pred};
use std::collections::HashSet;
use std::sync::atomic::Ordering;

mod common;

/*
Test cases that fully exercise orderings for fetch and modify operations. It's a little messy - the tests are:
 * If using Acquire ordering, Thread #2 should synchronise with the Release in Thread #1, perceiving the write to c
 * If using Release ordering, the Acquire in Thread #3 should synchronise with Thread #2, perceiving the write to b
 * A fairly standard SeqCst test, using the fetch_add op as a SeqCst write to a flag read by Thread #4

The operations tested are:
 * compare_exchange (happy path)
 * fetch_op (fetch_add, etc)
 * fetch_update (happy path)
 */

#[derive(Copy, Clone)]
enum ModifyTestType {
    CompareExchange,
    FetchOp,
    FetchUpdate,
}

#[test]
fn test_fetch_and_modify() {
    const ANOMALY_STALE_B: usize = 10;
    const ANOMALY_STALE_C: usize = 100;
    const ANOMALY_SEQ_CST: usize = 1000;

    fn inner(test_type: ModifyTestType, ordering: Ordering) -> Vec<usize> {
        let mut lt = LogTest::default();

        lt.add(move |mut eg: Environment| {
            eg.c.store(1, Ordering::Relaxed);
            eg.a.fetch_op(|v| v + 1, Ordering::Release);

            0
        });

        lt.add(move |mut eg: Environment| {
            eg.b.store(1, Ordering::Relaxed);

            // Operation with variable ordering
            let res = match test_type {
                ModifyTestType::CompareExchange => {
                    eg.a.exchange_weak(1, 11, ordering, Ordering::Relaxed)
                }
                ModifyTestType::FetchOp => Ok(eg.a.fetch_op(|v| v + 10, ordering)),
                ModifyTestType::FetchUpdate => {
                    eg.a.fetch_update(|v| Some(v + 10), ordering, Ordering::Relaxed)
                }
            };

            if let Ok(a) = res {
                let c = eg.c.load(Ordering::Relaxed);

                if eg.d.load(Ordering::SeqCst) == 0 {
                    eg.e.fetch_op(|v| v + 1, Ordering::Release);
                }

                // If we perceive Thread 1's write to a, we should see its write to c under Acquire, AcqRel, SeqCst
                if a == 1 && c == 0 {
                    ANOMALY_STALE_C
                } else {
                    0
                }
            } else {
                0
            }
        });

        lt.add(move |mut eg: Environment| {
            let a = eg.a.load(Ordering::Acquire);
            let b = eg.b.load(Ordering::Relaxed);

            // If we perceive Thread 2's write to a, we should see its write to b under Release, AcqRel, SeqCst
            if b == 0 && a > 5 {
                ANOMALY_STALE_B
            } else {
                0
            }
        });

        lt.add(move |mut eg: Environment| {
            eg.d.store(1, Ordering::SeqCst);

            let a = eg.a.load(Ordering::SeqCst);

            // If we don't see Thread 2's write to a, we should see it's write to e under SeqCst
            if a == 0 && eg.e.load(Ordering::Relaxed) != 0 {
                ANOMALY_SEQ_CST
            } else {
                0
            }
        });

        lt.run()
    }

    let check_result = |res: &HashSet<Vec<usize>>, expected: Vec<usize>| {
        let mut seen = HashSet::<usize>::new();

        for entry in res {
            for v in entry {
                seen.insert(*v);
            }
        }
        let expected: HashSet<usize> = expected.into_iter().collect();
        seen.eq(&expected)
    };

    for op in [
        ModifyTestType::FetchOp,
        ModifyTestType::CompareExchange,
        ModifyTestType::FetchUpdate,
    ] {
        assert!(run_until_pred(
            || inner(op, Ordering::Relaxed),
            |v| {
                check_result(
                    v,
                    vec![0, ANOMALY_STALE_B, ANOMALY_STALE_C, ANOMALY_SEQ_CST],
                )
            }
        ));

        assert!(run_until_pred(
            || inner(op, Ordering::Release),
            |v| { check_result(v, vec![0, ANOMALY_STALE_C, ANOMALY_SEQ_CST],) }
        ));

        assert!(run_until_pred(
            || inner(op, Ordering::Acquire),
            |v| { check_result(v, vec![0, ANOMALY_STALE_B, ANOMALY_SEQ_CST],) }
        ));

        assert!(run_until_pred(
            || inner(op, Ordering::AcqRel),
            |v| { check_result(v, vec![0, ANOMALY_SEQ_CST],) }
        ));

        assert!(run_until_pred(
            || inner(op, Ordering::SeqCst),
            |v| { check_result(v, vec![0],) }
        ));
    }
}

/*
Test the distinction between compare_exchange, and compare_exchange_weak.
This test uses run_sequential, and as such is deterministic. The exchange always has the correct old value,
so compare_exchange will always succeed, but compare_exchange_weak will spuriously fail.
 */

#[test]
fn test_compare_exchange_weakness() {
    fn inner(weak: bool) -> Vec<usize> {
        let mut lt = LogTest::default();

        lt.add(move |mut eg: Environment| {
            if weak {
                let _ =
                    eg.a.exchange_weak(0, 1, Ordering::Relaxed, Ordering::Relaxed);
            } else {
                let _ = eg.a.exchange(0, 1, Ordering::Relaxed, Ordering::Relaxed);
            }

            eg.b.store(1, Ordering::Release);

            0
        });

        lt.add(move |mut eg: Environment| {
            while eg.b.load(Ordering::Acquire) == 0 {}
            eg.a.load(Ordering::Acquire)
        });

        lt.run_sequential()
    }

    // compare_exchange_weak can spuriously fail
    assert!(run_until(|| inner(true), vec![vec![0, 0], vec![0, 1]]));
    // However, compare_exchange should succeed if the correct old value is passed in
    assert!(run_until(|| inner(false), vec![vec![0, 1]]));
}

/*
Tests the failure ordering of fetch modify ops. The call uses the incorrect old value, and thus always succeeds.
A Relaxed failure ordering doesn't always perceive the write to a, however higher levels will. SeqCst is used as
the success ordering, ensuring it's not being erroneously used.
 */
#[test]
fn test_fetch_modify_failure_ordering() {
    fn inner(test_type: ModifyTestType, failure_ordering: Ordering) -> Vec<usize> {
        let mut lt = LogTest::default();

        lt.add(move |mut eg: Environment| {
            eg.a.store(1, Ordering::Relaxed);
            eg.b.store(1, Ordering::Release);

            0
        });

        lt.add(move |mut eg: Environment| {
            // Failed exchange. Using the strongest possible success ordering, which should be ignored.
            let b = match test_type {
                ModifyTestType::CompareExchange => {
                    eg.b.exchange_weak(2, 3, Ordering::SeqCst, failure_ordering)
                }
                ModifyTestType::FetchOp => panic!(),
                ModifyTestType::FetchUpdate => {
                    eg.b.fetch_update(|_| None, Ordering::SeqCst, failure_ordering)
                }
            }
            .unwrap_err();

            let a = eg.a.load(Ordering::Relaxed);

            if b == 1 {
                a
            } else {
                1
            }
        });

        lt.run_sequential()
    }

    for test in [ModifyTestType::CompareExchange, ModifyTestType::FetchUpdate] {
        // Relaxed can see the store to b without seeing the store to a
        assert!(run_until(
            || inner(test, Ordering::Relaxed),
            vec![vec![0, 0], vec![0, 1]]
        ));

        assert!(run_until(
            || inner(test, Ordering::Acquire),
            vec![vec![0, 1]]
        ));

        assert!(run_until(
            || inner(test, Ordering::SeqCst),
            vec![vec![0, 1]]
        ));
    }
}

/*
Additional failure test to ensure SeqCst failure ordering has correct behaviour with fetch modify ops.
This is a standard SeqCst test, however we're using failed exchanges in place of a SeqCst load.
 */
#[test]
fn test_fetch_modify_failure_seq_cst_ordering() {
    fn inner(test_type: ModifyTestType, failure_ordering: Ordering) -> Vec<usize> {
        let mut lt = LogTest::default();

        let failed_read_op = move |v: &mut Value| {
            match test_type {
                ModifyTestType::CompareExchange => {
                    v.exchange_weak(2, 3, Ordering::SeqCst, failure_ordering)
                }
                ModifyTestType::FetchOp => panic!(),
                ModifyTestType::FetchUpdate => {
                    v.fetch_update(|_| None, Ordering::SeqCst, failure_ordering)
                }
            }
            .unwrap_err()
        };

        lt.add(move |mut eg: Environment| {
            eg.a.store(1, Ordering::SeqCst);
            let b = failed_read_op(&mut eg.b);

            if b == 0 {
                eg.c.fetch_op(|v| v + 1, Ordering::Relaxed);
            }

            eg.c.load(Ordering::Relaxed)
        });

        lt.add(move |mut eg: Environment| {
            eg.b.store(1, Ordering::SeqCst);
            let a = failed_read_op(&mut eg.a);

            if a == 0 {
                eg.c.fetch_op(|v| v + 1, Ordering::Relaxed);
            }

            eg.c.load(Ordering::Relaxed)
        });

        lt.run()
    }
    for test in [ModifyTestType::CompareExchange, ModifyTestType::FetchUpdate] {
        let non_seq_cst_outcomes = vec![
            vec![0, 0],
            vec![0, 1],
            vec![1, 0],
            vec![1, 1],
            vec![2, 1],
            vec![1, 2],
            vec![2, 2],
        ];

        assert!(run_until(
            || inner(test, Ordering::Relaxed),
            non_seq_cst_outcomes.clone()
        ));
        assert!(run_until(
            || inner(test, Ordering::Acquire),
            non_seq_cst_outcomes
        ));

        assert!(run_until(
            || inner(test, Ordering::SeqCst),
            permutations(vec![vec![0, 1], vec![0, 1]])
        ));
    }
}

// Todo: Test fetch ordering in fetch_update in the case of success
// Currently, the test harness is not capable of reentrant access to memlog, which means this cannot be tested.
