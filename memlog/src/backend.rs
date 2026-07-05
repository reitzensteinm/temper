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

impl MemoryBackend for log::MemorySystem {
    fn name(&self) -> &'static str {
        "log"
    }

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

    fn fetch_op(
        &mut self,
        thread: usize,
        addr: usize,
        f: &dyn Fn(usize) -> usize,
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

    fn fetch_update(
        &mut self,
        thread: usize,
        addr: usize,
        f: &dyn Fn(usize) -> Option<usize>,
        set_order: Ordering,
        fetch_order: Ordering,
    ) -> Result<usize, usize> {
        log::MemorySystem::fetch_update(self, thread, addr, f, set_order, fetch_order)
    }
}

#[derive(Clone, Copy)]
pub struct BackendConstructor {
    name: &'static str,
    construct: fn() -> Box<dyn MemoryBackend>,
}

impl BackendConstructor {
    pub const fn new(name: &'static str, construct: fn() -> Box<dyn MemoryBackend>) -> Self {
        BackendConstructor { name, construct }
    }

    pub fn name(&self) -> &'static str {
        self.name
    }

    pub fn construct(&self) -> Box<dyn MemoryBackend> {
        (self.construct)()
    }
}

const LOG_BACKEND: BackendConstructor = BackendConstructor::new("log", construct_log_backend);

pub fn available_backends() -> &'static [BackendConstructor] {
    static BACKENDS: &[BackendConstructor] = &[LOG_BACKEND];

    BACKENDS
}

pub fn create_default() -> Box<dyn MemoryBackend> {
    LOG_BACKEND.construct()
}

fn construct_log_backend() -> Box<dyn MemoryBackend> {
    Box::new(log::MemorySystem::default())
}

pub type MemorySystem = log::MemorySystem;
