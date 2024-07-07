use std::collections::HashSet;
use std::fmt::Debug;
use std::hash::Hash;
use std::sync::atomic::Ordering;

#[allow(unused)]
pub const ALL_ORDERINGS: [Ordering; 5] = [
    Ordering::Relaxed,
    Ordering::SeqCst,
    Ordering::Acquire,
    Ordering::Release,
    Ordering::AcqRel,
];

#[allow(unused)]
fn check_set<T: Clone + Eq + Hash>(hs: &HashSet<T>, arr: &[T]) -> bool {
    let mut ns = HashSet::new();
    for x in arr {
        ns.insert(x.clone());
    }
    ns == *hs
}

#[allow(unused)]
pub fn run_until<T: Clone + Eq + Hash + Debug, F: FnMut() -> T>(
    mut f: F,
    expected: Vec<T>,
) -> bool {
    let mut res = HashSet::new();

    for x in 0..10_000 {
        res.insert(f());

        if check_set(&res, &expected) && x > 200 {
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

// Todo: is run until pred really what we want here? Probably more like verify_holds_true
#[allow(unused)]
pub fn run_until_pred<
    T: Clone + Eq + Hash + Debug,
    F: FnMut() -> T,
    FP: Fn(&HashSet<T>) -> bool,
>(
    mut f: F,
    verify: FP,
) -> bool {
    let mut res = HashSet::new();

    for x in 0..300 {
        res.insert(f());

        if verify(&res) && x > 200 {
            return true;
        }
    }

    println!("Failed {:?}", res);
    false
}

pub fn permutations(possible: Vec<Vec<usize>>) -> Vec<Vec<usize>> {
    let mut out = vec![vec![]];

    for x in possible {
        let mut nout = vec![];

        for v in x {
            for o in out.iter() {
                let mut new_val = o.clone();
                new_val.push(v);
                nout.push(new_val);
            }
        }

        out = nout;
    }

    out
}

fn sorted<T: Clone + Ord>(mut v: Vec<T>) -> Vec<T> {
    v.sort();
    v
}

#[test]
fn test_permutations() {
    assert_eq!(
        sorted(permutations(vec![vec![0, 1], vec![1, 2]])),
        sorted(vec![vec![0, 1], vec![0, 2], vec![1, 1], vec![1, 2]])
    );

    assert_eq!(
        sorted(permutations(vec![vec![0, 1, 2, 3]])),
        sorted(vec![vec![0], vec![1], vec![2], vec![3]])
    );
}
