use std::collections::HashSet;
use std::hash::Hash;
use std::sync::{Arc, Mutex};
use temper::temper::memory::core::{Atomic, System};

/* Default test environment provides for four variables */

#[derive(Clone)]
#[allow(unused)]
struct Test {
    a: Atomic<usize>,
    b: Atomic<usize>,
    c: Atomic<usize>,
    d: Atomic<usize>,

    results: Arc<Mutex<Vec<usize>>>,
}

impl Default for Test {
    fn default() -> Self {
        Test {
            a: Atomic::new(0usize),
            b: Atomic::new(0usize),
            c: Atomic::new(0usize),
            d: Atomic::new(0usize),
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

fn run_until<T: Clone + Eq + Hash, F: FnMut() -> T>(mut f: F, expected: Vec<T>) -> bool {
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

/* From Intel's memory model documentation

Thread 1:
a = 1
print(b)

Thread 2:
b = 1
print(a)

Can print any of (0,0) (0,1) (1,0) (1,1)
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
