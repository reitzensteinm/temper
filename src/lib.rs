use crate::temper::memory::core::{Atomic, System};
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::Arc;

extern crate uuid;
use threadpool::ThreadPool;
pub mod temper;

#[derive(Clone)]
struct Test {
    a: Atomic<u32>,
    b: Atomic<u32>,
}

fn test_left(t: &mut Test) {
    t.b.set(1);
    Atomic::<()>::fence();
    let _res = t.a.get();
    //   println!("Got A {}", *res);
}

fn test_right(t: &mut Test) {
    t.a.set(1);
    Atomic::<()>::fence();
    let _res = t.b.get();
    // println!("Got B {}", *res);
}

fn run_test() {
    let s = System::new();

    let t = Test {
        a: Atomic::new(0),
        b: Atomic::new(0),
    };

    let mut ta = t.clone();
    let mut tb = t;

    let fns: Vec<Box<dyn FnMut() + Send>> = vec![
        Box::new(move || test_left(&mut ta)),
        Box::new(move || test_right(&mut tb)),
    ];

    s.run(fns);
}

pub fn run_bench() {
    let now = std::time::SystemTime::now();
    let n_workers = 1;
    let pool = ThreadPool::new(n_workers);

    let num = 1_000;
    let fin = Arc::new(AtomicUsize::new(0));

    for _ in 0..num {
        let fin = fin.clone();
        pool.execute(move || {
            run_test();
            fin.fetch_add(1, Relaxed);
        });
    }

    pool.join();

    while fin.load(Relaxed) != num {
        std::thread::sleep(std::time::Duration::from_millis(1));
    }

    println!("Done {:?}", now.elapsed().unwrap().as_millis());
}
