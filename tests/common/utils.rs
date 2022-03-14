use std::collections::HashSet;
use std::hash::Hash;
use std::sync::{Arc, Mutex};
use temper::temper::memory::core::{Atomic, SharedMemory};

/* Default test environment provides for four variables */

#[derive(Clone)]
#[allow(unused)]
pub struct Test {
    pub a: Atomic<usize>,
    pub b: Atomic<usize>,
    pub c: Atomic<usize>,
    pub d: Atomic<usize>,

    pub arr: SharedMemory<usize>,

    pub results: Arc<Mutex<Vec<usize>>>,
}

impl Default for Test {
    fn default() -> Self {
        Test {
            a: Atomic::new(0usize),
            b: Atomic::new(0usize),
            c: Atomic::new(0usize),
            d: Atomic::new(0usize),
            arr: SharedMemory::new(1024),
            results: Arc::new(Mutex::new(vec![])),
        }
    }
}

impl Test {
    pub fn report_result(&mut self, index: usize, result: usize) {
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

pub fn run_until<T: Clone + Eq + Hash, F: FnMut() -> T>(mut f: F, expected: Vec<T>) -> bool {
    let mut res = HashSet::new();

    for _x in 0..10_000 {
        res.insert(f());

        if check_set(&res, &expected) {
            //println!("Took {}", x);
            return true;
        }
    }

    false
}
