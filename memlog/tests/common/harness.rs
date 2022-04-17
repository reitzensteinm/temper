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
    pub position: usize,
}

pub struct Value {
    pub thread: usize,
    pub addr: usize,
    pub thread_state: Arc<Mutex<ThreadState>>,
    pub memory: Arc<Mutex<MemorySystem>>,
}

impl Value {
    pub fn wait(&mut self) {
        {
            let mut ts = self.thread_state.lock().unwrap();
            ts.waiting = true;
            let barrier = ts.barrier.clone();
            drop(ts);
            barrier.wait();
        }

        while self.thread_state.lock().unwrap().waiting {}
    }

    #[allow(unused)]
    pub fn fetch_update<F: Fn(usize) -> Option<usize>>(
        &mut self,
        f: F,
        ordering: Ordering,
    ) -> bool {
        self.wait();
        let mut mem = self.memory.lock().unwrap();
        mem.fetch_update(self.thread, self.addr, f, ordering)
            .is_ok()
    }

    #[allow(unused)]
    pub fn exchange_weak(&mut self, old: usize, new: usize, ordering: Ordering) -> bool {
        self.wait();
        let mut mem = self.memory.lock().unwrap();
        let f = |v| {
            if v == old {
                Some(new)
            } else {
                None
            }
        };
        mem.fetch_update(self.thread, self.addr, f, ordering)
            .is_ok()
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

pub struct Environment {
    pub a: Value,
    pub b: Value,
    pub c: Value,
}

impl Environment {
    #[allow(unused)]
    pub fn fence(&mut self, ordering: Ordering) {
        // Todo: This is gross!
        self.a.wait();
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
        let ts = Arc::new(Mutex::new(ThreadState {
            finished: false,
            waiting: false,
            barrier: Arc::new(Barrier::new(2)),
            position: 0,
        }));

        let env = Environment {
            a: Value {
                thread: i,
                addr: 0,
                thread_state: ts.clone(),
                memory: ms.clone(),
            },
            b: Value {
                thread: i,
                addr: 1,
                thread_state: ts.clone(),
                memory: ms.clone(),
            },
            c: Value {
                thread: i,
                addr: 2,
                thread_state: ts.clone(),
                memory: ms,
            },
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

        let mut results = vec![];

        for (i, f) in self.fns.drain(..).enumerate() {
            results.push(Self::drive(vec![Self::spawn_thread(ms.clone(), i, f)])[0]);
        }

        results
    }
}
