use rand::{Rng, RngCore, SeedableRng};
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
    pub release_chain: bool,
    pub source_sequence: MemorySequence,
    pub source_fence_sequence: FenceSequence,
}

#[derive(Default, Clone, Debug)]
pub struct FenceSequence {
    pub atomic: MemorySequence,
    pub fence: MemorySequence,
}

impl FenceSequence {
    pub fn synchronize(&mut self, other: &FenceSequence) {
        self.atomic.synchronize(&other.atomic);
        self.fence.synchronize(&other.fence);
    }

    pub fn mask_atomic(&self) -> FenceSequence {
        FenceSequence {
            atomic: Default::default(),
            ..self.clone()
        }
    }
}

#[derive(Default)]
pub struct ThreadView {
    pub sequence: usize,
    pub min_seq_cst_sequence: usize,
    pub mem_sequence: MemorySequence,
    pub fence_sequence: FenceSequence,
    pub read_fence_sequence: FenceSequence,
}

pub struct MemorySystem {
    pub global_sequence: usize,
    pub seq_cst_sequence: MemorySequence,
    pub log: Vec<MemoryOperation>,
    pub acc: Vec<MemoryOperation>,
    pub threads: Vec<ThreadView>,
}

impl MemorySystem {
    fn op<F: Fn(usize) -> Option<usize>>(
        &mut self,
        thread: usize,
        addr: usize,
        f: F,
        success: Ordering,
        failure: Ordering,
    ) -> Result<usize, usize> {
        assert!(
            failure == Ordering::SeqCst
                || failure == Ordering::Acquire
                || failure == Ordering::Relaxed
        );

        let view = &mut self.threads[thread];

        let all_ops = std::iter::once(&self.acc[addr]).chain(self.log.iter());

        let choice: &MemoryOperation = all_ops.filter(|mo| mo.address == addr).last().unwrap();

        let (load_ordering, store_ordering) = if success == Ordering::AcqRel {
            (Ordering::Acquire, Ordering::Release)
        } else {
            (success, success)
        };

        let v = choice.value;
        let res = f(v);

        if res.is_none() {
            Self::read_synchronize(view, choice, failure);

            return Err(v);
        }

        Self::read_synchronize(view, choice, load_ordering);

        Self::write_synchronize(view, &mut self.global_sequence, addr, store_ordering);

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

        // Are we continuing a release chain?
        let release_chain = choice.level != Ordering::Relaxed;

        let seqs = if choice.level == Ordering::Relaxed {
            this_seqs
        } else if success == Ordering::Release
            || success == Ordering::AcqRel
            || success == Ordering::SeqCst
        {
            combined_seqs
        } else {
            choice_seqs
        };

        let fence_sequence = if store_ordering == Ordering::Relaxed {
            seqs.1.mask_atomic()
        } else {
            seqs.1.clone()
        };

        self.log.push(MemoryOperation {
            thread,
            thread_sequence: view.sequence,
            global_sequence: self.global_sequence,
            source_fence_sequence: fence_sequence,
            level: store_ordering,
            release_chain,
            source_sequence: seqs.0,
            address: addr,
            value: res.unwrap(),
        });

        Ok(v)
    }

    // Used to implement fetch_add, fetch_sub etc
    pub fn fetch_op<F: Fn(usize) -> usize>(
        &mut self,
        thread: usize,
        addr: usize,
        f: F,
        level: Ordering,
    ) -> usize {
        // Relaxed is passed in for failure ordering - this operation can't fail
        self.op(thread, addr, |v| Some(f(v)), level, Ordering::Relaxed)
            .unwrap()
    }

    pub fn compare_exchange(
        &mut self,
        thread: usize,
        addr: usize,
        current: usize,
        new: usize,
        success: Ordering,
        failure: Ordering,
    ) -> Result<usize, usize> {
        self.op(
            thread,
            addr,
            |v| if v == current { Some(new) } else { None },
            success,
            failure,
        )
    }

    pub fn compare_exchange_weak(
        &mut self,
        thread: usize,
        addr: usize,
        current: usize,
        new: usize,
        success: Ordering,
        failure: Ordering,
    ) -> Result<usize, usize> {
        let s = std::time::UNIX_EPOCH.elapsed().unwrap().as_nanos() as u64;
        let mut rng = ChaCha8Rng::seed_from_u64(s);

        if rng.gen_bool(0.5) {
            self.op(
                thread,
                addr,
                |v| if v == current { Some(new) } else { None },
                success,
                failure,
            )
        } else {
            Err(self.load(thread, addr, failure))
        }
    }

    pub fn fetch_update<F: Fn(usize) -> Option<usize>>(
        &mut self,
        thread: usize,
        addr: usize,
        f: F,
        set_order: Ordering,
        fetch_order: Ordering,
    ) -> Result<usize, usize> {
        loop {
            let current = self.load(thread, addr, fetch_order);
            match f(current) {
                None => return Err(current),
                Some(new) => {
                    if self
                        .compare_exchange_weak(thread, addr, current, new, set_order, fetch_order)
                        .is_ok()
                    {
                        return Ok(current);
                    }
                }
            }
        }
    }

