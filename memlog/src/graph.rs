use rand::{Rng, RngCore, SeedableRng};
use rand_chacha::ChaCha8Rng;
use std::sync::atomic::Ordering;

#[derive(Clone, Debug)]
struct Write {
    value: usize,
    level: Ordering,
    release_chain: bool,
    view: Vec<usize>,
    fence_view: Vec<usize>,
    event: Option<usize>,
}

#[derive(Clone, Debug)]
struct ThreadView {
    visible: Vec<usize>,
    pending_fence: Vec<usize>,
    release_fence: Vec<usize>,
    seq_cst_min: Vec<usize>,
    last_seq_cst: Option<usize>,
}

#[derive(Clone, Debug)]
enum EventKind {
    Load { addr: usize, from: usize },
    Store,
}

#[derive(Clone, Debug)]
struct Event {
    kind: EventKind,
}

#[derive(Debug)]
pub struct MemorySystem {
    writes: Vec<Vec<Write>>,
    latest_seq_cst: Vec<usize>,
    seq_cst_writes: Vec<Vec<usize>>,
    seq_cst_fence: Vec<usize>,
    threads: Vec<ThreadView>,
    events: Vec<Event>,
    edges: Vec<Vec<usize>>,
    read_frontier: Vec<Vec<usize>>,
    reach_seen: Vec<u32>,
    reach_generation: u32,
    reach_stack: Vec<usize>,
    rng: ChaCha8Rng,
}

impl MemorySystem {
    pub fn add_thread(&mut self) -> usize {
        let thread = self.threads.len();
        self.threads.push(ThreadView {
            visible: self
                .latest_seq_cst
                .iter()
                .zip(self.seq_cst_fence.iter())
                .map(|(store, fence)| (*store).max(*fence))
                .collect(),
            pending_fence: vec![0; self.writes.len()],
            release_fence: vec![0; self.writes.len()],
            seq_cst_min: vec![0; self.writes.len()],
            last_seq_cst: None,
        });
        thread
    }

    pub fn malloc(&mut self, size: usize) -> usize {
        let base = self.writes.len();

        for _ in 0..size {
            self.writes.push(vec![Write {
                value: 0,
                level: Ordering::Relaxed,
                release_chain: false,
                view: vec![],
                fence_view: vec![],
                event: None,
            }]);
            self.latest_seq_cst.push(0);
            self.seq_cst_writes.push(vec![]);
            self.seq_cst_fence.push(0);
            self.read_frontier.push(vec![]);
        }

        for thread in &mut self.threads {
            thread.visible.resize(self.writes.len(), 0);
            thread.pending_fence.resize(self.writes.len(), 0);
            thread.release_fence.resize(self.writes.len(), 0);
            thread.seq_cst_min.resize(self.writes.len(), 0);
        }

        base
    }

    pub fn load(&mut self, thread: usize, addr: usize, level: Ordering) -> usize {
        assert!(matches!(
            level,
            Ordering::Relaxed | Ordering::Acquire | Ordering::SeqCst
        ));

        let mut first = self.threads[thread].visible[addr];
        if level == Ordering::SeqCst {
            first = first.max(self.seq_cst_fence[addr]);
        } else {
            first = first.max(self.threads[thread].seq_cst_min[addr]);
        }

        let selected = self.select_load_candidate(thread, addr, first, level);
        let value = self.read_from(thread, addr, selected, level);

        if level == Ordering::SeqCst {
            self.add_seq_cst_load(thread, addr, selected);
        }

        value
    }

    pub fn store(&mut self, thread: usize, addr: usize, val: usize, level: Ordering) {
        assert!(matches!(
            level,
            Ordering::Relaxed | Ordering::Release | Ordering::SeqCst
        ));

        let view = if is_release_store_order(level) {
            self.threads[thread].visible.clone()
        } else {
            vec![]
        };

        self.push_write(thread, addr, val, level, false, view);
    }

