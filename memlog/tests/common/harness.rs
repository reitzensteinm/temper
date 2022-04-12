use memlog::log::MemorySystem;
use rand::{RngCore, SeedableRng};
use rand_chacha::ChaCha8Rng;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Barrier, Mutex};
use std::thread;

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
    pub fn fetch_modify<F: Fn(usize) -> usize>(&mut self, f: F, ordering: Ordering) {
        self.wait();
        let mut mem = self.memory.lock().unwrap();
        mem.fetch_modify_old(self.thread, self.addr, f, ordering)
    }

    #[allow(unused)]
    pub fn exchange_weak(&mut self, old: usize, new: usize, ordering: Ordering) -> bool {
        self.wait();
        let mut mem = self.memory.lock().unwrap();
        mem.exchange_old(self.thread, self.addr, old, new, ordering)
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

#[derive(Default)]
pub struct LogTest<T: Copy + Send + 'static> {
    pub fns: Vec<Box<dyn FnMut(Environment) -> T + Send>>,
}

impl<T: Copy + Send + 'static> LogTest<T> {
    pub fn add<F: FnMut(Environment) -> T + Send + 'static + Sized>(&mut self, f: F) {
        self.fns.push(Box::new(f))
    }

    pub fn run(&mut self) -> Vec<T> {
        let s = std::time::UNIX_EPOCH.elapsed().unwrap().as_nanos() as u64;
        let mut rng = ChaCha8Rng::seed_from_u64(s);

        let mut handles = vec![];
        let ms = Arc::new(Mutex::new(MemorySystem::default()));
        let mut threads = vec![];
        for (i, mut f) in self.fns.drain(..).enumerate() {
            let ts = Arc::new(Mutex::new(ThreadState {
                finished: false,
                waiting: false,
                barrier: Arc::new(Barrier::new(2)),
                position: 0,
            }));

            threads.push(ts.clone());

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
                    memory: ms.clone(),
                },
            };

            handles.push(thread::spawn(move || {
                let res: T = f(env);
                ts.lock().unwrap().finished = true;
                res
            }));
        }

        loop {
            let mut all_finished = true;
            let mut all_waiting = true;
            for tsm in threads.iter() {
                let ts = tsm.lock().unwrap();
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
                let mut l = r.lock().unwrap();
                if l.waiting {
                    l.waiting = false;
                    l.barrier.wait();
                }
            }
        }

        let mut res = vec![];

        for h in handles.drain(..) {
            res.push(h.join().unwrap())
        }

        res
    }
}
