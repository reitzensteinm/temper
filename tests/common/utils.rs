use std::collections::HashSet;
use std::fmt::Debug;
use std::hash::Hash;
use std::sync::{Arc, Mutex};
use temper::temper::memory::core::{Atomic, SharedMemory};

/* Default test environment provides for four variables */

#[derive(Clone)]
#[allow(unused)]
pub struct Test {
    pub a: Arc<Atomic<usize>>,
    pub b: Arc<Atomic<usize>>,
    pub c: Arc<Atomic<usize>>,
    pub d: Arc<Atomic<usize>>,

    pub arr: Arc<SharedMemory<usize>>,

    pub results: Arc<Mutex<Vec<usize>>>,
}

impl Default for Test {
    fn default() -> Self {
        Test {
            a: Arc::new(Atomic::new(0usize)),
            b: Arc::new(Atomic::new(0usize)),
            c: Arc::new(Atomic::new(0usize)),
            d: Arc::new(Atomic::new(0usize)),
            arr: Arc::new(SharedMemory::new(1024)),
            results: Arc::new(Mutex::new(vec![])),
        }
    }
}

impl Test {
    pub fn report_result(&self, index: usize, result: usize) {
        let mut res = self.results.lock().unwrap();
        while res.len() <= index {
            res.push(0);
        }
        res[index] = result;
    }
}

fn check_set<T: Clone + Eq + Hash>(hs: &HashSet<T>, arr: &Vec<T>) -> bool {
    let mut ns = HashSet::new();
    for x in arr {
        ns.insert(x.clone());
    }
    ns == *hs
}

pub fn run_until<T: Clone + Eq + Hash + Debug, F: FnMut() -> T>(
    mut f: F,
    expected: Vec<T>,
) -> bool {
    let mut res = HashSet::new();

    for x in 0..10_000 {
        res.insert(f());

        if check_set(&res, &expected) && x > 100 {
            return true;
        }

        if res.len() > expected.len() {
            println!("Failed {:?} {:?}", res, expected);
            return false;
        }
    }

    println!("Failed {:?} {:?}", res, expected);
    false
}
