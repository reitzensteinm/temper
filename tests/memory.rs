use std::collections::HashSet;
use std::hash::Hash;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::SeqCst;
use std::sync::Arc;
use tempers::temper::memory::core::{Atomic, System};

fn run_test() -> Vec<usize> {
    let s = System::new();

    #[derive(Clone)]
    struct Test {
        a: Atomic<usize>,
        b: Atomic<usize>,
        result_left: Arc<AtomicUsize>,
        result_right: Arc<AtomicUsize>,
    }

    let test = Test {
        a: Atomic::new(0usize),
        b: Atomic::new(0usize),
        result_left: Arc::new(AtomicUsize::new(0)),
        result_right: Arc::new(AtomicUsize::new(0)),
    };

    let fa = {
        let mut test = test.clone();
        move || {
            test.b.set(1);
            test.result_left.store(*test.a.get(), SeqCst);
        }
    };

    let fb = {
        let mut test = test.clone();
        move || {
            test.a.set(1);
            test.result_right.store(*test.b.get(), SeqCst);
        }
    };

    let fns: Vec<Box<dyn FnMut() + Send>> = vec![Box::new(fa), Box::new(fb)];

    s.run(fns);

    vec![
        test.result_left.load(SeqCst),
        test.result_right.load(SeqCst),
    ]
}

fn check_set<T: Clone + Eq + Hash>(hs: HashSet<T>, arr: Vec<T>) -> bool {
    let mut ns = HashSet::new();
    for x in arr {
        ns.insert(x.clone());
    }
    ns == hs
}

#[test]
fn do_test() {
    let mut res = HashSet::new();
    for _ in 0..100 {
        res.insert(run_test());
    }

    assert!(check_set(
        res,
        vec![vec![0, 0], vec![0, 1], vec![1, 0], vec![1, 1]]
    ));
}
