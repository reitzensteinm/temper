use crate::temper::system::core::{with_system, Op, Operation};
use std::any::Any;
use std::cell::UnsafeCell;
use std::ops::Deref;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

thread_local! {
    pub static MODEL: Mutex<Option<MemoryModel>> = Mutex::new(None);
}

pub fn get_model() -> Option<MemoryModel> {
    MODEL.with(|v| *v.lock().unwrap())
}

pub fn set_model(model: MemoryModel) {
    MODEL.with(|v| *v.lock().unwrap() = Some(model))
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum MemoryOpType {
    Get,
    Set,
    Fence,
}

pub struct MemoryOp {
    pub op: MemoryOpType,
    thread: usize,
    location: Uuid,
    pub func: Box<dyn Fn() + Send>,
}

impl Op for MemoryOp {
    fn blocks(&self, other: &(dyn Op + Send)) -> bool {
        if let Some(other) = other.as_any().downcast_ref::<MemoryOp>() {
            self.blocks(other, get_model().unwrap())
        } else {
            false
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn execute(&self) {
        (self.func)()
    }
}

impl MemoryOp {
    pub fn blocks(&self, other: &MemoryOp, model: MemoryModel) -> bool {
        let standard_op = |a| a == MemoryOpType::Set || a == MemoryOpType::Get;

        if self.thread != other.thread {
            return false;
        }

        if other.location == self.location {
            return true;
        }

        if model == MemoryModel::ARM && standard_op(self.op) && standard_op(other.op) {
            return false;
        }

        #[allow(clippy::match_like_matches_macro)]
        match (&self.op, &other.op) {
            (MemoryOpType::Set, MemoryOpType::Get) => false,
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
                with_system(|s| s.parked.fetch_add(1, Ordering::SeqCst));
                taken = true;
            }
        }

        if taken {
            with_system(|s| s.parked.fetch_sub(1, Ordering::SeqCst));
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

    pub fn queue_op<F: Fn() + Send + 'static>(id: Uuid, op_type: MemoryOpType, op: F) {
        let op = {
            Operation::build(MemoryOp {
                op: op_type,
                location: id,
                thread: with_system(|s| s.thread),
                func: Box::new(op),
            })
        };

        with_system(move |s| s.chan.send(op).unwrap());
    }

    pub fn fence() {
        Self::queue_op(Uuid::new_v4(), MemoryOpType::Fence, move || {});
    }

    pub fn get(&self) -> PendingResult<T> {
        let value = Rc::new(UnsafeCell::new(T::default()));

        let vclone = self.value.clone();
        let executed = Arc::new(AtomicBool::new(false));
        let value_slot = Arc::new(Mutex::new(None));

        {
            let executed = executed.clone();
            let value_slot = value_slot.clone();
            Self::queue_op(self.id, MemoryOpType::Get, move || {
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

            Self::queue_op(self.id, MemoryOpType::Set, move || {
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

#[derive(Copy, Clone, PartialEq)]
pub enum MemoryModel {
    ARM,
    Intel,
}

/*


#[derive(Clone)]
pub struct MemorySystem {
    model: MemoryModel,
}

impl MemorySystem {
    pub fn new(model: MemoryModel) -> Self {
        Self { model }
    }
}
*/
