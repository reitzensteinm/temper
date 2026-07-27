use crate::common::harness::{Environment, LogTest, Value};
use rand::{RngCore, SeedableRng};
use rand_chacha::ChaCha8Rng;
use std::sync::atomic::Ordering;

#[allow(dead_code)]
mod common;

const SLOT_COUNT: usize = 3;
const PRODUCER_COUNT: usize = 2;
const CONSUMER_COUNT: usize = 2;
const CONCURRENT_RUNS: u64 = 1_000;

#[derive(Clone, Copy, Debug, Default)]
struct ThreadResult {
    inserted: usize,
    taken: usize,
    take_count: usize,
}

fn slots(env: &mut Environment) -> [&mut Value; SLOT_COUNT] {
    [&mut env.a, &mut env.b, &mut env.c]
}

fn producer(value: usize) -> impl FnMut(Environment) -> ThreadResult {
    move |mut env| {
        let inserted = slots(&mut env).into_iter().any(|slot| {
            slot.exchange(0, value, Ordering::AcqRel, Ordering::Relaxed)
                .is_ok()
        });

        ThreadResult {
            inserted: usize::from(inserted) << value,
            ..ThreadResult::default()
        }
    }
}

fn consumer(mut env: Environment) -> ThreadResult {
    slots(&mut env)
        .into_iter()
        .map(|slot| slot.swap(0, Ordering::AcqRel))
        .filter(|value| *value != 0)
        .fold(ThreadResult::default(), |mut result, value| {
            result.taken |= 1 << value;
            result.take_count += 1;
            result
        })
}

fn execution_seeds(seed: u64) -> (u64, u64) {
    let mut seeds = ChaCha8Rng::seed_from_u64(seed);
    (seeds.next_u64(), seeds.next_u64())
}

#[test]
fn lock_free_array_completes_and_reuses_slots() {
    let mut test = LogTest::default();
    test.add(producer(1));
    test.add(producer(2));
    test.add(consumer);
    test.add(producer(3));
    test.add(consumer);

    let results = test.run_sequential();
    assert_eq!(results[0].inserted, 1 << 1);
    assert_eq!(results[1].inserted, 1 << 2);
    assert_eq!(results[2].taken, (1 << 1) | (1 << 2), "{results:?}");
    assert_eq!(results[2].take_count, 2);
    assert_eq!(results[3].inserted, 1 << 3);
    assert_eq!(results[4].taken, 1 << 3, "{results:?}");
    assert_eq!(results[4].take_count, 1);
}

#[test]
fn lock_free_array_preserves_ownership_under_contention() {
    for seed in 0..CONCURRENT_RUNS {
        let mut test = LogTest::default();
        for value in 1..=PRODUCER_COUNT {
            test.add(producer(value));
        }
        for _ in 0..CONSUMER_COUNT {
            test.add(consumer);
        }

        let (scheduler_seed, memory_seed) = execution_seeds(seed);
        let results = test.run_with_seed(scheduler_seed, memory_seed);
        let inserted = results[..PRODUCER_COUNT]
            .iter()
            .fold(0, |values, result| values | result.inserted);
        let consumers = &results[PRODUCER_COUNT..];
        let taken = consumers
            .iter()
            .fold(0, |values, result| values | result.taken);
        let take_count = consumers
            .iter()
            .map(|result| result.take_count)
            .sum::<usize>();
        let case =
            format!("seed={seed}, scheduler_seed={scheduler_seed}, memory_seed={memory_seed}");

        assert_eq!(inserted, (1 << 1) | (1 << 2), "{case}");
        assert_eq!(taken & !inserted, 0, "{case}");
        assert_eq!(taken.count_ones() as usize, take_count, "{case}");
    }
}
