use rand::{RngCore, SeedableRng};
use rand_chacha::ChaCha8Rng;
use std::collections::HashMap;
use std::sync::atomic::Ordering;

#[derive(Default, Debug, Clone)]
pub struct MemorySequence {
    pub sequence: HashMap<usize, usize>,
}

impl MemorySequence {
    pub fn synchronize(&mut self, other: &MemorySequence) {
        for (k, v) in other.sequence.iter() {
            let res = if let Some(ev) = self.sequence.get(k) {
                (*ev).max(*v)
            } else {
                *v
            };

            self.sequence.insert(*k, res);
        }
    }
}

#[derive(Debug)]
pub struct MemoryOperation {
    pub thread: usize,
    pub thread_sequence: usize,
    pub global_sequence: usize,
    pub level: Ordering,
    pub address: usize,
    pub value: usize,
    pub source_sequence: MemorySequence,
    pub source_fence_sequence: MemorySequence,
}

#[derive(Default)]
pub struct ThreadView {
    pub sequence: usize,
    pub mem_sequence: MemorySequence,
    pub fence_sequence: MemorySequence,
    pub read_fence_sequence: MemorySequence,
}

pub struct MemorySystem {
    pub global_sequence: usize,
    pub seq_cst_sequence: MemorySequence,
    pub log: Vec<MemoryOperation>,
    pub acc: Vec<MemoryOperation>,
    pub threads: Vec<ThreadView>,
}

impl MemorySystem {
    pub fn fetch_update<F: Fn(usize) -> Option<usize>>(
        &mut self,
        thread: usize,
        addr: usize,
        f: F,
        level: Ordering,
    ) -> Result<usize, usize> {
        let view = &mut self.threads[thread];

        let all_ops = std::iter::once(&self.acc[addr]).chain(self.log.iter());

        let choice: &MemoryOperation = all_ops.filter(|mo| mo.address == addr).last().unwrap();

        let (load_ordering, store_ordering) = if level == Ordering::AcqRel {
            (Ordering::Acquire, Ordering::Release)
        } else {
            (level, level)
        };

        Self::read_synchronize(view, choice, &self.seq_cst_sequence, load_ordering);

        let v = choice.value;
        let res = f(v);

        if res.is_none() {
            return Err(v);
        }

        // Todo: Tests for load/Store ordering
        Self::write_synchronize(
            view,
            &mut self.seq_cst_sequence,
            &mut self.global_sequence,
            addr,
            store_ordering,
        );

        // Todo: Write some great tests to explore these
        // If previous store is relaxed, normal rules apply
        // If previous store release or seqcst:
        // Relaxed: Stores choice's fence and mem sequence
        // Acquire: Stores choice's fence and mem sequence
        // Release: Stores choice's fence and mem sequence plus this thread's
        // AcqRel: Stores choice's fence and mem sequence plus this thread's
        // SeqCst: Stores choice's fence and mem sequence plus this thread's

        let choice_seqs = (
            choice.source_sequence.clone(),
            choice.source_fence_sequence.clone(),
        );
        let this_seqs = (view.mem_sequence.clone(), view.fence_sequence.clone());
        let combined_seqs = {
            let mut ms = view.mem_sequence.clone();
            ms.synchronize(&choice.source_sequence);
            let mut fs = view.fence_sequence.clone();
            fs.synchronize(&choice.source_fence_sequence);
            (ms, fs)
        };

        let seqs = if choice.level == Ordering::Relaxed {
            this_seqs
        } else {
            if level == Ordering::Release || level == Ordering::AcqRel || level == Ordering::SeqCst
            {
                combined_seqs
            } else {
                choice_seqs
            }
        };

        self.log.push(MemoryOperation {
            thread,
            thread_sequence: view.sequence,
            global_sequence: self.global_sequence,
            source_fence_sequence: seqs.1,
            level,
            source_sequence: seqs.0,
            address: addr,
            value: res.unwrap(),
        });

        Ok(v)
    }

    pub fn fetch_modify_old<F: Fn(usize) -> usize>(
        &mut self,
        thread: usize,
        addr: usize,
        f: F,
        level: Ordering,
    ) {
        assert!(
            level == Ordering::Relaxed || level == Ordering::AcqRel || level == Ordering::SeqCst
        );

        let view = &mut self.threads[thread];

        // fetch_modify always gets the latest
        view.mem_sequence
            .sequence
            .insert(addr, self.global_sequence);

        let (load_ordering, store_ordering) = if level == Ordering::AcqRel {
            (Ordering::Acquire, Ordering::Release)
        } else {
            (level, level)
        };

        let v = self.load(thread, addr, load_ordering);

        self.store(thread, addr, f(v), store_ordering);
    }

    pub fn exchange_old(
        &mut self,
        thread: usize,
        addr: usize,
        expected: usize,
        new: usize,
        level: Ordering,
    ) -> bool {
        assert!(
            level == Ordering::Relaxed || level == Ordering::AcqRel || level == Ordering::SeqCst
        );

        let view = &mut self.threads[thread];

        // CAS operations will
        view.mem_sequence
            .sequence
            .insert(addr, self.global_sequence);

        let (load_ordering, store_ordering) = if level == Ordering::AcqRel {
            (Ordering::Acquire, Ordering::Release)
        } else {
            (level, level)
        };

        let v = self.load(thread, addr, load_ordering);

        if v == expected {
            self.store(thread, addr, new, store_ordering);
            true
        } else {
            false
        }
    }

