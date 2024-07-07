use memlog::log::MemorySystem;
use rand::{RngCore, SeedableRng};
use rand_chacha::ChaCha8Rng;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Barrier, Mutex};
use std::thread;
use std::thread::JoinHandle;

pub struct ThreadState {
    pub finished: bool,
    pub waiting: bool,
    pub barrier: Arc<Barrier>,
}

impl ThreadState {
    pub fn wait(thread_state: &Arc<Mutex<ThreadState>>) {
        {
            let mut ts = thread_state.lock().unwrap();
            ts.waiting = true;
            let barrier = ts.barrier.clone();
            drop(ts);
            barrier.wait();
        }

        while thread_state.lock().unwrap().waiting {}
    }
}

pub struct Value {
    pub thread: usize,
    pub addr: usize,
    pub thread_state: Arc<Mutex<ThreadState>>,
    pub memory: Arc<Mutex<MemorySystem>>,
}

impl Value {
    pub fn wait(&mut self) {
        ThreadState::wait(&self.thread_state);
    }

    #[allow(unused)]
    pub fn fetch_update<F: Fn(usize) -> Option<usize>>(
        &mut self,
        f: F,
        set_order: Ordering,
        fetch_order: Ordering,
    ) -> Result<usize, usize> {
        self.wait();
        let mut mem = self.memory.lock().unwrap();
        mem.fetch_update(self.thread, self.addr, f, set_order, fetch_order)
    }

    #[allow(unused)]
    pub fn exchange_weak(
        &mut self,
        old: usize,
        new: usize,
        success: Ordering,
        failure: Ordering,
    ) -> Result<usize, usize> {
        self.wait();
        let mut mem = self.memory.lock().unwrap();

        mem.compare_exchange_weak(self.thread, self.addr, old, new, success, failure)
    }

    #[allow(unused)]
    pub fn exchange(
        &mut self,
        old: usize,
        new: usize,
        success: Ordering,
        failure: Ordering,
    ) -> Result<usize, usize> {
        self.wait();
        let mut mem = self.memory.lock().unwrap();

        mem.compare_exchange(self.thread, self.addr, old, new, success, failure)
    }

    // Used for fetch_add, fetch_sub, etc
    #[allow(unused)]
    pub fn fetch_op<F: Fn(usize) -> usize>(&mut self, f: F, ordering: Ordering) -> usize {
        self.wait();
        let mut mem = self.memory.lock().unwrap();
        mem.fetch_op(self.thread, self.addr, f, ordering)
    }

    pub fn load(&mut self, ordering: Ordering) -> usize {
        self.wait();
        let mut mem = self.memory.lock().unwrap();
        mem.load(self.thread, self.addr, ordering)
    }

    pub fn store(&mut self, val: usize, ordering: Ordering) {
        self.wait();
        let mut mem = self.memory.lock().unwrap();
        mem.store(self.thread, self.addr, val, ordering);
    }
}

#[allow(unused)]
pub struct Environment {
    pub thread_state: Arc<Mutex<ThreadState>>,
    pub a: Value,
    pub b: Value,
    pub c: Value,
    pub d: Value,
    pub e: Value,
    pub arr: Vec<Value>,
}

impl Environment {
    #[allow(unused)]
    pub fn fence(&mut self, ordering: Ordering) {
        ThreadState::wait(&self.thread_state);
        let mut mem = self.a.memory.lock().unwrap();
        mem.fence(self.a.thread, ordering)
    }
}

pub struct Thread<T> {
    pub thread_state: Arc<Mutex<ThreadState>>,
    pub handle: JoinHandle<T>,
}

#[derive(Default)]
pub struct LogTest<T: Copy + Send + 'static> {
    pub fns: Vec<Box<dyn FnMut(Environment) -> T + Send>>,
}

impl<T: Copy + Send + 'static> LogTest<T> {
    pub fn add<F: FnMut(Environment) -> T + Send + 'static + Sized>(&mut self, f: F) {
        self.fns.push(Box::new(f))
    }

    pub fn spawn_thread<F: FnMut(Environment) -> T + Send + 'static + Sized>(
        ms: Arc<Mutex<MemorySystem>>,
        i: usize,
        mut f: F,
    ) -> Thread<T> {
        ms.lock().unwrap().add_thread();

        let ts = Arc::new(Mutex::new(ThreadState {
            finished: false,
            waiting: false,
            barrier: Arc::new(Barrier::new(2)),
        }));

        let mut addr = 0;

        let mut build_value = || {
            let res = Value {
                thread: i,
                addr,
                thread_state: ts.clone(),
                memory: ms.clone(),
            };

            addr += 1;
            res
        };

        let env = Environment {
            thread_state: ts.clone(),
            a: build_value(),
            b: build_value(),
            c: build_value(),
            d: build_value(),
            e: build_value(),
            arr: (0..100).map(|_| build_value()).collect(),
        };

        Thread {
            thread_state: ts.clone(),
            handle: thread::spawn(move || {
                let res: T = f(env);
                ts.lock().unwrap().finished = true;
                res
            }),
        }
    }

    pub fn drive(mut threads: Vec<Thread<T>>) -> Vec<T> {
        let s = std::time::UNIX_EPOCH.elapsed().unwrap().as_nanos() as u64;
        let mut rng = ChaCha8Rng::seed_from_u64(s);

        loop {
            let mut all_finished = true;
            let mut all_waiting = true;
            for tsm in threads.iter() {
                let ts = tsm.thread_state.lock().unwrap();
                if !ts.finished {
                    all_finished = false;

                    if !ts.waiting {
                        all_waiting = false;
                    }
                }
            }

            if all_finished {
                break;
            }

            if all_waiting {
                let ind = (rng.next_u32() as usize) % threads.len();
                let r = &mut threads[ind];
                let mut l = r.thread_state.lock().unwrap();
                if l.waiting {
                    l.waiting = false;
                    l.barrier.wait();
                }
            }
        }

        let mut res = vec![];

        for h in threads.drain(..) {
            res.push(h.handle.join().unwrap())
        }

        res
    }

    // Runs all threads randomly interleaved
    #[allow(unused)]
    pub fn run(&mut self) -> Vec<T> {
        let ms = Arc::new(Mutex::new(MemorySystem::default()));
        ms.lock().unwrap().malloc(105);

        let mut threads = vec![];

        for (i, f) in self.fns.drain(..).enumerate() {
            threads.push(Self::spawn_thread(ms.clone(), i, f));
        }

        Self::drive(threads)
    }

    // Runs Thread A fully, then Thread B, etc
    #[allow(unused)]
    pub fn run_sequential(&mut self) -> Vec<T> {
        let ms = Arc::new(Mutex::new(MemorySystem::default()));
        ms.lock().unwrap().malloc(5);

        let mut results = vec![];

        for (i, f) in self.fns.drain(..).enumerate() {
            results.push(Self::drive(vec![Self::spawn_thread(ms.clone(), i, f)])[0]);
        }

        results
    }
}