    pub fn fence(&mut self, thread: usize, level: Ordering) {
        assert!(
            level == Ordering::Acquire
                || level == Ordering::Release
                || level == Ordering::SeqCst
                || level == Ordering::AcqRel
        );

        self.global_sequence += 1;

        let view = &mut self.threads[thread];

        if level == Ordering::SeqCst {
            view.mem_sequence.synchronize(&self.seq_cst_sequence);
            self.seq_cst_sequence.synchronize(&view.mem_sequence);
            view.min_seq_cst_sequence = self.global_sequence;
        }

        if level == Ordering::Release || level == Ordering::SeqCst || level == Ordering::AcqRel {
            view.fence_sequence.fence.synchronize(&view.mem_sequence);
        }

        if level == Ordering::Acquire || level == Ordering::SeqCst || level == Ordering::AcqRel {
            view.mem_sequence
                .synchronize(&view.read_fence_sequence.fence);
            view.mem_sequence
                .synchronize(&view.read_fence_sequence.atomic);
        }
    }

    pub fn store(&mut self, thread: usize, addr: usize, val: usize, level: Ordering) {
        assert!(
            level == Ordering::Relaxed || level == Ordering::Release || level == Ordering::SeqCst
        );

        let view = &mut self.threads[thread];

        Self::write_synchronize(view, &mut self.global_sequence, addr, level);

        let fence_sequence = if level == Ordering::Relaxed {
            view.fence_sequence.mask_atomic()
        } else {
            view.fence_sequence.clone()
        };

        self.log.push(MemoryOperation {
            thread,
            thread_sequence: view.sequence,
            global_sequence: self.global_sequence,
            source_fence_sequence: fence_sequence,
            level,
            release_chain: false,
            source_sequence: view.mem_sequence.clone(),
            address: addr,
            value: val,
        });
    }

    fn write_synchronize(
        view: &mut ThreadView,
        global_sequence: &mut usize,
        addr: usize,
        level: Ordering,
    ) {
        *global_sequence += 1;
        view.sequence += 1;
        view.mem_sequence.sequence.insert(addr, *global_sequence);

        if level == Ordering::SeqCst || level == Ordering::Release {
            view.fence_sequence.atomic.synchronize(&view.mem_sequence);
        }
    }

    fn read_synchronize(view: &mut ThreadView, choice: &MemoryOperation, level: Ordering) {
        if (choice.level == Ordering::Release
            || choice.level == Ordering::SeqCst
            || choice.release_chain)
            && (level == Ordering::SeqCst || level == Ordering::Acquire)
        {
            view.mem_sequence.synchronize(&choice.source_sequence);
        }

        if level == Ordering::Acquire || level == Ordering::SeqCst {
            view.mem_sequence
                .synchronize(&choice.source_fence_sequence.fence);
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

        let seq_cst_ops = possible.iter().filter(|mo| mo.level == Ordering::SeqCst);

        let minimum_op = if level == Ordering::SeqCst {
            // A seq_cst load will see the latest seq_cst store if it exists
            let latest_seq_cst_op = seq_cst_ops
                .last()
                .map(|mo| mo.global_sequence)
                .unwrap_or(0_usize);

            // A seq_cst load will see all stores (regardless of level) prior to a seq_cst memory fence
            let latest_fence_op = self
                .seq_cst_sequence
                .sequence
                .get(&addr)
                .unwrap_or(&0_usize);

            latest_seq_cst_op.max(*latest_fence_op)
        } else {
            // A seq_cst fence on this thread causes the latest prior seq_cst store to be the minimum
            seq_cst_ops
                .filter(|mo| mo.global_sequence < view.min_seq_cst_sequence)
                .last()
                .map(|v| v.global_sequence)
                .unwrap_or(0_usize)
        };

        let first_ind = possible
            .iter()
            .position(|mo| {
                mo.global_sequence
                    >= *view
                        .mem_sequence
                        .sequence
                        .get(&addr)
                        .unwrap_or(&0_usize)
                        .max(&minimum_op)
            })
            .unwrap();

        let possible = &possible[first_ind..];

        let choice = possible[(rng.next_u32() as usize) % possible.len()];

        Self::read_synchronize(view, choice, level);

        choice.value
    }
}

impl Default for MemorySystem {
    fn default() -> Self {
        MemorySystem {
            threads: vec![],
            acc: vec![],
            global_sequence: 10,
            seq_cst_sequence: Default::default(),
            log: vec![],
        }
    }
}

impl MemorySystem {
    pub fn add_thread(&mut self) -> usize {
        let v = self.threads.len();

        self.threads.push(ThreadView::default());

        v
    }

    pub fn malloc(&mut self, size: usize) -> usize {
        let base = self.acc.len();

        for i in 0..size {
            self.acc.push(MemoryOperation {
                thread: 0,
                thread_sequence: 0,
                global_sequence: 0,
                level: Ordering::Relaxed,
                release_chain: false,
                address: i,
                value: 0,
                source_sequence: Default::default(),
                source_fence_sequence: Default::default(),
            })
        }

        base
    }
}
