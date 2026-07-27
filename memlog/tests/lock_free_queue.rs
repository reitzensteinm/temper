use crate::common::harness::{Environment, LogTest, Value};
use rand::{RngCore, SeedableRng};
use rand_chacha::ChaCha8Rng;
use std::sync::atomic::Ordering;

#[allow(dead_code)]
mod common;

const SPSC_SLOT_COUNT: usize = 4;
const SPSC_VALUE_COUNT: usize = 64;
const SPSC_MAX_ATTEMPTS: usize = SPSC_VALUE_COUNT * 64;
const MPMC_SLOT_COUNT: usize = 4;
const MPMC_PRODUCER_COUNT: usize = 2;
const MPMC_CONSUMER_COUNT: usize = 2;
const MPMC_VALUES_PER_THREAD: usize = 16;
const MPMC_VALUE_COUNT: usize = MPMC_PRODUCER_COUNT * MPMC_VALUES_PER_THREAD;
const MPMC_MAX_ATTEMPTS: usize = MPMC_VALUE_COUNT * 128;
const EXECUTION_COUNT: u64 = 1_000;

#[derive(Clone, Copy, Debug)]
struct SpscResult {
    pushed: usize,
    popped: usize,
    values: [usize; SPSC_VALUE_COUNT],
}

impl Default for SpscResult {
    fn default() -> Self {
        Self {
            pushed: 0,
            popped: 0,
            values: [0; SPSC_VALUE_COUNT],
        }
    }
}

fn spsc_slot(env: &mut Environment, index: usize) -> &mut Value {
    &mut env.heap[index]
}

fn spsc_try_push(env: &mut Environment, value: usize) -> bool {
    let tail = env.b.load(Ordering::Relaxed);
    let next_tail = (tail + 1) % SPSC_SLOT_COUNT;

    if next_tail == env.a.load(Ordering::Acquire) {
        return false;
    }

    spsc_slot(env, tail).store(value, Ordering::Relaxed);
    env.b.store(next_tail, Ordering::Release);
    true
}

fn spsc_try_pop(env: &mut Environment) -> Option<usize> {
    let head = env.a.load(Ordering::Relaxed);

    if head == env.b.load(Ordering::Acquire) {
        return None;
    }

    let value = spsc_slot(env, head).load(Ordering::Relaxed);
    env.a.store((head + 1) % SPSC_SLOT_COUNT, Ordering::Release);
    Some(value)
}

fn spsc_producer(mut env: Environment) -> SpscResult {
    let mut result = SpscResult::default();

    for _ in 0..SPSC_MAX_ATTEMPTS {
        if result.pushed == SPSC_VALUE_COUNT {
            break;
        }

        if spsc_try_push(&mut env, result.pushed + 1) {
            result.pushed += 1;
        }
    }

    assert_eq!(
        result.pushed, SPSC_VALUE_COUNT,
        "SPSC producer reached SPSC_MAX_ATTEMPTS"
    );
    result
}

fn spsc_consumer(mut env: Environment) -> SpscResult {
    let mut result = SpscResult::default();

    for _ in 0..SPSC_MAX_ATTEMPTS {
        if result.popped == SPSC_VALUE_COUNT {
            break;
        }

        if let Some(value) = spsc_try_pop(&mut env) {
            result.values[result.popped] = value;
            result.popped += 1;
        }
    }

    assert_eq!(
        result.popped, SPSC_VALUE_COUNT,
        "SPSC consumer reached SPSC_MAX_ATTEMPTS"
    );
    result
}

#[derive(Clone, Copy, Debug, Default)]
struct MpmcEntry {
    position: usize,
    value: usize,
}

#[derive(Clone, Copy, Debug, Default)]
struct MpmcResult {
    count: usize,
    entries: [MpmcEntry; MPMC_VALUES_PER_THREAD],
}

fn mpmc_sequence(env: &mut Environment, index: usize) -> &mut Value {
    &mut env.heap[index]
}

fn mpmc_value(env: &mut Environment, index: usize) -> &mut Value {
    &mut env.heap[MPMC_SLOT_COUNT + index]
}

fn initialize_mpmc(env: &mut Environment) {
    for index in 0..MPMC_SLOT_COUNT {
        mpmc_sequence(env, index).store(index, Ordering::Relaxed);
    }
    env.c.store(1, Ordering::Release);
}

fn wait_for_mpmc_initialization(env: &mut Environment) {
    for _ in 0..MPMC_MAX_ATTEMPTS {
        if env.c.load(Ordering::Acquire) == 1 {
            return;
        }
    }

    panic!("MPMC initialization reached MPMC_MAX_ATTEMPTS");
}

fn mpmc_try_push(env: &mut Environment, value: usize) -> Option<MpmcEntry> {
    let position = env.b.load(Ordering::Relaxed);
    let index = position % MPMC_SLOT_COUNT;
    let sequence = mpmc_sequence(env, index).load(Ordering::Acquire);

    if sequence as isize - position as isize != 0 {
        return None;
    }

    if env
        .b
        .exchange_weak(position, position + 1, Ordering::Relaxed, Ordering::Relaxed)
        .is_err()
    {
        return None;
    }

    mpmc_value(env, index).store(value, Ordering::Relaxed);
    mpmc_sequence(env, index).store(position + 1, Ordering::Release);
    Some(MpmcEntry { position, value })
}

