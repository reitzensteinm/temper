use std::sync::atomic::{AtomicU32, Ordering};

// Assembly tests for Godbolt

// --target=arm-unknown-linux-gnueabihf

pub fn store_release(x: &AtomicU32) {
    x.store(1, Ordering::Release)
}

pub fn store_seq(x: &AtomicU32) {
    x.store(1, Ordering::SeqCst)
}

pub fn load_release(x: &AtomicU32) -> u32 {
    x.load(Ordering::Acquire)
}

pub fn load_seqcst(x: &AtomicU32) -> u32 {
    x.load(Ordering::SeqCst)
}

pub fn twostore_release(x: &AtomicU32, y: &AtomicU32) {
    x.store(1, Ordering::Relaxed);
    y.store(1, Ordering::Release)
}

pub fn twostore_seq(x: &AtomicU32, y: &AtomicU32) {
    x.store(1, Ordering::SeqCst);
    y.store(1, Ordering::SeqCst)
}

pub fn twoload_set_seq(x: &AtomicU32, y: &AtomicU32) {
    x.store(1, Ordering::SeqCst)
}

pub fn twoload_set_release(x: &AtomicU32, y: &AtomicU32) {
    y.store(1, Ordering::Release)
}

pub fn twoload_release(x: &AtomicU32, y: &AtomicU32) -> u32 {
    while x.load(Ordering::Acquire) == 0 {}
    y.load(Ordering::Acquire)
}

pub fn twoload_seqcst(x: &AtomicU32, y: &AtomicU32) -> u32 {
    while x.load(Ordering::SeqCst) == 0 {}
    y.load(Ordering::SeqCst)
}

pub fn barrier_loadstore(x: &AtomicU32, y: &AtomicU32) {
    x.store(1, Ordering::Relaxed);
    std::sync::atomic::fence(Ordering::AcqRel);
    y.store(1, Ordering::Relaxed)
}

pub fn barrier_seq(x: &AtomicU32, y: &AtomicU32) {
    x.store(1, Ordering::Relaxed);
    std::sync::atomic::fence(Ordering::SeqCst);
    y.store(1, Ordering::Relaxed)
}

pub fn two_acqrel(x: &AtomicU32, y: &AtomicU32) {
    x.load(Ordering::Acquire);
    y.store(1, Ordering::Release)
}

pub fn two_fence(x: &AtomicU32, y: &AtomicU32) {
    std::sync::atomic::fence(Ordering::AcqRel);
    x.load(Ordering::Relaxed);
    y.store(1, Ordering::Relaxed);
    std::sync::atomic::fence(Ordering::AcqRel);
}

pub fn two_fence_seq(x: &AtomicU32, y: &AtomicU32) {
    std::sync::atomic::fence(Ordering::SeqCst);
    x.load(Ordering::Relaxed);
    y.store(1, Ordering::Relaxed);
    std::sync::atomic::fence(Ordering::SeqCst);
}

pub fn two_load_acq(x: &AtomicU32, y: &AtomicU32) {
    x.load(Ordering::Acquire);
    y.load(Ordering::Acquire);
}
pub fn two_load_seq(x: &AtomicU32, y: &AtomicU32) {
    x.load(Ordering::SeqCst);
    y.load(Ordering::SeqCst);
}
