use std::collections::HashSet;
use std::fmt::Debug;
use std::hash::Hash;

fn check_set<T: Clone + Eq + Hash>(hs: &HashSet<T>, arr: &[T]) -> bool {
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
