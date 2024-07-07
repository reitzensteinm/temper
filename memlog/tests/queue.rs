use crate::common::harness::{Environment, LogTest};
use crate::common::utils::{run_until, run_until_pred};
use std::sync::atomic::Ordering;

mod common;

#[test]
fn test_spsc_queue() {
    fn inner() -> Vec<usize> {
        let mut lt = LogTest::default();

        const QUEUE_LENGTH: usize = 2;
        const TO_WRITE: usize = 8;

        let read = |eg: &mut Environment| -> Option<usize> {
            let read_pointer = eg.a.load(Ordering::Relaxed);
            let write_pointer = eg.b.load(Ordering::Relaxed);

            if write_pointer > read_pointer {
                eg.fence(Ordering::SeqCst);
                let elem = eg.arr[read_pointer % QUEUE_LENGTH].load(Ordering::Relaxed);
                eg.a.store(read_pointer + 1, Ordering::Relaxed);
                Some(elem)
            } else {
                None
            }
        };

        let write = |eg: &mut Environment, v: usize| -> bool {
            let read_pointer = eg.a.load(Ordering::Relaxed);
            let write_pointer = eg.b.load(Ordering::Relaxed);
            let stored = write_pointer - read_pointer;

            if stored < QUEUE_LENGTH {
                eg.arr[write_pointer % QUEUE_LENGTH].store(v, Ordering::Relaxed);
                eg.fence(Ordering::SeqCst);
                eg.b.store(write_pointer + 1, Ordering::Relaxed);
                true
            } else {
                false
            }
        };

        lt.add(move |mut eg: Environment| {
            for x in 0..TO_WRITE {
                while !write(&mut eg, x) {}
            }
            0
        });

        lt.add(move |mut eg: Environment| {
            let mut out = 0;
            for _ in 0..TO_WRITE {
                loop {
                    if let Some(v) = read(&mut eg) {
                        out += v;
                        break;
                    }
                }
            }
            out
        });

        lt.run()
    }

    for _ in 0..100 {
        assert!(run_until(inner, vec![vec![0, 28]]));
    }
}

#[test]
fn test_mpmc_queue() {
    fn inner() -> Vec<usize> {
        let mut lt = LogTest::default();

        const QUEUE_LENGTH: usize = 2;
        const TO_WRITE: usize = 8;

        let write = |eg: &mut Environment, v: usize| -> bool {
            let discard_pointer = eg.c.load(Ordering::Relaxed);
            let write_pointer = eg.b.load(Ordering::Relaxed);
            let stored: isize = write_pointer as isize - discard_pointer as isize;

            if stored < QUEUE_LENGTH as isize
                && eg
                .b
                .exchange_weak(
                    write_pointer,
                    write_pointer + 1,
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                )
                .is_ok()
            {
                eg.arr[write_pointer % QUEUE_LENGTH].store(v, Ordering::Relaxed);
                eg.fence(Ordering::SeqCst);
                loop {
                    if eg
                        .d
                        .exchange_weak(
                            write_pointer,
                            write_pointer + 1,
                            Ordering::Relaxed,
                            Ordering::Relaxed,
                        )
                        .is_ok()
                    {
                        return true;
                    }
                }
            } else {
                false
            }
        };

        let read = |eg: &mut Environment| -> Option<usize> {
            let read_pointer = eg.a.load(Ordering::Relaxed);
            let committed_pointer = eg.d.load(Ordering::Relaxed);

            if committed_pointer > read_pointer
                && eg
                .a
                .exchange_weak(
                    read_pointer,
                    read_pointer + 1,
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                )
                .is_ok()
            {
                eg.fence(Ordering::SeqCst);
                //let elem = eg.arr[read_pointer % QUEUE_LENGTH].load(Ordering::Relaxed);
                loop {
                    if eg
                        .c
                        .exchange_weak(
                            read_pointer,
                            read_pointer + 1,
                            Ordering::Relaxed,
                            Ordering::Relaxed,
                        )
                        .is_ok()
                    {
                        let elem = eg.arr[read_pointer % QUEUE_LENGTH].load(Ordering::Relaxed);
                        return Some(elem);
                    }
                }
            } else {
                None
            }
        };

        lt.add(move |mut eg: Environment| {
            for x in 0..TO_WRITE {
                while !write(&mut eg, x) {}
            }
            0
        });

        lt.add(move |mut eg: Environment| {
            for x in 0..TO_WRITE {
                while !write(&mut eg, x) {}
            }
            0
        });

        lt.add(move |mut eg: Environment| {
            let mut out = 0;
            for _ in 0..TO_WRITE {
                loop {
                    if let Some(v) = read(&mut eg) {
                        out += v;
                        break;
                    }
                }
            }
            out
        });

        lt.add(move |mut eg: Environment| {
            let mut out = 0;
            for _ in 0..TO_WRITE {
                loop {
                    if let Some(v) = read(&mut eg) {
                        out += v;
                        break;
                    }
                }
            }
            out
        });

        lt.run()
    }

    //for x in 0..100 {
    assert!(run_until_pred(inner, |v| v
        .iter()
        .all(|r: &Vec<usize>| r.iter().sum::<usize>() == 28 * 2)));
    //assert!(run_until(inner, vec![vec![0, 0, 28 * 2]]));
    //}
}
