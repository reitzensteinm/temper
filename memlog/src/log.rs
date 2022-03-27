//pub struct MemoryThread {}

//use crate::temper::memory::core::{MemoryOp, MemoryOpType};

/*
pub enum MemoryLevel {
    Relaxed,
    Acquire,
    Release,
    AcqRel,
    SeqCst,
}*/

use std::sync::atomic::Ordering;

pub enum OperationType {
    Store(usize, usize),
    //Load(usize, usize),
    Fence,
}

pub struct MemoryOperation {
    pub thread: usize,
    pub thread_sequence: usize,
    pub global_sequence: usize,
    pub level: Ordering,
    pub op: OperationType,
}

//pub struct MemoryCell {}

pub struct MemorySystem {
    pub thread_sequence: Vec<usize>,
    pub global_sequence: usize,
    pub log: Vec<MemoryOperation>,
}

impl MemorySystem {
    pub fn store(&mut self, thread: usize, addr: usize, val: usize, level: Ordering) {
        self.global_sequence += 1;
        self.thread_sequence[thread] += 1;
        self.log.push(MemoryOperation {
            thread,
            thread_sequence: self.thread_sequence[thread],
            global_sequence: self.global_sequence,
            level,
            op: OperationType::Store(addr, val),
        })
    }

    pub fn load(&mut self, thread: usize, addr: usize, level: Ordering) -> usize {
        //        let mut possible_values = vec![];

        let mut self_value = None;

        for x in self.log.iter().rev() {
            if x.thread == thread {
                match x.op {
                    OperationType::Store(addr, val) => {
                        self_value = Some(val);
                        break;
                    }
                    // OperationType::Load(addr, val) => {
                    //     self_value = Some(val);
                    //     break;
                    // }
                    OperationType::Fence => {}
                }
            } else {
                if let OperationType::Store(oa, val) = &x.op {
                    if *oa == addr {
                        println!("Got Operation");
                    }
                }
            }
        }

        //println!("{:?}", possible_values);

        todo!()
    }

    // pub fn add_thread(&mut self) {
    //     self.threads.push(MemoryThread::default());
    // }
}
//
// impl Default for MemoryThread {
//     fn default() -> Self {
//         MemoryThread {}
//     }
// }

impl Default for MemorySystem {
    fn default() -> Self {
        MemorySystem {
            thread_sequence: vec![0, 0, 0, 0],
            global_sequence: 0,
            log: vec![],
        }
    }
}
