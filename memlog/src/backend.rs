use crate::log;
use std::sync::atomic::Ordering;

pub trait MemoryBackend: Default {
    const NAME: &'static str;

    fn add_thread(&mut self) -> usize;
    fn malloc(&mut self, size: usize) -> usize;
    fn load(&mut self, thread: usize, addr: usize, level: Ordering) -> usize;
    fn store(&mut self, thread: usize, addr: usize, val: usize, level: Ordering);
    fn fence(&mut self, thread: usize, level: Ordering);

    fn fetch_op<F: Fn(usize) -> usize>(
        &mut self,
        thread: usize,
        addr: usize,
        f: F,
        level: Ordering,
    ) -> usize;

    fn compare_exchange(
        &mut self,
        thread: usize,
        addr: usize,
        current: usize,
        new: usize,
        success: Ordering,
        failure: Ordering,
    ) -> Result<usize, usize>;

    fn compare_exchange_weak(
        &mut self,
        thread: usize,
        addr: usize,
        current: usize,
        new: usize,
        success: Ordering,
        failure: Ordering,
    ) -> Result<usize, usize>;

    fn fetch_update<F: Fn(usize) -> Option<usize>>(
        &mut self,
        thread: usize,
        addr: usize,
        f: F,
        set_order: Ordering,
        fetch_order: Ordering,
    ) -> Result<usize, usize>;
}

impl MemoryBackend for log::MemorySystem {
    const NAME: &'static str = "log";

    fn add_thread(&mut self) -> usize {
        log::MemorySystem::add_thread(self)
    }

    fn malloc(&mut self, size: usize) -> usize {
        log::MemorySystem::malloc(self, size)
    }

    fn load(&mut self, thread: usize, addr: usize, level: Ordering) -> usize {
        log::MemorySystem::load(self, thread, addr, level)
    }

    fn store(&mut self, thread: usize, addr: usize, val: usize, level: Ordering) {
        log::MemorySystem::store(self, thread, addr, val, level);
    }

    fn fence(&mut self, thread: usize, level: Ordering) {
        log::MemorySystem::fence(self, thread, level);
    }

    fn fetch_op<F: Fn(usize) -> usize>(
        &mut self,
        thread: usize,
        addr: usize,
        f: F,
        level: Ordering,
    ) -> usize {
        log::MemorySystem::fetch_op(self, thread, addr, f, level)
    }

    fn compare_exchange(
        &mut self,
        thread: usize,
        addr: usize,
        current: usize,
        new: usize,
        success: Ordering,
        failure: Ordering,
    ) -> Result<usize, usize> {
        log::MemorySystem::compare_exchange(self, thread, addr, current, new, success, failure)
    }

    fn compare_exchange_weak(
        &mut self,
        thread: usize,
        addr: usize,
        current: usize,
        new: usize,
        success: Ordering,
        failure: Ordering,
    ) -> Result<usize, usize> {
        log::MemorySystem::compare_exchange_weak(self, thread, addr, current, new, success, failure)
    }

    fn fetch_update<F: Fn(usize) -> Option<usize>>(
        &mut self,
        thread: usize,
        addr: usize,
        f: F,
        set_order: Ordering,
        fetch_order: Ordering,
    ) -> Result<usize, usize> {
        log::MemorySystem::fetch_update(self, thread, addr, f, set_order, fetch_order)
    }
}

pub type MemorySystem = log::MemorySystem;

pub const NAME: &str = <MemorySystem as MemoryBackend>::NAME;
