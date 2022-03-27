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

use rand::{RngCore, SeedableRng};
use rand_chacha::ChaCha8Rng;
use std::collections::HashMap;
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

#[derive(Default)]
pub struct ThreadView {
    pub sequence: usize,
    pub mem_sequence: HashMap<usize, usize>,
}

//pub struct MemoryCell {}

pub struct MemorySystem {
    pub global_sequence: usize,
    pub log: Vec<MemoryOperation>,
    pub threads: Vec<ThreadView>,
}

impl MemorySystem {
    pub fn store(&mut self, thread: usize, addr: usize, val: usize, level: Ordering) {
        self.global_sequence += 1;
        self.threads[thread].sequence += 1;
        self.log.push(MemoryOperation {
            thread,
            thread_sequence: self.threads[thread].sequence,
            global_sequence: self.global_sequence,
            level,
            op: OperationType::Store(addr, val),
        })
    }

    pub fn load(&mut self, thread: usize, _addr: usize, _level: Ordering) -> usize {
        let s = std::time::UNIX_EPOCH.elapsed().unwrap().as_nanos() as u64;
        let mut rng = ChaCha8Rng::seed_from_u64(s);

        let view = &mut self.threads[thread];

        let possible: Vec<&MemoryOperation> = self
            .log
            .iter()
            .filter(|mo| match mo.op {
                OperationType::Store(addr, _) => {
                    mo.global_sequence >= *view.mem_sequence.get(&addr).unwrap_or(&0_usize)
                }
                OperationType::Fence => false,
            })
            .collect();

        let choice = possible[(rng.next_u32() as usize) % possible.len()];

        match choice.op {
            OperationType::Store(loc, val) => {
                view.mem_sequence.insert(loc, choice.global_sequence);
                val
            }
            OperationType::Fence => {
                todo!()
            }
        }

        //        let mut possible_values = vec![];

        /*
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
        */
        //println!("{:?}", possible_values);
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
            threads: vec![ThreadView::default(), ThreadView::default()],
            global_sequence: 0,
            log: vec![],
        }
    }
}
