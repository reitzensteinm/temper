use rand::{RngCore, SeedableRng};
use rand_chacha::ChaCha8Rng;
use std::collections::HashMap;
use std::sync::atomic::Ordering;

#[derive(Debug)]
pub enum OperationType {
    Store(usize, usize),
    //Load(usize, usize),
    Fence,
}

#[derive(Debug)]
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

pub struct MemorySystem {
    pub global_sequence: usize,
    pub log: Vec<MemoryOperation>,
    pub acc: Vec<MemoryOperation>,
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

    pub fn load(&mut self, thread: usize, addr: usize, _level: Ordering) -> usize {
        let s = std::time::UNIX_EPOCH.elapsed().unwrap().as_nanos() as u64;
        let mut rng = ChaCha8Rng::seed_from_u64(s);

        let view = &mut self.threads[thread];

        let all_ops = std::iter::once(&self.acc[addr]).chain(self.log.iter());

        let possible: Vec<&MemoryOperation> = all_ops
            .filter(|mo| match mo.op {
                // Todo: Is the global sequence the only correct here?
                OperationType::Store(a, _) => a == addr,
                OperationType::Fence => false,
            })
            .collect();

        let first_ind = possible
            .iter()
            .position(|mo| match mo.op {
                OperationType::Store(a, _) => {
                    mo.global_sequence >= *view.mem_sequence.get(&a).unwrap_or(&0_usize)
                }
                OperationType::Fence => false,
            })
            .unwrap_or(0_usize);

        /*
        println!(
            "{} {}",
            first_ind,
            *view.mem_sequence.get(&0).unwrap_or(&5_usize)
        );

        for p in possible.iter() {
            println!("{:?}", p);
        }*/

        let possible = &possible[first_ind..];

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
    }
}

impl Default for MemorySystem {
    fn default() -> Self {
        let mut acc = vec![];

        for i in 0..100 {
            acc.push(MemoryOperation {
                thread: 0,
                thread_sequence: 0,
                global_sequence: 0,
                level: Ordering::Relaxed,
                op: OperationType::Store(i, 0),
            })
        }

        MemorySystem {
            threads: vec![ThreadView::default(), ThreadView::default()],
            acc,
            global_sequence: 10,
            log: vec![],
        }
    }
}
