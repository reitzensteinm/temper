use crate::common::harness::{Environment, LogTest};
use crate::common::utils::{run_until, run_until_pred};
use std::collections::HashSet;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

mod common;

// Tests from "Rust Atomics and Locks" by Mara Bos
// Code examples taken fom https://github.com/m-ou-se/rust-atomics-and-locks/tree/main/examples

// The book touches on basically all of Rust concurrency. Memlog was specifically built to
// model non coherent low level memory access, so much of the book isn't specifically relevant.

/*
Chapter 1 & 2 skipped except where marked. Not in scope for memlog:
 * Thread spawning, joining, scoping
 * Rc, Cell, Refcell, Mutex
 * Thread sleeping, parking and waking
 * Reified time
 * Condvars
*/

// Listing 2.4
// Tests single thread lazy loads and stores, in isolation a single thread should emulate SeqCst

#[test]
fn test_2_4() {
    fn inner() -> Vec<[usize; 2]> {
        let mut lt = LogTest::default();

        lt.add(|mut eg: Environment| {
            let mut get_value = || {
                let mut v = eg.a.load(Ordering::Relaxed);
                if v == 0 {
                    v = 123;
                    eg.a.store(v, Ordering::Relaxed);
                }
                v
            };

            [get_value(), get_value()]
        });

        lt.run()
    }

    assert!(run_until(inner, vec![vec![[123_usize, 123_usize]]]));
}

// Listing 2.4
// Tests single thread fetch adds, in isolation a single thread should emulate SeqCst

#[test]
fn test_2_5() {
    fn inner() -> Vec<[usize; 2]> {
        let mut lt = LogTest::default();

        lt.add(|mut eg: Environment| {
            let v0 = eg.a.fetch_op(|x| x + 23, Ordering::Relaxed);
            let v1 = eg.a.load(Ordering::Relaxed);

            [v0, v1]
        });

        lt.run()
    }

    assert!(run_until(inner, vec![vec![[0_usize, 23_usize]]]));
}

// Listing 2.8 and 2.9 skipped - memlog testing harness only supports 64 bit ints

// Listing 2.10
// Emulating overflow in an ugly way, this test shows that duplicate IDs can be generated
// due to a race condition between the add and sub + panic checks when generating IDs.
// Test harness is reused for 2.12's fixed version

#[test]
fn test_2_10_and_2_12() {
    fn inner(fixed_version: bool) -> Vec<bool> {
        let mut lt = LogTest::default();

        const MAX_ID: usize = 2;
        const MAX_REPR: usize = 4;
        const PANIC_ID: usize = 100;

        // Broken allocate_id from 2.10
        let allocate_id = |eg: &mut Environment| {
            // The use of logic in fetch_op here is _not_ something fetch_add etc can provide. But as it's simulating
            // wrapping, it'll work for our purposes
            let id =
                eg.a.fetch_op(|v| if v == MAX_REPR { 0 } else { v + 1 }, Ordering::Relaxed);

            if id > MAX_ID {
                eg.a.fetch_op(|v| if v == 0 { MAX_REPR } else { v - 1 }, Ordering::Relaxed);
                PANIC_ID
            } else {
                id
            }
        };

        // Safe allocate_id from 2.12
        let allocate_id_safe = |eg: &mut Environment| {
            let mut id = eg.a.load(Ordering::Relaxed);
            loop {
                if id > MAX_ID {
                    return PANIC_ID;
                } else {
                    match eg
                        .a
                        .exchange_weak(id, id + 1, Ordering::Relaxed, Ordering::Relaxed)
                    {
                        Ok(_) => return id,
                        Err(v) => id = v,
                    }
                }
            }
        };

        for _ in 0..4 {
            lt.add(move |mut eg: Environment| {
                let mut seen_error = false;
                let mut seen = HashSet::<usize>::new();
                for _ in 0..10 {
                    let id = if fixed_version {
                        allocate_id_safe(&mut eg)
                    } else {
                        allocate_id(&mut eg)
                    };

                    if id == PANIC_ID {
                        break;
                    }

                    if seen.contains(&id) {
                        seen_error = true;
                    };

                    seen.insert(id);
                }
                seen_error
            });
        }

        lt.run()
    }

    let check = |hs: &HashSet<Vec<bool>>| hs.iter().any(|v| v.contains(&true));

    assert!(run_until_pred(|| inner(false), check));
    assert!(run_until(
        || inner(true),
        vec![vec![false, false, false, false]]
    ));
}

