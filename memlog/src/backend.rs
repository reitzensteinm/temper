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

pub fn rmw_orderings(success: Ordering) -> (Ordering, Ordering) {
    match success {
        Ordering::AcqRel => (Ordering::Acquire, Ordering::Release),
        Ordering::Acquire => (Ordering::Acquire, Ordering::Relaxed),
        Ordering::Release => (Ordering::Relaxed, Ordering::Release),
        other => (other, other),
    }
}

pub fn assert_valid_failure_order(level: Ordering) {
    assert!(matches!(
        level,
        Ordering::Relaxed | Ordering::Acquire | Ordering::SeqCst
    ));
}

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
