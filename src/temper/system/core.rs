use rand::{RngCore, SeedableRng};
use rand_chacha::ChaCha8Rng;
use std::any::Any;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::SeqCst;
use std::sync::mpsc::{channel, Sender};
use std::sync::Arc;
use std::sync::Mutex;
use std::thread;

#[derive(Clone)]
pub struct SystemInfo {
    pub thread: usize,
    pub chan: Sender<Operation>,
    pub parked: Arc<AtomicUsize>,
}

thread_local! {
    pub static SYSTEM: Mutex<Option<SystemInfo>> = const { Mutex::new(None) };
}

pub fn with_system<T, F: FnOnce(&SystemInfo) -> T>(f: F) -> T {
    SYSTEM.with(|a| f(a.lock().unwrap().as_ref().unwrap()))
}

pub trait Op {
    fn blocks(&self, other: &(dyn Op + Send)) -> bool;
    fn as_any(&self) -> &dyn Any;
    fn execute(&self);
}

pub struct Operation {
    pub op: Box<dyn Op + Send>,
}

impl Operation {
    pub fn build<T: 'static + Op + Send>(op: T) -> Operation {
        Operation { op: Box::new(op) }
    }

    pub fn execute(&self) {
        self.op.execute();
    }
}

#[derive(Default)]
pub struct System {}

impl System {
    pub fn new() -> Self {
        Self {}
    }

    pub fn get_op(&self, ops: &mut Vec<Operation>, ind: usize) -> Option<Operation> {
        if ops.is_empty() {
            return None;
        }

        let ind = ind % ops.len();

        for x in 0..ind {
            if ops[x].op.blocks(ops[ind].op.as_ref()) {
                return None;
            }
        }

        Some(ops.remove(ind))
    }

    pub fn run<F: FnMut() + Send + 'static + ?Sized>(self, mut fns: Vec<Box<F>>) {
        let s = std::time::UNIX_EPOCH.elapsed().unwrap().as_nanos() as u64;
        let mut rng = ChaCha8Rng::seed_from_u64(s);

        //println!("Executing with Seed {}", s);
        let mut handles = vec![];
        let finished = Arc::new(AtomicUsize::new(0));

        let (sender, receiver) = channel();

        let mut sys_info = SystemInfo {
            chan: sender,
            thread: 0,
            parked: Arc::new(AtomicUsize::new(0)),
        };

        for mut f in fns.drain(..) {
            let finished = finished.clone();

            sys_info.thread += 1;
            let sys_info = sys_info.clone();

            handles.push(thread::spawn(move || {
                SYSTEM.with(|v| *v.lock().unwrap() = Some(sys_info));
                f();
                finished.fetch_add(1, SeqCst);
            }));
        }

        let mut operations = vec![];

        while finished.load(SeqCst) < handles.len() {
            while let Ok(v) = receiver.try_recv() {
                operations.push(v);
            }

            let finished_count = finished.load(SeqCst);
            let parked_count = sys_info.parked.load(SeqCst);

            if finished_count + parked_count == handles.len() {
                if let Some(o) = self.get_op(&mut operations, rng.next_u64() as usize) {
                    o.execute();
                }
            }
        }

        for h in handles {
            h.join().unwrap()
        }
    }
}