fn mpmc_try_pop(env: &mut Environment) -> Option<MpmcEntry> {
    let position = env.a.load(Ordering::Relaxed);
    let index = position % MPMC_SLOT_COUNT;
    let sequence = mpmc_sequence(env, index).load(Ordering::Acquire);

    if sequence as isize - (position + 1) as isize != 0 {
        return None;
    }

    if env
        .a
        .exchange_weak(position, position + 1, Ordering::Relaxed, Ordering::Relaxed)
        .is_err()
    {
        return None;
    }

    let value = mpmc_value(env, index).load(Ordering::Relaxed);
    mpmc_sequence(env, index).store(position + MPMC_SLOT_COUNT, Ordering::Release);
    Some(MpmcEntry { position, value })
}

fn mpmc_producer(producer: usize, initializes: bool) -> impl FnMut(Environment) -> MpmcResult {
    move |mut env| {
        if initializes {
            initialize_mpmc(&mut env);
        } else {
            wait_for_mpmc_initialization(&mut env);
        }

        let mut result = MpmcResult::default();
        for offset in 0..MPMC_VALUES_PER_THREAD {
            let value = producer * MPMC_VALUES_PER_THREAD + offset + 1;

            for _ in 0..MPMC_MAX_ATTEMPTS {
                if let Some(entry) = mpmc_try_push(&mut env, value) {
                    result.entries[result.count] = entry;
                    result.count += 1;
                    break;
                }
            }

            assert_eq!(
                result.count,
                offset + 1,
                "MPMC producer reached MPMC_MAX_ATTEMPTS"
            );
        }

        result
    }
}

fn mpmc_consumer(mut env: Environment) -> MpmcResult {
    wait_for_mpmc_initialization(&mut env);

    let mut result = MpmcResult::default();
    for offset in 0..MPMC_VALUES_PER_THREAD {
        for _ in 0..MPMC_MAX_ATTEMPTS {
            if let Some(entry) = mpmc_try_pop(&mut env) {
                result.entries[result.count] = entry;
                result.count += 1;
                break;
            }
        }

        assert_eq!(
            result.count,
            offset + 1,
            "MPMC consumer reached MPMC_MAX_ATTEMPTS"
        );
    }

    result
}

fn mpmc_values_in_position_order(results: &[MpmcResult], case: &str) -> [usize; MPMC_VALUE_COUNT] {
    let mut values = [0; MPMC_VALUE_COUNT];
    let mut seen = [false; MPMC_VALUE_COUNT];

    for result in results {
        assert_eq!(result.count, MPMC_VALUES_PER_THREAD, "{case}: {results:?}");

        for entry in result.entries {
            assert!(entry.position < MPMC_VALUE_COUNT, "{case}: {results:?}");
            assert!(!seen[entry.position], "{case}: {results:?}");
            seen[entry.position] = true;
            values[entry.position] = entry.value;
        }
    }

    assert!(seen.into_iter().all(|value| value), "{case}: {results:?}");
    values
}

fn execution_seeds(seed: u64) -> (u64, u64) {
    let mut seeds = ChaCha8Rng::seed_from_u64(seed);
    (seeds.next_u64(), seeds.next_u64())
}

#[test]
fn spsc_queue_preserves_fifo_under_contention() {
    for seed in 0..EXECUTION_COUNT {
        let mut test = LogTest::default();
        test.add(spsc_producer);
        test.add(spsc_consumer);

        let (scheduler_seed, memory_seed) = execution_seeds(seed);
        let results = test.run_with_seed(scheduler_seed, memory_seed);
        let case =
            format!("seed={seed}, scheduler_seed={scheduler_seed}, memory_seed={memory_seed}");

        let expected = std::array::from_fn(|index| index + 1);

        assert_eq!(results[0].pushed, SPSC_VALUE_COUNT, "{case}: {results:?}");
        assert_eq!(results[1].popped, SPSC_VALUE_COUNT, "{case}: {results:?}");
        assert_eq!(results[1].values, expected, "{case}: {results:?}");
    }
}

#[test]
fn mpmc_queue_preserves_fifo_and_ownership_under_contention() {
    for seed in 0..EXECUTION_COUNT {
        let mut test = LogTest::default();
        for producer in 0..MPMC_PRODUCER_COUNT {
            test.add(mpmc_producer(producer, producer == 0));
        }
        for _ in 0..MPMC_CONSUMER_COUNT {
            test.add(mpmc_consumer);
        }

        let (scheduler_seed, memory_seed) = execution_seeds(seed);
        let results = test.run_with_seed(scheduler_seed, memory_seed);
        let case =
            format!("seed={seed}, scheduler_seed={scheduler_seed}, memory_seed={memory_seed}");
        let produced = mpmc_values_in_position_order(&results[..MPMC_PRODUCER_COUNT], &case);
        let consumed = mpmc_values_in_position_order(&results[MPMC_PRODUCER_COUNT..], &case);
        let mut produced_values = produced;
        produced_values.sort_unstable();
        let expected_values = std::array::from_fn(|index| index + 1);

        assert_eq!(produced_values, expected_values, "{case}: {results:?}");
        assert_eq!(consumed, produced, "{case}: {results:?}");
    }
}
