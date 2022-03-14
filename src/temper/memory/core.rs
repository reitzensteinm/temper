use rand::prelude::*;
use rand_chacha::ChaCha8Rng;
use std::borrow::Borrow;
use std::cell::UnsafeCell;
use std::ops::Deref;
use std::rc::Rc;
use std::sync::atomic::Ordering::SeqCst;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use uuid::Uuid;

#[derive(Clone)]
struct SystemInfo {
    thread: usize,
    chan: Sender<Operation>,
    parked: Arc<AtomicUsize>,
}

thread_local! {
    static SYSTEM: Mutex<Option<SystemInfo>> = Mutex::new(None);
}

#[derive(Debug)]
pub enum OperationType {
    Get,
    Set,
    Fence,
}

pub struct Operation {
    pub op: OperationType,
    thread: usize,
    location: Uuid,
    pub execute: Box<dyn Fn() + Send>,
}

impl Operation {
    pub fn blocks(&self, other: &Operation) -> bool {
        if self.thread != other.thread {
            return false;
        }

        if other.location == self.location {
            return true;
        }

        #[allow(clippy::match_like_matches_macro)]
        match (&self.op, &other.op) {
            (OperationType::Set, OperationType::Get) => false,
            _ => true,
        }
    }
}

pub struct PendingResult<T: Copy> {
    value_slot: Arc<Mutex<Option<T>>>,
    value: Rc<UnsafeCell<T>>,
    executed: Arc<AtomicBool>,
}

pub struct Atomic<T: Copy> {
    value: Arc<Mutex<T>>,
    id: Uuid,
}

pub struct SharedMemory<T: Copy> {
    arr: Vec<Atomic<T>>,
}

impl<T: Copy + Default + 'static + Send> SharedMemory<T> {
    pub fn new(len: usize) -> Self {
        SharedMemory {
            arr: (0..len).map(|_| Atomic::new(T::default())).collect(),
        }
    }

    pub fn get(&self, ind: usize) -> PendingResult<T> {
        self.arr[ind].get()
    }

    pub fn set(&self, ind: usize, val: T) -> PendingResult<T> {
        self.arr[ind].set(val)
    }
}

impl<T: Copy> Deref for PendingResult<T> {
    type Target = T;

    fn deref(&self) -> &T {
        let mut taken = false;

        while !self.executed.load(Ordering::Relaxed) {
            if !taken {
                SYSTEM.with(|v| {
                    let p = &v.borrow().lock().unwrap();
                    p.as_ref().unwrap().parked.fetch_add(1, Ordering::SeqCst);
                });
                taken = true;
            }
        }

        if taken {
            SYSTEM.with(|v| {
                let p = &v.borrow().lock().unwrap();
                p.as_ref().unwrap().parked.fetch_sub(1, Ordering::SeqCst)
            });
        }

        let v = self.value_slot.lock().unwrap();

        unsafe {
            *self.value.get() = v.unwrap();
            &*self.value.get()
        }
    }
}

impl<T: Copy + Default + 'static + Send> Atomic<T> {
    pub fn new(value: T) -> Self {
        Self {
            id: Uuid::new_v4(),
            value: Arc::new(Mutex::new(value)),
        }
    }

    pub fn queue_op<F: Fn() + Send + 'static>(id: Uuid, op_type: OperationType, op: F) {
        let op = {
            Operation {
                op: op_type,
                location: id,
                thread: SYSTEM.with(|v| v.lock().unwrap().as_ref().unwrap().thread),
                execute: Box::new(op),
            }
        };

        SYSTEM.with(|v| {
            let sys = v.borrow().lock().unwrap();

            if let Some(s) = sys.as_ref() {
                s.chan.send(op).unwrap();
            }
        });
    }

    pub fn fence() {
        Self::queue_op(Uuid::new_v4(), OperationType::Fence, move || {});
    }

    pub fn get(&self) -> PendingResult<T> {
        let value = Rc::new(UnsafeCell::new(T::default()));

        let vclone = self.value.clone();
        let executed = Arc::new(AtomicBool::new(false));
        let value_slot = Arc::new(Mutex::new(None));

        {
            let executed = executed.clone();
            let value_slot = value_slot.clone();
            Self::queue_op(self.id, OperationType::Get, move || {
                let v = *vclone.lock().unwrap();

                *value_slot.lock().unwrap() = Some(v);

                executed.store(true, Ordering::Relaxed);
            });
        }

        PendingResult {
            value,
            executed,
            value_slot,
        }
    }

    pub fn set(&self, val: T) -> PendingResult<T> {
        let value = Rc::new(UnsafeCell::new(val));

        let vclone = self.value.clone();
        let executed = Arc::new(AtomicBool::new(false));
        let value_slot = Arc::new(Mutex::new(None));

        {
            let executed = executed.clone();
            let value_slot = value_slot.clone();

            Self::queue_op(self.id, OperationType::Set, move || {
                *vclone.lock().unwrap() = val;

                *value_slot.lock().unwrap() = Some(val);

                executed.store(true, Ordering::Relaxed);
            });
        }

        PendingResult {
            value,
            executed,
            value_slot,
        }
    }
}

#[derive(Clone, Default)]
pub struct System {}

impl System {
    pub fn new() -> Self {
        Self {}
    }

    pub fn get_op(ops: &mut Vec<Operation>, ind: usize) -> Option<Operation> {
        if ops.is_empty() {
            return None;
        }

        let ind = ind % ops.len();

        for x in 0..ind {
            if ops[x].blocks(&ops[ind]) {
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
                if let Some(o) = Self::get_op(&mut operations, rng.next_u64() as usize) {
                    o.execute.as_ref()();
                }
            }
        }

        for h in handles {
            h.join().unwrap()
        }
    }
}
