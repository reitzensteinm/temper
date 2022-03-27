use memlog::log::MemorySystem;
use std::sync::atomic::Ordering;

mod common;

#[test]
fn test_system() {
    let mut ms = MemorySystem::default();
    for x in 0..=5 {
        ms.store(0, 0, x, Ordering::Relaxed);
    }

    let mut last = None;
    for _x in 0..5 {
        let v = ms.load(1, 0, Ordering::Relaxed);

        if let Some(x) = last {
            assert!(v >= x);
        }

        println!("Got {}", v);

        last = Some(v);
    }

    /*

    for x in 0..5 {
        ms.store(0, 0, x, MemoryLevel::Relaxed);
    }

    //ms.store(0, 1, 1, MemoryLevel::Relaxed);
    let v = ms.load(1, 0, MemoryLevel::Relaxed);

    println!("{}", v);*/
}
