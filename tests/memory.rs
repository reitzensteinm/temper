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
