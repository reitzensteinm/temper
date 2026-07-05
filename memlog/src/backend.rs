#[cfg(feature = "graph")]
use crate::graph;
use crate::log;
use std::sync::atomic::Ordering;

pub trait MemoryBackend: Send {
    fn name(&self) -> &'static str;

    fn add_thread(&mut self) -> usize;
    fn malloc(&mut self, size: usize) -> usize;
    fn load(&mut self, thread: usize, addr: usize, level: Ordering) -> usize;
    fn store(&mut self, thread: usize, addr: usize, val: usize, level: Ordering);
    fn fence(&mut self, thread: usize, level: Ordering);

    fn fetch_op(
        &mut self,
        thread: usize,
        addr: usize,
        f: &dyn Fn(usize) -> usize,
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

    fn fetch_update(
        &mut self,
        thread: usize,
        addr: usize,
        f: &dyn Fn(usize) -> Option<usize>,
        set_order: Ordering,
        fetch_order: Ordering,
    ) -> Result<usize, usize>;
}

macro_rules! impl_memory_backend {
    ($module:ident, $name:literal) => {
        impl MemoryBackend for $module::MemorySystem {
            fn name(&self) -> &'static str {
                $name
            }

            fn add_thread(&mut self) -> usize {
                $module::MemorySystem::add_thread(self)
            }

            fn malloc(&mut self, size: usize) -> usize {
                $module::MemorySystem::malloc(self, size)
            }

            fn load(&mut self, thread: usize, addr: usize, level: Ordering) -> usize {
                $module::MemorySystem::load(self, thread, addr, level)
            }

            fn store(&mut self, thread: usize, addr: usize, val: usize, level: Ordering) {
                $module::MemorySystem::store(self, thread, addr, val, level);
            }

            fn fence(&mut self, thread: usize, level: Ordering) {
                $module::MemorySystem::fence(self, thread, level);
            }

            fn fetch_op(
                &mut self,
                thread: usize,
                addr: usize,
                f: &dyn Fn(usize) -> usize,
                level: Ordering,
            ) -> usize {
                $module::MemorySystem::fetch_op(self, thread, addr, f, level)
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
                $module::MemorySystem::compare_exchange(
                    self, thread, addr, current, new, success, failure,
                )
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
                $module::MemorySystem::compare_exchange_weak(
                    self, thread, addr, current, new, success, failure,
                )
            }

            fn fetch_update(
                &mut self,
                thread: usize,
                addr: usize,
                f: &dyn Fn(usize) -> Option<usize>,
                set_order: Ordering,
                fetch_order: Ordering,
            ) -> Result<usize, usize> {
                $module::MemorySystem::fetch_update(self, thread, addr, f, set_order, fetch_order)
            }
        }
    };
}

impl_memory_backend!(log, "log");

#[cfg(feature = "graph")]
impl_memory_backend!(graph, "graph");

pub fn create_all() -> Vec<Box<dyn MemoryBackend>> {
    let mut backends: Vec<Box<dyn MemoryBackend>> = vec![Box::new(log::MemorySystem::default())];
    add_optional_backends(&mut backends);

    backends
}

#[cfg(feature = "graph")]
fn add_optional_backends(backends: &mut Vec<Box<dyn MemoryBackend>>) {
    backends.push(Box::new(graph::MemorySystem::default()));
}

#[cfg(not(feature = "graph"))]
fn add_optional_backends(_backends: &mut Vec<Box<dyn MemoryBackend>>) {}

#[cfg(feature = "graph")]
pub fn create_default() -> Box<dyn MemoryBackend> {
    Box::new(graph::MemorySystem::default())
}

#[cfg(not(feature = "graph"))]
pub fn create_default() -> Box<dyn MemoryBackend> {
    Box::new(log::MemorySystem::default())
}

pub type MemorySystem = log::MemorySystem;
