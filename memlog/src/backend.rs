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

    fn swap(&mut self, thread: usize, addr: usize, new: usize, level: Ordering) -> usize {
        self.fetch_op(thread, addr, &|_| new, level)
    }

    fn compare_exchange(
        &mut self,
        thread: usize,
        addr: usize,
        current: usize,
        new: usize,
        success: Ordering,
        failure: Ordering,
    ) -> Result<usize, usize>;

    /// Coin flip deciding whether compare_exchange_weak fails spuriously.
    fn spurious_failure(&mut self) -> bool;

    fn compare_exchange_weak(
        &mut self,
        thread: usize,
        addr: usize,
        current: usize,
        new: usize,
        success: Ordering,
        failure: Ordering,
    ) -> Result<usize, usize> {
        assert_valid_failure_order(failure);

        if self.spurious_failure() {
            Err(self.load(thread, addr, failure))
        } else {
            self.compare_exchange(thread, addr, current, new, success, failure)
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allocations_have_disjoint_addresses() {
        for mut memory in create_all() {
            let first = memory.malloc(1);
            let second = memory.malloc(1);
            let thread = memory.add_thread();

            memory.store(thread, first, 1, Ordering::SeqCst);
            assert_eq!(memory.load(thread, first, Ordering::SeqCst), 1);
            assert_eq!(memory.load(thread, second, Ordering::SeqCst), 0);
        }
    }

    #[test]
    fn swap_replaces_value_for_all_rmw_orderings() {
        let orderings = [
            Ordering::Relaxed,
            Ordering::Acquire,
            Ordering::Release,
            Ordering::AcqRel,
            Ordering::SeqCst,
        ];

        for mut memory in create_all() {
            let address = memory.malloc(1);
            let thread = memory.add_thread();
            memory.store(thread, address, 1, Ordering::SeqCst);

            for (index, ordering) in orderings.into_iter().enumerate() {
                assert_eq!(
                    memory.swap(thread, address, index + 2, ordering),
                    index + 1,
                    "{} backend with {ordering:?}",
                    memory.name()
                );
            }
        }
    }

    fn load_samples(mut memory: impl MemoryBackend) -> Vec<usize> {
        let address = memory.malloc(1);
        let writer = memory.add_thread();
        memory.store(writer, address, 1, Ordering::Relaxed);
        memory.store(writer, address, 2, Ordering::Relaxed);

        (0..16)
            .map(|_| {
                let reader = memory.add_thread();
                memory.load(reader, address, Ordering::Relaxed)
            })
            .collect()
    }

    #[test]
    fn seeded_backends_repeat_choices() {
        assert_eq!(
            load_samples(log::MemorySystem::default().with_seed(7)),
            load_samples(log::MemorySystem::default().with_seed(7))
        );

        #[cfg(feature = "graph")]
        assert_eq!(
            load_samples(graph::MemorySystem::default().with_seed(7)),
            load_samples(graph::MemorySystem::default().with_seed(7))
        );
    }
}
