#![allow(clippy::ptr_arg)]

mod common;

use common::utils::{run_until, Test};

use temper::temper::memory::core::{Atomic, System};

/* From Intel's memory model documentation

Thread 1:
a = 1
print(b)

Thread 2:
b = 1
print(a)

Can print any of (0,0) (0,1) (1,0) (1,1)
If a memfence is present, (0,0) is not a valid result
*/

fn test_a(memfence: bool) -> Vec<usize> {
    let s = System::new();

    let test = Test::default();

    let fa = {
        let mut test = test.clone();
        move || {
            test.b.set(1);
            if memfence {
                Atomic::<()>::fence()
            }
            let res = *test.a.get();
            test.report_result(0, res);
        }
    };

    let fb = {
        let mut test = test.clone();
        move || {
            test.a.set(1);
            if memfence {
                Atomic::<()>::fence()
            }
            let res = *test.b.get();
            test.report_result(1, res);
        }
    };

    let fns: Vec<Box<dyn FnMut() + Send>> = vec![Box::new(fa), Box::new(fb)];

    s.run(fns);

    let tr = test.results.lock().unwrap();
    (*tr).clone()
}

#[test]
fn test_a_runner() {
    assert!(run_until(
        || test_a(false),
        vec![vec![0, 0], vec![0, 1], vec![1, 0], vec![1, 1]],
    ));

    assert!(run_until(
        || test_a(true),
        vec![vec![0, 1], vec![1, 0], vec![1, 1]],
    ));
}

fn test_queue(iters: usize) -> Vec<usize> {
    //let start = Utc::now();
    let system = System::new();
    let test = Test::default();

    let fa = {
        let mut test = test.clone();
        move || {
            for x in 0..iters {
                let i = *test.a.get();
                test.arr.set(i, x);
                test.a.set(i + 1);
            }
        }
    };

    let fb = {
        let mut test = test.clone();
        move || {
            let mut o = 0;
            for _ in 0..iters {
                let res = loop {
                    let a = test.a.get();
                    let b = test.b.get();

                    if *a > *b {
                        test.b.set(*b + 1);
                        break *test.arr.get(*b);
                    }
                };
                o += res;
            }

            test.report_result(0, o);
        }
    };

    let fns: Vec<Box<dyn FnMut() + Send>> = vec![Box::new(fa), Box::new(fb)];

    system.run(fns);

    //println!("Elapsed {}", (Utc::now() - start));

    let tr = test.results.lock().unwrap();
    (*tr).clone()
}

#[test]
fn test_queue_runner() {
    let expected = (0..5).sum();
    assert!(run_until(|| test_queue(5), vec![vec![expected]]));
}

/*use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::Arc;
use threadpool::ThreadPool;*/
#[test]
#[cfg(feature = "perf")]
fn test_queue_perf() {
    for x in 1..=16 {
        let start = Utc::now();
        let n_workers = x;
        let pool = ThreadPool::new(n_workers);

        let num = 1000;
        let fin = Arc::new(AtomicUsize::new(0));

        for _ in 0..num {
            let fin = fin.clone();
            pool.execute(move || {
                test_queue();
                fin.fetch_add(1, Relaxed);
            });
        }

        pool.join();

        while fin.load(Relaxed) != num {
            std::thread::sleep(std::time::Duration::from_millis(1));
        }

        println!("{} {}", x, Utc::now() - start);
    }
}
