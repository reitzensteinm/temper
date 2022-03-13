use rand::prelude::*;
use rand_chacha::ChaCha8Rng;
use std::borrow::{Borrow, BorrowMut};
use std::cell::RefCell;
use std::cell::UnsafeCell;
use std::future::Pending;
use std::ops::Deref;
use std::rc::Rc;
use std::sync::atomic::Ordering::SeqCst;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
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

/*
pub enum OperationBody<T: Copy> {
    Get,
    Set(T),
}

pub struct Operation<T: Copy> {
    body: OperationBody<T>,
    id: Uuid,
    target: Rc<UnsafeCell<T>>,
}*/

#[derive(Debug)]
pub enum OperationType {
    Get,
    Set,
    Fence,
}

pub struct Operation {
    pub op: OperationType,
    thread: usize,
    pub execute: Box<dyn Fn() + Send>,
}

impl Operation {
    pub fn blocks(&self, other: &Operation) -> bool {
        if self.thread != other.thread {
            return false;
        }

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

#[derive(Clone)]
pub struct Atomic<T: Copy> {
    value: Arc<Mutex<T>>,
    id: Uuid,
}

impl<T: Copy> Deref for PendingResult<T> {
    type Target = T;

    fn deref(&self) -> &T {
        //let v = SYSTEM.with(|v| v.borrow().is_some());
        //println!("Got {}", v);

        let mut taken = false;

        while !self.executed.load(Ordering::Relaxed) {
            if !taken {
                SYSTEM.with(|v| {
                    v.borrow()
                        .lock()
                        .unwrap()
                        .as_ref()
                        .unwrap()
                        .parked
                        .fetch_add(1, Ordering::SeqCst)
                });
                taken = true;
            }
            //            println!("Waiting on op");
        }

        if taken {
            SYSTEM.with(|v| {
                v.borrow()
                    .lock()
                    .unwrap()
                    .as_ref()
                    .unwrap()
                    .parked
                    .fetch_sub(1, Ordering::SeqCst)
            });
        }

        //println!("Got op");

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
            value: Arc::new(Mutex::new(value)),
            id: Uuid::new_v4(),
        }
    }

    pub fn fence() {
        let op = {
            //    let executed = executed.clone();
            Operation {
                op: OperationType::Fence,
                thread: SYSTEM.with(|v| v.lock().unwrap().as_ref().unwrap().thread),
                execute: Box::new(move || {
                    println!("Fence!");
                    //          executed.store(true, Ordering::Relaxed);
                }),
            }
        };

        SYSTEM.with(|v| {
            let sys = v.borrow().lock().unwrap();

            if let Some(mut s) = sys.as_ref() {
                s.chan.send(op);
                //s.operations.lock().unwrap().push(op);
            }
        });
    }

    pub fn get(&mut self) -> PendingResult<T> {
        //self.value;

        let mut value = Rc::new(UnsafeCell::new(T::default()));

        let mut valc = value.clone();
        let mut vclone = self.value.clone();
        let executed = Arc::new(AtomicBool::new(false));
        let value_slot = Arc::new(Mutex::new(None));

        let op = {
            let executed = executed.clone();
            let value_slot = value_slot.clone();
            Operation {
                op: OperationType::Get,
                thread: SYSTEM.with(|v| v.lock().unwrap().as_ref().unwrap().thread),
                execute: Box::new(move || {
                    let v = *vclone.lock().unwrap();

                    *value_slot.lock().unwrap() = Some(v);

                    executed.store(true, Ordering::Relaxed);
                }),
            }
        };

        SYSTEM.with(|v| {
            let sys = v.borrow().lock().unwrap();

            if let Some(mut s) = sys.as_ref() {
                s.chan.send(op);
                //s.operations.lock().unwrap().push(op);
            }
        });

        PendingResult {
            value,
            executed,
            value_slot,
        }
    }

    pub fn set(&mut self, val: T) -> PendingResult<T> {
        //self.value = val;
        let value = Rc::new(UnsafeCell::new(val));

        let vclone = self.value.clone();
        let valc = value.clone();
        let executed = Arc::new(AtomicBool::new(false));
        let value_slot = Arc::new(Mutex::new(None));

        let operation = {
            let executed = executed.clone();
            let value_slot = value_slot.clone();

            Operation {
                op: OperationType::Set,
                thread: SYSTEM.with(|v| v.lock().unwrap().as_ref().unwrap().thread),
                execute: Box::new(move || {
                    *vclone.lock().unwrap() = val;

                    *value_slot.lock().unwrap() = Some(val);

                    executed.store(true, Ordering::Relaxed);
                }),
            }
        };

        SYSTEM.with(|v| {
            let sys = v.borrow().lock().unwrap();

            if let Some(mut s) = sys.as_ref() {
                s.chan.send(operation);
            }
        });

        PendingResult {
            value,
            executed,
            value_slot,
        }
    }
}

#[derive(Clone)]
pub struct System {
    // How ?
    pub operations: Arc<Mutex<Vec<Operation>>>,
}

impl System {
    pub fn new() -> Self {
        Self {
            operations: Arc::new(Mutex::new(vec![])),
        }
    }

    pub fn get_op(ops: &mut Vec<Operation>, ind: usize) -> Option<Operation> {
        if ops.len() == 0 {
            return None;
        }

        let ind = ind % ops.len();

        for x in 0..ind {
            if ops[x].blocks(&ops[ind]) {
                /*
                println!(
                    "Blocks! {} {} {:?} {:?} {} {}",
                    x, ind, ops[x].op, ops[ind].op, ops[x].thread, ops[ind].thread
                );*/
                return None;
            }
            /*
            println!(
                "Checking block {} {:?} {:?} {} {}",
                x, ops[x].op, ops[ind].op, ops[x].thread, ops[ind].thread
            );*/
        }

        Some(ops.remove(ind))
    }

    pub fn run<F: FnMut() + Send + 'static + ?Sized>(self, mut fns: Vec<Box<F>>) {
        let s = std::time::UNIX_EPOCH.elapsed().unwrap().as_nanos() as u64;
        let mut rng = ChaCha8Rng::seed_from_u64(s);

        //println!("Executing with Seed {}", s);
        let mut handles = vec![];
        let mut finished = Arc::new(Mutex::new(0));

        let (sender, receiver) = channel();

        let mut sys_info = SystemInfo {
            chan: sender,
            thread: 0,
            parked: Arc::new(AtomicUsize::new(0)),
        };

        for mut f in fns.drain(..) {
            let system = self.clone();

            let mut finished = finished.clone();

            sys_info.thread += 1;
            let mut sys_info = sys_info.clone();

            handles.push(thread::spawn(move || {
                SYSTEM.with(|v| *v.lock().unwrap() = Some(sys_info));
                f();
                *finished.lock().unwrap() += 1;
            }));
        }

        while *finished.lock().unwrap() < handles.len() {
            while let Ok(v) = receiver.try_recv() {
                self.operations.lock().unwrap().push(v);
            }

            let finished_count = *finished.lock().unwrap();
            let parked_count = sys_info.parked.load(SeqCst);

            if finished_count + parked_count == handles.len() {
                //println!("Ready to progress {} {}", finished_count, parked_count);

                let mut ops = self.operations.lock().unwrap();

                if let Some(o) = Self::get_op(&mut ops, rng.next_u64() as usize) {
                    drop(ops);
                    o.execute.as_ref()();
                }
            }
        }

        for h in handles {
            h.join().unwrap()
        }
    }
}
