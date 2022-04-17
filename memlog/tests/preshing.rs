use crate::common::harness::{Environment, LogTest};
use crate::common::utils::run_until;
use std::sync::atomic::Ordering;

mod common;

/* Tests from Preshing on Programming */

// https://preshing.com/20130823/the-synchronizes-with-relation/
#[test]
fn test_basic_acq_rel() {
    fn intel_failure_inner() -> Vec<usize> {
        let mut lt = LogTest::default();

        lt.add(|mut eg: Environment| {
            eg.a.store(1, Ordering::Relaxed);
            eg.b.store(1, Ordering::Release);
            0
        });

        lt.add(|mut eg: Environment| {
            while eg.b.load(Ordering::Acquire) == 0 {}
            eg.a.load(Ordering::Relaxed)
        });

        lt.run()
    }

    assert!(run_until(intel_failure_inner, vec![vec![0, 1]]));
}

// https://preshing.com/20130922/acquire-and-release-fences/
#[test]
fn test_basic_acq_rel_rel_fence() {
    fn intel_failure_inner() -> Vec<usize> {
        let mut lt = LogTest::default();

        lt.add(|mut eg: Environment| {
            eg.a.store(1, Ordering::Relaxed);
            eg.fence(Ordering::Release);
            eg.b.store(1, Ordering::Relaxed);
            0
        });

        lt.add(|mut eg: Environment| {
            while eg.b.load(Ordering::Acquire) == 0 {}
            eg.a.load(Ordering::Relaxed)
        });

        lt.run()
    }

    assert!(run_until(intel_failure_inner, vec![vec![0, 1]]));
}

#[test]
fn test_basic_acq_rel_acq_fence() {
    fn intel_failure_inner() -> Vec<usize> {
        let mut lt = LogTest::default();

        lt.add(|mut eg: Environment| {
            eg.a.store(1, Ordering::Relaxed);
            eg.b.store(1, Ordering::Release);
            0
        });

        lt.add(|mut eg: Environment| {
            while eg.b.load(Ordering::Relaxed) == 0 {}
            eg.fence(Ordering::Acquire);
            eg.a.load(Ordering::Relaxed)
        });

        lt.run()
    }

    assert!(run_until(intel_failure_inner, vec![vec![0, 1]]));
}

// https://preshing.com/20131125/acquire-and-release-fences-dont-work-the-way-youd-expect/
#[test]
fn test_release_reorder() {
    fn intel_failure_inner() -> Vec<usize> {
        let mut lt = LogTest::default();

        lt.add(|mut eg: Environment| {
            eg.a.store(1, Ordering::Release);
            eg.b.store(1, Ordering::Relaxed);
            0
        });

        lt.add(|mut eg: Environment| {
            while eg.b.load(Ordering::Acquire) == 0 {}
            eg.a.load(Ordering::Relaxed)
        });

        lt.run()
    }

    assert!(run_until(intel_failure_inner, vec![vec![0, 0], vec![0, 1]]));
}
