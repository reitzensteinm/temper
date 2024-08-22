#![allow(clippy::ptr_arg)]

mod common;

use common::utils::{run_until, Test};

use temper::temper::memory::core::{set_model, Atomic, MemoryModel};
use temper::temper::system::core::System;

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
    set_model(MemoryModel::Intel);
    let s = System::new();

    let test = Test::default();

    let fa = {
        let test = test.clone();
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
        let test = test.clone();
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

fn test_queue(iters: usize, model: MemoryModel) -> Vec<usize> {
    //let start = Utc::now();
    set_model(model);
    let system = System::new();

    let test = Test::default();

    let fa = {
        let test = test.clone();
        move || {
            for x in 0..iters {
                let i = *test.a.get();
                test.arr.set(i, x);

                // ARM requires fence here
                if model == MemoryModel::ARM {
                    Atomic::<()>::fence();
                }

                test.a.set(i + 1);
            }
        }
    };

    let fb = {
        let test = test.clone();
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
    let size = 20;
    let expected = (0..size).sum();
    assert!(run_until(
        || test_queue(size, MemoryModel::ARM),
        vec![vec![expected]]
    ));
    assert!(run_until(
        || test_queue(size, MemoryModel::Intel),
        vec![vec![expected]]
    ));
}