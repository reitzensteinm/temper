use crate::common::harness::{Environment, LogTest};
use crate::common::utils::{permutations, run_until};
use std::sync::atomic::Ordering;

mod common;

// http://www.open-std.org/jtc1/sc22/wg21/docs/papers/2017/n4713.pdf
// 32.4.4-32.4.8
// From https://stackoverflow.com/questions/59316262/when-is-a-memory-order-seq-cst-fence-useful

/*
For an atomic operation B that reads the value of an atomic object M
if there is a memory_order_seq_cst fence X sequenced before B
then B observes either the last memory_order_seq_cst modification of M preceding X in the total order S
or a later modification of M in its modification order.
*/

#[test]
fn test_fence_read() {
    fn inner() -> Vec<usize> {
        let mut lt = LogTest::default();

        lt.add(move |mut eg: Environment| {
            eg.a.store(1, Ordering::Relaxed); // Unrelated modification of M that is not memory_order_seq_cst
            eg.a.store(2, Ordering::SeqCst); // Operation A
            eg.a.store(3, Ordering::Relaxed); // Does not happen before A

            0
        });

        lt.add(move |mut eg: Environment| {
            eg.fence(Ordering::SeqCst);
            eg.a.load(Ordering::Relaxed) // Operation B
        });

        lt.run_sequential()
    }

    assert!(run_until(inner, permutations(vec![vec![0], vec![2, 3]])));
}

/*
For atomic operations A and B on an atomic object M,
where A modifies M and B takes its value,
if there is a memory_order_seq_cst fence X such that A is sequenced before X and B follows X in S,
then B observes either the effects of A or a later modification of M in its modification order.*/

#[test]
fn test_fence_write() {
    fn inner() -> Vec<usize> {
        let mut lt = LogTest::default();

        lt.add(move |mut eg: Environment| {
            eg.a.store(1, Ordering::Relaxed); // Unrelated modification of M that is not memory_order_seq_cst
            eg.a.store(2, Ordering::Relaxed); // Operation A
            eg.fence(Ordering::SeqCst); // Fence
            eg.a.store(3, Ordering::Relaxed);

            0
        });

        lt.add(move |mut eg: Environment| {
            eg.a.load(Ordering::SeqCst) // Operation B
        });

        lt.run_sequential()
    }

    assert!(run_until(inner, permutations(vec![vec![0], vec![2, 3]])));
}

/*
For atomic operations A and B on an atomic object M,
where A modifies M and B takes its value,
if there are memory_order_seq_cst fences X and Y such that A is sequenced before X,
Y is sequenced before B,
and X precedes Y in S,
then B observes either the effects of A or a later modification of M in its modification order.
*/

#[test]
fn test_fence_fence() {
    fn inner() -> Vec<usize> {
        let mut lt = LogTest::default();

        lt.add(move |mut eg: Environment| {
            eg.a.store(1, Ordering::Relaxed); // Unrelated modification of M that is not memory_order_seq_cst
            eg.a.store(2, Ordering::Relaxed); // Operation A
            eg.fence(Ordering::SeqCst); // Fence X
            eg.a.store(3, Ordering::Relaxed);

            0
        });

        lt.add(move |mut eg: Environment| {
            eg.fence(Ordering::SeqCst); // Fence Y
            eg.a.load(Ordering::Relaxed) // Operation B
        });

        lt.run_sequential()
    }

    assert!(run_until(inner, permutations(vec![vec![0], vec![2, 3]])));
}

// Store buffer litmus test. SeqCst rules out 0, 0 as a result
#[test]
fn test_intel_failure() {
    fn intel_failure_inner() -> Vec<usize> {
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

    assert!(run_until(
        intel_failure_inner,
        vec![vec![0, 1], vec![1, 0], vec![1, 1]]
    ));
}
