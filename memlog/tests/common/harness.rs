use memlog::log::MemorySystem;
use rand::{RngCore, SeedableRng};
use rand_chacha::ChaCha8Rng;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::thread;

pub struct ThreadState {
    pub finished: bool,
    pub waiting: bool,
    pub position: usize,
    pub authority: usize,
}

pub struct Value {
    pub thread: usize,
    pub addr: usize,
    pub thread_state: Arc<Mutex<ThreadState>>,
    pub memory: Arc<Mutex<MemorySystem>>,
}

impl Value {
    pub fn wait(&mut self) {
        self.thread_state.lock().unwrap().waiting = true;
        while self.thread_state.lock().unwrap().waiting {}
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
}

#[derive(Default)]
pub struct LogTest {}

impl LogTest {
    pub fn run<F: FnMut(Environment) + Send + 'static + ?Sized>(self, mut fns: Vec<Box<F>>) {
        let s = std::time::UNIX_EPOCH.elapsed().unwrap().as_nanos() as u64;
        let mut rng = ChaCha8Rng::seed_from_u64(s);

        let mut handles = vec![];
        let ms = Arc::new(Mutex::new(MemorySystem::default()));
        let mut threads = vec![];
        for (i, mut f) in fns.drain(..).enumerate() {
            let ts = Arc::new(Mutex::new(ThreadState {
                finished: false,
                waiting: false,
                position: 0,
                authority: 0,
            }));

            threads.push(ts.clone());

            let env = Environment {
                a: Value {
                    thread: i,
                    addr: 0,
                    thread_state: ts.clone(),
                    memory: ms.clone(),
                },
            };

            handles.push(thread::spawn(move || {
                f(env);
                ts.lock().unwrap().finished = true;
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
                    l.authority += 1;
                    l.waiting = false;
                }
            }
        }

        for h in handles.drain(..) {
            h.join().unwrap();
        }
    }
}