// Listing 2.10
// Tests an increment based on compare_exchange, using multiple threads to ensure success
#[test]
fn test_2_11() {
    fn inner() -> Vec<usize> {
        let mut lt = LogTest::default();

        let increment = |eg: &mut Environment| {
            let mut current = eg.a.load(Ordering::Relaxed);

            loop {
                let new = current + 1;

                // Since we're in a loop, I'm using exchange_weak
                match eg
                    .a
                    .exchange_weak(current, new, Ordering::Relaxed, Ordering::Relaxed)
                {
                    Ok(_) => return,
                    Err(v) => current = v,
                }
            }
        };

        for _ in 0..2 {
            lt.add(move |mut eg: Environment| {
                increment(&mut eg);
                increment(&mut eg);

                // Relaxed is sufficient ordering here, as one of the two threads will successfully write 4, and
                // that same thread is guaranteed to reread that same value
                eg.a.load(Ordering::Relaxed)
            });
        }

        lt.run()
    }

    assert!(run_until_pred(inner, |a| a.iter().any(|v| v.contains(&4))));
}

// Listing 2.13
// Uses compare_exchange to ensure consistent initialisation of a global constant
// generate_random_key can be called multiple times, but all threads must perceive the same get_key result
#[test]
fn test_2_13() {
    fn inner() -> Vec<usize> {
        let mut lt = LogTest::default();

        let random_seed: Arc<Mutex<usize>> = Arc::new(Mutex::new(0));

        let generate_random_key = |random_seed: &Arc<Mutex<usize>>| {
            // Doesn't need an rng - just needs to return something new each time it's called
            let mut m = random_seed.lock().unwrap();
            *m += 1;
            *m
        };

        let get_key = move |eg: &mut Environment, random_seed| {
            let key = eg.a.load(Ordering::Relaxed);

            if key == 0 {
                let new_key = generate_random_key(&random_seed);

                match eg
                    .a
                    .exchange(key, new_key, Ordering::Relaxed, Ordering::Relaxed)
                {
                    Ok(_) => new_key,
                    Err(k) => k,
                }
            } else {
                key
            }
        };

        for _ in 0..4 {
            let random_seed = random_seed.clone();
            lt.add(move |mut eg: Environment| get_key(&mut eg, random_seed.clone()));
        }

        lt.run()
    }

    let same_id = |arr: &Vec<usize>| arr.iter().all(|v| *v == arr[0]);
    assert!(run_until_pred(inner, |a| a.iter().all(same_id)));
}

// Listing 3.1
// Demonstrates relaxed reads can be perceived out of store order
#[test]
fn test_3_1() {
    fn inner() -> Vec<[usize; 2]> {
        let mut lt = LogTest::default();

        lt.add(|mut eg: Environment| {
            eg.a.store(1, Ordering::Relaxed);
            eg.b.store(1, Ordering::Relaxed);

            [0, 0]
        });

        lt.add(|mut eg: Environment| {
            let a = eg.a.load(Ordering::Relaxed);
            let b = eg.b.load(Ordering::Relaxed);

            [a, b]
        });

        lt.run()
    }

    // The writes can be perceived in any order, or not at all
    assert!(run_until(
        inner,
        vec![
            vec![[0, 0], [0, 0]],
            vec![[0, 0], [0, 1]],
            vec![[0, 0], [1, 0]],
            vec![[0, 0], [1, 1]]
        ]
    ));
}

// Listing 3.2 skipped - memlog does not support thread joining

// Listing 3.3
// Demonstrates that even for Relaxed stores/loads, stores are perceived in order
#[test]
fn test_3_3() {
    fn inner() -> Vec<bool> {
        let mut lt = LogTest::default();

        lt.add(|mut eg: Environment| {
            eg.a.store(1, Ordering::Relaxed);
            eg.a.store(2, Ordering::Relaxed);
            true
        });

        lt.add(|mut eg: Environment| {
            let v0 = eg.a.load(Ordering::Relaxed);
            let v1 = eg.a.load(Ordering::Relaxed);
            let v2 = eg.a.load(Ordering::Relaxed);
            let v3 = eg.a.load(Ordering::Relaxed);

            v0 <= v1 && v1 <= v2 && v2 <= v3
        });

        lt.run()
    }

    // Writes to the same variable should be perceived in order
    assert!(run_until(inner, vec![vec![true, true]]));
}

// Listing 3.4
// Demonstrates that all threads perceive the same modification order of a single atomic variable
#[test]
fn test_3_4() {
    fn inner() -> Vec<usize> {
        let mut lt = LogTest::default();

        lt.add(|mut eg: Environment| {
            eg.a.fetch_op(|v| v + 5, Ordering::Relaxed);
            0
        });
        lt.add(|mut eg: Environment| {
            eg.a.fetch_op(|v| v + 10, Ordering::Relaxed);
            0
        });

        for _ in 0..2 {
            lt.add(|mut eg: Environment| eg.a.load(Ordering::Relaxed));
        }

        lt.run()
    }

    // Reader threads can perceive before, after, or in the middle of the writes
    // If both perceive a middle write, the modification order must be preserved, which means
    // a reading of [5 10] or [10 5] from them is not possible.

    let check_result = |r: &Vec<usize>| {
        let hs: HashSet<&usize> = HashSet::from_iter(r.iter());
        !(hs.contains(&5) && hs.contains(&10))
    };

    assert!(run_until_pred(inner, |hs| hs.iter().all(check_result)));
}