    pub fn fetch_op<F: Fn(usize) -> usize>(
        &mut self,
        thread: usize,
        addr: usize,
        f: F,
        level: Ordering,
    ) -> usize {
        let selected = self.writes[addr].len() - 1;
        let current = self.rmw_read(thread, addr, selected, level);
        self.rmw_write(thread, addr, f(current), selected, rmw_store_order(level));
        current
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
        assert_valid_failure_order(failure);

        let selected = self.writes[addr].len() - 1;
        let actual = self.writes[addr][selected].value;

        if actual == current {
            self.rmw_read(thread, addr, selected, success);
            self.rmw_write(thread, addr, new, selected, rmw_store_order(success));
            Ok(actual)
        } else {
            self.read_from(thread, addr, selected, failure);

            if failure == Ordering::SeqCst {
                debug_assert!(self.seq_cst_load_is_valid(thread, addr, selected));
                self.add_seq_cst_load(thread, addr, selected);
            }

            Err(actual)
        }
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
        assert_valid_failure_order(failure);

        if self.rng.gen_bool(0.5) {
            self.compare_exchange(thread, addr, current, new, success, failure)
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

    pub fn fence(&mut self, _thread: usize, level: Ordering) {
        assert!(matches!(
            level,
            Ordering::Acquire | Ordering::Release | Ordering::AcqRel | Ordering::SeqCst
        ));

        if matches!(
            level,
            Ordering::Acquire | Ordering::AcqRel | Ordering::SeqCst
        ) {
            let pending_fence = self.threads[_thread].pending_fence.clone();
            synchronize(&mut self.threads[_thread].visible, &pending_fence);
        }

        if matches!(
            level,
            Ordering::Release | Ordering::AcqRel | Ordering::SeqCst
        ) {
            let visible = self.threads[_thread].visible.clone();
            synchronize(&mut self.threads[_thread].release_fence, &visible);
        }

        if level == Ordering::SeqCst {
            synchronize(&mut self.threads[_thread].visible, &self.seq_cst_fence);
            synchronize(&mut self.seq_cst_fence, &self.threads[_thread].visible);
            synchronize(&mut self.threads[_thread].seq_cst_min, &self.latest_seq_cst);
        }
    }

    fn read_from(&mut self, thread: usize, addr: usize, selected: usize, level: Ordering) -> usize {
        let write = self.writes[addr][selected].clone();

        self.threads[thread].visible[addr] = selected;

        if is_synchronizing_source(&write) {
            synchronize(&mut self.threads[thread].pending_fence, &write.view);
        }

        synchronize(&mut self.threads[thread].pending_fence, &write.fence_view);

        if matches!(level, Ordering::Acquire | Ordering::SeqCst) {
            if is_synchronizing_source(&write) {
                synchronize(&mut self.threads[thread].visible, &write.view);
            }
            synchronize(&mut self.threads[thread].visible, &write.fence_view);
        }

        write.value
    }

    fn rmw_read(&mut self, thread: usize, addr: usize, selected: usize, level: Ordering) -> usize {
        let load_order = match level {
            Ordering::AcqRel => Ordering::Acquire,
            Ordering::Release => Ordering::Relaxed,
            _ => level,
        };

        self.read_from(thread, addr, selected, load_order)
    }

    fn rmw_write(
        &mut self,
        thread: usize,
        addr: usize,
        val: usize,
        source: usize,
        level: Ordering,
    ) {
        let source_write = self.writes[addr][source].clone();
        let carries_release = is_synchronizing_source(&source_write);
        let mut view = if carries_release {
            source_write.view
        } else {
            vec![]
        };

        if is_release_store_order(level) {
            synchronize(&mut view, &self.threads[thread].visible);
        }

        self.push_write(thread, addr, val, level, carries_release, view);
    }

    fn push_write(
        &mut self,
        thread: usize,
        addr: usize,
        val: usize,
        level: Ordering,
        release_chain: bool,
        mut view: Vec<usize>,
    ) {
        let new_index = self.writes[addr].len();
        self.threads[thread].visible[addr] = new_index;

        if is_release_store_order(level) {
            synchronize(&mut view, &self.threads[thread].visible);
        }

        let event = if level == Ordering::SeqCst {
            Some(self.add_seq_cst_store(thread, addr, new_index))
        } else {
            None
        };

        self.writes[addr].push(Write {
            value: val,
            level,
            release_chain,
            view,
            fence_view: self.threads[thread].release_fence.clone(),
            event,
        });

        if level == Ordering::SeqCst {
            self.latest_seq_cst[addr] = new_index;
            self.seq_cst_writes[addr].push(new_index);
        }
    }

    fn select_load_candidate(
        &mut self,
        thread: usize,
        addr: usize,
        first: usize,
        level: Ordering,
    ) -> usize {
        let last = self.writes[addr].len() - 1;
        assert!(first <= last);

        let first_valid = if level == Ordering::SeqCst {
            self.first_valid_seq_cst_load(thread, addr, first, last)
        } else {
            first
        };

        debug_assert!(first_valid <= last);
        debug_assert!(
            level != Ordering::SeqCst || self.seq_cst_load_is_valid(thread, addr, first_valid)
        );

        first_valid + self.rng.next_u32() as usize % (last - first_valid + 1)
    }

    fn seq_cst_load_is_valid(&mut self, thread: usize, addr: usize, from: usize) -> bool {
        let (incoming, outgoing) = self.seq_cst_load_edges(thread, addr, from);
        self.seq_cst_event_edges_are_valid(&incoming, &outgoing)
    }

    fn first_valid_seq_cst_load(
        &mut self,
        thread: usize,
        addr: usize,
        first: usize,
        last: usize,
    ) -> usize {
        let Some(previous) = self.threads[thread].last_seq_cst else {
            return first;
        };

        let mut low = first;
        let mut high = last;

        while low < high {
            let mid = low + (high - low) / 2;

            if self.seq_cst_candidate_can_follow_previous(addr, mid, previous) {
                high = mid;
            } else {
                low = mid + 1;
            }
        }

        #[cfg(debug_assertions)]
        {
            for candidate in first..=last {
                let valid = self.seq_cst_load_is_valid(thread, addr, candidate);
                debug_assert_eq!(
                    valid,
                    candidate >= low,
                    "seq_cst load validity must be a suffix of modification order"
                );
            }
        }

        low
    }

    fn seq_cst_candidate_can_follow_previous(
        &mut self,
        addr: usize,
        from: usize,
        previous: usize,
    ) -> bool {
        let Some(next_store) = self.next_seq_cst_store_after(addr, from) else {
            return true;
        };
        let next_store_event = self.writes[addr][next_store]
            .event
            .expect("seq_cst write index without event");

        !self.reaches(next_store_event, previous)
    }

    fn add_seq_cst_load(&mut self, thread: usize, addr: usize, from: usize) {
        let (incoming, outgoing) = self.seq_cst_load_edges(thread, addr, from);
        debug_assert!(self.seq_cst_event_edges_are_valid(&incoming, &outgoing));

        let event = self.add_seq_cst_event(EventKind::Load { addr, from });

        for source in incoming {
            self.add_seq_cst_edge(source, event);
        }

        for target in outgoing {
            self.add_seq_cst_edge(event, target);
        }

        debug_assert!(is_acyclic(&self.edges));
        self.add_read_frontier(addr, event);
        self.threads[thread].last_seq_cst = Some(event);
    }

    fn seq_cst_load_edges(
        &self,
        thread: usize,
        addr: usize,
        from: usize,
    ) -> (Vec<usize>, Vec<usize>) {
        let mut incoming = vec![];
        let mut outgoing = vec![];

        if let Some(previous) = self.threads[thread].last_seq_cst {
            incoming.push(previous);
        }

        if let Some(source) = self.writes[addr][from].event {
            incoming.push(source);
        }

        if let Some(next_store) = self.next_seq_cst_store_after(addr, from) {
            outgoing.push(
                self.writes[addr][next_store]
                    .event
                    .expect("seq_cst write index without event"),
            );
        }

        (incoming, outgoing)
    }

    fn next_seq_cst_store_after(&self, addr: usize, from: usize) -> Option<usize> {
        let stores = &self.seq_cst_writes[addr];
        stores
            .get(stores.partition_point(|store| *store <= from))
            .copied()
    }

    fn seq_cst_event_edges_are_valid(&mut self, incoming: &[usize], outgoing: &[usize]) -> bool {
        for source in incoming {
            for target in outgoing {
                if source == target || self.reaches(*target, *source) {
                    return false;
                }
            }
        }

        true
    }

    fn add_seq_cst_store(&mut self, thread: usize, addr: usize, index: usize) -> usize {
        let event = self.add_seq_cst_event(EventKind::Store);

        if let Some(previous) = self.threads[thread].last_seq_cst {
            self.add_seq_cst_edge(previous, event);
        }

        if let Some(store) = self.writes[addr][self.latest_seq_cst[addr]].event {
            self.add_seq_cst_edge(store, event);
        }

        debug_assert!(self.read_frontier[addr]
            .iter()
            .all(|load| matches!(self.events[*load].kind, EventKind::Load { addr: load_addr, from } if load_addr == addr && from < index)));

        let read_frontier = self.read_frontier[addr].clone();
        for load in read_frontier {
            self.add_seq_cst_edge(load, event);
        }

        debug_assert!(is_acyclic(&self.edges));
        self.threads[thread].last_seq_cst = Some(event);
        event
    }

    fn add_seq_cst_event(&mut self, kind: EventKind) -> usize {
        let event = self.events.len();
        self.events.push(Event { kind });
        self.edges.push(vec![]);
        self.reach_seen.push(0);

        event
    }

    fn add_seq_cst_edge(&mut self, source: usize, target: usize) {
        debug_assert_ne!(source, target);
        debug_assert!(!self.reaches(target, source));

        if !self.edges[source].contains(&target) {
            self.edges[source].push(target);
        }
    }

    fn reaches(&mut self, source: usize, target: usize) -> bool {
        self.reach_generation = self.reach_generation.wrapping_add(1);
        if self.reach_generation == 0 {
            self.reach_seen.fill(0);
            self.reach_generation = 1;
        }

        self.reach_stack.clear();
        self.reach_stack.push(source);

        while let Some(event) = self.reach_stack.pop() {
            if event == target {
                return true;
            }

            if self.reach_seen[event] == self.reach_generation {
                continue;
            }

            self.reach_seen[event] = self.reach_generation;
            self.reach_stack.extend(self.edges[event].iter().copied());
        }

        false
    }

    fn add_read_frontier(&mut self, addr: usize, event: usize) {
        let mut is_maximal = true;
        let mut frontier = vec![];

        let existing_frontier = self.read_frontier[addr].clone();

        for existing in existing_frontier {
            if self.reaches(event, existing) {
                is_maximal = false;
                frontier.push(existing);
            } else if !self.reaches(existing, event) {
                frontier.push(existing);
            }
        }

        if is_maximal {
            frontier.push(event);
        }

        self.read_frontier[addr] = frontier;
    }

    #[allow(unused)]
    fn validate_seq_cst_reachability(&mut self) {
        let edges = self.edges.clone();

        for (source, event_edges) in edges.iter().enumerate() {
            for target in event_edges {
                debug_assert!(self.reaches(source, *target));
            }
        }

        for source in 0..self.events.len() {
            for target in 0..self.events.len() {
                if source != target && self.reaches(source, target) {
                    debug_assert_ne!(source, target);
                }
            }
        }
    }
}

impl Default for MemorySystem {
    fn default() -> Self {
        let seed = std::time::UNIX_EPOCH.elapsed().unwrap().as_nanos() as u64;

        MemorySystem {
            writes: vec![],
            latest_seq_cst: vec![],
            seq_cst_writes: vec![],
            seq_cst_fence: vec![],
            threads: vec![],
            events: vec![],
            edges: vec![],
            read_frontier: vec![],
            reach_seen: vec![],
            reach_generation: 0,
            reach_stack: vec![],
            rng: ChaCha8Rng::seed_from_u64(seed),
        }
    }
}

fn assert_valid_failure_order(level: Ordering) {
    assert!(matches!(
        level,
        Ordering::Relaxed | Ordering::Acquire | Ordering::SeqCst
    ));
}

fn rmw_store_order(level: Ordering) -> Ordering {
    match level {
        Ordering::AcqRel => Ordering::Release,
        Ordering::Acquire => Ordering::Relaxed,
        _ => level,
    }
}

fn is_release_store_order(level: Ordering) -> bool {
    matches!(level, Ordering::Release | Ordering::SeqCst)
}

fn is_synchronizing_source(write: &Write) -> bool {
    matches!(write.level, Ordering::Release | Ordering::SeqCst) || write.release_chain
}

fn synchronize(target: &mut Vec<usize>, source: &[usize]) {
    if target.len() < source.len() {
        target.resize(source.len(), 0);
    }

    for (addr, sequence) in source.iter().enumerate() {
        target[addr] = target[addr].max(*sequence);
    }
}

fn is_acyclic(edges: &[Vec<usize>]) -> bool {
    fn visit(node: usize, edges: &[Vec<usize>], state: &mut [u8]) -> bool {
        if state[node] == 1 {
            return false;
        }

        if state[node] == 2 {
            return true;
        }

        state[node] = 1;

        for next in &edges[node] {
            if !visit(*next, edges, state) {
                return false;
            }
        }

        state[node] = 2;
        true
    }

    let mut state = vec![0; edges.len()];

    for node in 0..edges.len() {
        if state[node] == 0 && !visit(node, edges, &mut state) {
            return false;
        }
    }

    true
}