    pub fn fence(&mut self, thread: usize, level: Ordering) {
        assert!(
            level == Ordering::Acquire
                || level == Ordering::Release
                || level == Ordering::SeqCst
                || level == Ordering::AcqRel
        );

        let view = &mut self.threads[thread];

        if level == Ordering::SeqCst {
            view.mem_sequence.synchronize(&self.seq_cst_sequence);
            self.seq_cst_sequence.synchronize(&view.mem_sequence);
        }

        if level == Ordering::Release || level == Ordering::SeqCst || level == Ordering::AcqRel {
            view.fence_sequence = view.mem_sequence.clone();
        }

        if level == Ordering::Acquire || level == Ordering::AcqRel {
            view.mem_sequence.synchronize(&view.read_fence_sequence);
        }
    }

    pub fn store(&mut self, thread: usize, addr: usize, val: usize, level: Ordering) {
        assert!(
            level == Ordering::Relaxed || level == Ordering::Release || level == Ordering::SeqCst
        );

        let view = &mut self.threads[thread];

        Self::write_synchronize(
            view,
            &mut self.seq_cst_sequence,
            &mut self.global_sequence,
            addr,
            level,
        );

        self.log.push(MemoryOperation {
            thread,
            thread_sequence: view.sequence,
            global_sequence: self.global_sequence,
            source_fence_sequence: view.fence_sequence.clone(),
            level,
            source_sequence: view.mem_sequence.clone(),
            address: addr,
            value: val,
        });
    }

    fn write_synchronize(
        view: &mut ThreadView,
        seq_cst_sequence: &mut MemorySequence,
        global_sequence: &mut usize,
        addr: usize,
        level: Ordering,
    ) {
        *global_sequence += 1;
        view.sequence += 1;
        view.mem_sequence.sequence.insert(addr, *global_sequence);

        if level == Ordering::SeqCst {
            seq_cst_sequence.synchronize(&view.mem_sequence);
        }

        if level == Ordering::SeqCst || level == Ordering::Release {
            view.fence_sequence = view.mem_sequence.clone();
        }
    }

    fn read_synchronize(
        view: &mut ThreadView,
        choice: &MemoryOperation,
        seq_cst_sequence: &MemorySequence,
        level: Ordering,
    ) {
        if level == Ordering::SeqCst {
            view.mem_sequence.synchronize(seq_cst_sequence);
        }

        if (choice.level == Ordering::Release || choice.level == Ordering::SeqCst)
            && (level == Ordering::SeqCst || level == Ordering::Acquire)
        {
            view.mem_sequence.synchronize(&choice.source_sequence);
        }

        if level == Ordering::Acquire || level == Ordering::SeqCst {
            view.mem_sequence.synchronize(&choice.source_fence_sequence);
        }

        view.read_fence_sequence
            .synchronize(&choice.source_fence_sequence);

        view.mem_sequence
            .sequence
            .insert(choice.address, choice.global_sequence);
    }

    pub fn load(&mut self, thread: usize, addr: usize, level: Ordering) -> usize {
        assert!(
            level == Ordering::Relaxed || level == Ordering::Acquire || level == Ordering::SeqCst
        );
        let s = std::time::UNIX_EPOCH.elapsed().unwrap().as_nanos() as u64;
        let mut rng = ChaCha8Rng::seed_from_u64(s);

        let view = &mut self.threads[thread];

        let all_ops = std::iter::once(&self.acc[addr]).chain(self.log.iter());

        let possible: Vec<&MemoryOperation> = all_ops.filter(|mo| mo.address == addr).collect();

        let first_ind = possible
            .iter()
            .position(|mo| {
                mo.global_sequence >= *view.mem_sequence.sequence.get(&addr).unwrap_or(&0_usize)
            })
            .unwrap_or(0_usize);

        let possible = &possible[first_ind..];

        let choice = possible[(rng.next_u32() as usize) % possible.len()];

        Self::read_synchronize(view, choice, &self.seq_cst_sequence, level);

        choice.value
    }
}

impl Default for MemorySystem {
    fn default() -> Self {
        let mut acc = vec![];

        // Todo: Allocate the right number of buckets! malloc!()
        for i in 0..10 {
            acc.push(MemoryOperation {
                thread: 0,
                thread_sequence: 0,
                global_sequence: 0,
                level: Ordering::Relaxed,
                address: i,
                value: 0,
                source_sequence: Default::default(),
                source_fence_sequence: Default::default(),
            })
        }

        MemorySystem {
            threads: vec![
                ThreadView::default(),
                ThreadView::default(),
                ThreadView::default(),
                ThreadView::default(),
            ],
            acc,
            global_sequence: 10,
            seq_cst_sequence: Default::default(),
            log: vec![],
        }
    }
}