// Listing 3.5 skipped - memlog does not support witchcraft

// Listing 3.6
// The reader can't terminate until the writer has written to b, at which point a is definitely available
#[test]
fn test_3_6() {
    fn inner() -> Vec<usize> {
        let mut lt = LogTest::default();

        lt.add(|mut eg: Environment| {
            eg.a.store(123, Ordering::Relaxed);
            eg.b.store(1, Ordering::Release);
            0
        });

        lt.add(|mut eg: Environment| {
            while eg.b.load(Ordering::Acquire) == 0 {}
            eg.a.load(Ordering::Relaxed)
        });

        lt.run()
    }

    // Second thread should always perceive the 123 write
    assert!(run_until(inner, vec![vec![0, 123]]));
}

// Listing 3.7 skipped - memlog does not yet support nonatomic access
// Todo: Implement 3.7 when non atomics are implemented

// Listing 3.8
// Implements a lock using Acquire and Release
// Adapted to run the lock operation in a loop so we've got a sensible victory condition
#[test]
fn test_3_8() {
    const THREAD_COUNT: usize = 10;
    fn inner() -> Vec<usize> {
        let mut lt = LogTest::default();

        for _ in 0..THREAD_COUNT {
            lt.add(|mut eg: Environment| {
                loop {
                    // This wasn't a weak exchange in the book - but we're putting it in a loop here
                    if eg
                        .a
                        .exchange_weak(0, 1, Ordering::Acquire, Ordering::Relaxed)
                        .is_ok()
                    {
                        let old = eg.b.load(Ordering::Relaxed);
                        eg.b.store(old + 1, Ordering::Relaxed);
                        eg.a.store(0, Ordering::Release);
                        break;
                    }
                }
                eg.b.load(Ordering::Relaxed)
            });
        }

        lt.run()
    }

    // Exactly one thread should see the final Mutex write
    assert!(run_until_pred(inner, |v| v
        .iter()
        .all(|a| a.contains(&THREAD_COUNT))));
}

// Listing 3.9 skipped - memlog does not yet support AtomicPtr<T>
// Otherwise identical to 2.13

// Listing 3.10
// Tests sequential consistent flags protecting data access
#[test]
fn test_3_10() {
    fn inner() -> Vec<usize> {
        let mut lt = LogTest::default();

        lt.add(|mut eg: Environment| {
            eg.b.store(1, Ordering::SeqCst);
            if eg.a.load(Ordering::SeqCst) == 0 {
                // Todo: Use nonatomic stores here for eg.c
                eg.c.fetch_op(|v| v + 1, Ordering::Relaxed);
            }

            eg.c.load(Ordering::Relaxed)
        });

        lt.add(|mut eg: Environment| {
            eg.a.store(1, Ordering::SeqCst);
            if eg.b.load(Ordering::SeqCst) == 0 {
                eg.c.fetch_op(|v| v + 1, Ordering::Relaxed);
            }

            eg.c.load(Ordering::Relaxed)
        });

        lt.run()
    }

    // At most one store is made to C, and either thread may see it if it occurs
    assert!(run_until(
        inner,
        vec![vec![0, 1], vec![1, 0], vec![0, 0], vec![1, 1]]
    ));
}

// Listing 3.11
// Tests Atomic-Fence synchronisation, where an atomic Release operation on the writer thread synchronises with
// an Acquire fence on the reader thread
#[test]
fn test_3_11() {
    fn inner() -> Vec<usize> {
        let mut lt = LogTest::default();

        lt.add(|mut eg: Environment| {
            eg.a.store(1, Ordering::Relaxed);
            eg.b.store(1, Ordering::Release);
            0
        });

        lt.add(|mut eg: Environment| {
            eg.c.store(1, Ordering::Relaxed);
            eg.d.store(1, Ordering::Release);
            0
        });

        lt.add(|mut eg: Environment| {
            let r0 = eg.b.load(Ordering::Relaxed);
            let r1 = eg.d.load(Ordering::Relaxed);

            if r0 == 1 || r1 == 1 {
                eg.fence(Ordering::Acquire);
            }

            let mut stale_reads = 0;

            if r0 == 1 && eg.a.load(Ordering::Relaxed) == 0 {
                stale_reads += 1;
            }

            if r1 == 1 && eg.c.load(Ordering::Relaxed) == 0 {
                stale_reads += 1;
            }

            stale_reads
        });

        lt.run()
    }

    // Thread #3 should never perceive an old value for a/c when the b/d flags are set, respectively
    assert!(run_until(inner, vec![vec![0, 0, 0]]));
}

// Listing 8.1 skipped - memlog does not support futex
