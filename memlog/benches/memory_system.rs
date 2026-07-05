use memlog::backend::MemorySystem;
use std::hint::black_box;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

const ITERATION_COUNTS: [usize; 4] = [10, 100, 1_000, 10_000];

#[cfg(feature = "rc11")]
const BACKEND: &str = "rc11";

#[cfg(all(feature = "memgraph", not(feature = "rc11")))]
const BACKEND: &str = "memgraph";

#[cfg(not(feature = "memgraph"))]
const BACKEND: &str = "log";

struct BenchResult {
    name: &'static str,
    iterations: usize,
    elapsed: Duration,
    checksum: usize,
}

struct BenchCase {
    name: &'static str,
    bench: fn(usize) -> usize,
}

fn main() {
    let cases = [
        BenchCase {
            name: "same_thread_relaxed",
            bench: same_thread_relaxed,
        },
        BenchCase {
            name: "relaxed_history_reads",
            bench: relaxed_history_reads,
        },
        BenchCase {
            name: "release_acquire_handoff",
            bench: release_acquire_handoff,
        },
        BenchCase {
            name: "seq_cst_exchange",
            bench: seq_cst_exchange,
        },
    ];

    println!("backend: {BACKEND}");
    println!("iterations: {}", format_iterations(&ITERATION_COUNTS));
    println!("case,iterations,total_ms,ns_per_iter,checksum");

    for iterations in ITERATION_COUNTS {
        for case in &cases {
            let result = run(iterations, case);
            let ns_per_iter = result.elapsed.as_nanos() as f64 / result.iterations as f64;

            println!(
                "{},{},{:.3},{:.1},{}",
                result.name,
                result.iterations,
                result.elapsed.as_secs_f64() * 1_000.0,
                ns_per_iter,
                result.checksum
            );
        }
    }
}

fn format_iterations(iterations: &[usize]) -> String {
    iterations
        .iter()
        .map(|count| count.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

fn run(iterations: usize, case: &BenchCase) -> BenchResult {
    let start = Instant::now();
    let checksum = (case.bench)(iterations);
    let elapsed = start.elapsed();

    BenchResult {
        name: case.name,
        iterations,
        elapsed,
        checksum: black_box(checksum),
    }
}

fn same_thread_relaxed(iterations: usize) -> usize {
    let mut mem = MemorySystem::default();
    let addr = mem.malloc(1);
    let thread = mem.add_thread();
    let mut checksum = 0;

    for i in 0..iterations {
        mem.store(thread, addr, i, Ordering::Relaxed);
        checksum ^= mem.load(thread, addr, Ordering::Relaxed);
    }

    checksum
}

fn relaxed_history_reads(iterations: usize) -> usize {
    let mut mem = MemorySystem::default();
    let addr = mem.malloc(1);
    let writer = mem.add_thread();
    let reader = mem.add_thread();
    let mut checksum = 0usize;

    for i in 0..iterations {
        mem.store(writer, addr, i, Ordering::Relaxed);
    }

    for _ in 0..iterations {
        checksum = checksum.wrapping_add(mem.load(reader, addr, Ordering::Relaxed));
    }

    checksum
}

fn release_acquire_handoff(iterations: usize) -> usize {
    let mut mem = MemorySystem::default();
    let base = mem.malloc(2);
    let data = base;
    let flag = base + 1;
    let writer = mem.add_thread();
    let reader = mem.add_thread();
    let mut checksum = 0usize;

    for i in 1..=iterations {
        mem.store(writer, data, i, Ordering::Relaxed);
        mem.store(writer, flag, i, Ordering::Release);
        checksum = checksum.wrapping_add(mem.load(reader, flag, Ordering::Acquire));
        checksum = checksum.wrapping_add(mem.load(reader, data, Ordering::Relaxed));
    }

    checksum
}

fn seq_cst_exchange(iterations: usize) -> usize {
    let mut mem = MemorySystem::default();
    let base = mem.malloc(2);
    let left_addr = base;
    let right_addr = base + 1;
    let left = mem.add_thread();
    let right = mem.add_thread();
    let mut checksum = 0usize;

    for i in 1..=iterations {
        mem.store(left, left_addr, i, Ordering::SeqCst);
        checksum = checksum.wrapping_add(mem.load(right, left_addr, Ordering::SeqCst));
        mem.store(right, right_addr, i, Ordering::SeqCst);
        checksum = checksum.wrapping_add(mem.load(left, right_addr, Ordering::SeqCst));
    }

    checksum
}
