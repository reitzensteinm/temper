use crate::backend::MemoryBackend;
use crate::graph::{self, SeqCstMode};
use crate::log;
use rand::seq::SliceRandom;
use rand::{Rng, RngCore, SeedableRng};
use rand_chacha::ChaCha8Rng;
use std::collections::BTreeSet;
use std::fmt::{self, Write};
use std::sync::atomic::Ordering;

const CONFIRMATION_RUNS: usize = 100_000;

#[derive(Clone, Copy, Debug)]
struct Config {
    seed: u64,
    cases: usize,
    max_operations: usize,
    runs_per_case: usize,
    ordering_mode: OrderingMode,
}

#[derive(Clone, Copy, Debug)]
enum OrderingMode {
    All,
    OnlySeqCst,
    WithoutSeqCst,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Operation {
    Load {
        thread: usize,
        address: usize,
        ordering: Ordering,
    },
    Store {
        thread: usize,
        address: usize,
        value: usize,
        ordering: Ordering,
    },
    CompareExchange {
        thread: usize,
        address: usize,
        current: usize,
        new: usize,
        success: Ordering,
        failure: Ordering,
        weak: bool,
    },
    FetchOp {
        thread: usize,
        address: usize,
        increment: usize,
        ordering: Ordering,
    },
    FetchUpdate {
        thread: usize,
        address: usize,
        increment: Option<usize>,
        set_order: Ordering,
        fetch_order: Ordering,
    },
    Fence {
        thread: usize,
        ordering: Ordering,
    },
}

#[derive(Clone, Copy, Debug)]
enum OperationKind {
    Load,
    Store,
    CompareExchange {
        expectation: CompareExchangeExpectation,
        weak: bool,
    },
    FetchOp,
    FetchUpdate,
    Fence,
}

#[derive(Clone, Copy, Debug)]
enum CompareExchangeExpectation {
    MustFail,
    Either,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum Observation {
    Value(usize),
    Result(Result<usize, usize>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Program {
    threads: usize,
    addresses: usize,
    operations: Vec<Operation>,
}

#[derive(Clone, Copy, Debug)]
enum Backend {
    Log,
    Graph,
}

impl fmt::Display for Backend {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Log => output.write_str("log"),
            Self::Graph => output.write_str("graph"),
        }
    }
}

#[derive(Default)]
struct Observations {
    log: BTreeSet<Vec<Observation>>,
    graph: BTreeSet<Vec<Observation>>,
    log_runs: usize,
    graph_runs: usize,
}

#[derive(Debug)]
struct Difference {
    case: usize,
    seed: u64,
    source: Backend,
    outcome: Vec<Observation>,
    log_runs: usize,
    graph_runs: usize,
    program: Program,
}

impl fmt::Display for Difference {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            output,
            "potential backend difference in case {} (seed {})",
            self.case, self.seed
        )?;
        writeln!(output, "{}", self.program)?;
        write!(
            output,
            "{} produced {:?}; the other backend did not reproduce it \
             after {} log runs and {} graph runs",
            self.source, self.outcome, self.log_runs, self.graph_runs
        )
    }
}

impl std::error::Error for Difference {}

fn run(config: Config) -> Result<(), Difference> {
    assert!(config.cases > 0, "cases must be greater than zero");
    assert!(
        config.max_operations >= 8,
        "max_operations must be at least eight"
    );
    assert!(
        config.runs_per_case > 0,
        "runs_per_case must be greater than zero"
    );

    let mut program_rng = ChaCha8Rng::seed_from_u64(config.seed);
    let mut execution_rng = ChaCha8Rng::seed_from_u64(config.seed);
    execution_rng.set_stream(1);

    for case in 0..config.cases {
        let program = Program::generate(
            &mut program_rng,
            config.max_operations,
            config.ordering_mode,
        );
        let mut observations = Observations::default();
        probe(
            &program,
            config.runs_per_case,
            &mut observations,
            &mut execution_rng,
        );

        if let Some((source, outcome)) = confirm(&program, &mut observations, &mut execution_rng) {
            return Err(Difference {
                case,
                seed: config.seed,
                source,
                outcome,
                log_runs: observations.log_runs,
                graph_runs: observations.graph_runs,
                program,
            });
        }
    }

    Ok(())
}

fn probe(
    program: &Program,
    runs: usize,
    observations: &mut Observations,
    execution_rng: &mut ChaCha8Rng,
) {
    for _ in 0..runs {
        let seed = execution_rng.next_u64();
        observe_log(program, observations, seed);
        observe_graph(program, observations, seed);
    }
}

fn confirm(
    program: &Program,
    observations: &mut Observations,
    execution_rng: &mut ChaCha8Rng,
) -> Option<(Backend, Vec<Observation>)> {
    loop {
        if let Some(outcome) = observations
            .log
            .difference(&observations.graph)
            .next()
            .cloned()
        {
            if !find_in_graph(program, &outcome, observations, execution_rng) {
                return Some((Backend::Log, outcome));
            }
            continue;
        }

        if let Some(outcome) = observations
            .graph
            .difference(&observations.log)
            .next()
            .cloned()
        {
            if !find_in_log(program, &outcome, observations, execution_rng) {
                return Some((Backend::Graph, outcome));
            }
            continue;
        }

        return None;
    }
}

fn find_in_log(
    program: &Program,
    expected: &[Observation],
    observations: &mut Observations,
    execution_rng: &mut ChaCha8Rng,
) -> bool {
    for _ in 0..CONFIRMATION_RUNS {
        if observe_log(program, observations, execution_rng.next_u64()) == expected {
            return true;
        }
    }
    false
}

fn find_in_graph(
    program: &Program,
    expected: &[Observation],
    observations: &mut Observations,
    execution_rng: &mut ChaCha8Rng,
) -> bool {
    for _ in 0..CONFIRMATION_RUNS {
        if observe_graph(program, observations, execution_rng.next_u64()) == expected {
            return true;
        }
    }
    false
}

fn observe_log(program: &Program, observations: &mut Observations, seed: u64) -> Vec<Observation> {
    let outcome = execute(log::MemorySystem::default().with_seed(seed), program);
    observations.log_runs += 1;
    observations.log.insert(outcome.clone());
    outcome
}

fn observe_graph(
    program: &Program,
    observations: &mut Observations,
    seed: u64,
) -> Vec<Observation> {
    let outcome = execute(
        graph::MemorySystem::with_seq_cst_mode(SeqCstMode::Cxx11).with_seed(seed),
        program,
    );
    observations.graph_runs += 1;
    observations.graph.insert(outcome.clone());
    outcome
}

fn execute<B: MemoryBackend>(mut memory: B, program: &Program) -> Vec<Observation> {
    let base = memory.malloc(program.addresses);
    let threads: Vec<_> = (0..program.threads).map(|_| memory.add_thread()).collect();
    let mut outcome = vec![];

    for operation in &program.operations {
        match *operation {
            Operation::Load {
                thread,
                address,
                ordering,
            } => outcome.push(Observation::Value(memory.load(
                threads[thread],
                base + address,
                ordering,
            ))),
            Operation::Store {
                thread,
                address,
                value,
                ordering,
            } => memory.store(threads[thread], base + address, value, ordering),
            Operation::CompareExchange {
                thread,
                address,
                current,
                new,
                success,
                failure,
                weak,
            } => {
                let address = base + address;
                let result = if weak {
                    memory.compare_exchange_weak(
                        threads[thread],
                        address,
                        current,
                        new,
                        success,
                        failure,
                    )
                } else {
                    memory.compare_exchange(
                        threads[thread],
                        address,
                        current,
                        new,
                        success,
                        failure,
                    )
                };
                outcome.push(Observation::Result(result));
            }
            Operation::FetchOp {
                thread,
                address,
                increment,
                ordering,
            } => outcome.push(Observation::Value(memory.fetch_op(
                threads[thread],
                base + address,
                &|value| value.wrapping_add(increment),
                ordering,
            ))),
            Operation::FetchUpdate {
                thread,
                address,
                increment,
                set_order,
                fetch_order,
            } => {
                let result = memory.fetch_update(
                    threads[thread],
                    base + address,
                    &|value| increment.map(|increment| value.wrapping_add(increment)),
                    set_order,
                    fetch_order,
                );
                outcome.push(Observation::Result(result));
            }
            Operation::Fence { thread, ordering } => memory.fence(threads[thread], ordering),
        }
    }

    outcome
}

impl Program {
    fn generate(rng: &mut ChaCha8Rng, max_operations: usize, ordering_mode: OrderingMode) -> Self {
        let operation_count = rng.gen_range(8..=max_operations);
        let threads = 4.min(operation_count);
        let addresses = rng.gen_range(1..=3.min(operation_count));
        let mut initial_threads: Vec<_> = (0..threads).collect();
        initial_threads.shuffle(rng);
        let mut initial_addresses: Vec<_> = (0..addresses).collect();
        initial_addresses.shuffle(rng);
        let mut operation_kinds = vec![
            OperationKind::CompareExchange {
                expectation: CompareExchangeExpectation::MustFail,
                weak: false,
            },
            OperationKind::Load,
            OperationKind::Store,
            OperationKind::Fence,
            OperationKind::FetchOp,
            OperationKind::FetchUpdate,
        ];
        while operation_kinds.len() < operation_count - 2 {
            operation_kinds.push(match rng.gen_range(0..7) {
                0 => OperationKind::Load,
                1 => OperationKind::Store,
                2 => OperationKind::CompareExchange {
                    expectation: CompareExchangeExpectation::Either,
                    weak: false,
                },
                3 => OperationKind::CompareExchange {
                    expectation: CompareExchangeExpectation::Either,
                    weak: true,
                },
                4 => OperationKind::Fence,
                5 => OperationKind::FetchOp,
                _ => OperationKind::FetchUpdate,
            });
        }
        operation_kinds.shuffle(rng);
        let mut next_value = 1;
        let mut operations = Vec::with_capacity(operation_count);

        let first_thread = initial_threads[0];
        let first_address = initial_addresses[0];
        let (success, failure) = ordering_mode.random_compare_exchange_orderings(rng);
        operations.push(Operation::CompareExchange {
            thread: first_thread,
            address: first_address,
            current: 0,
            new: next_value,
            success,
            failure,
            weak: false,
        });
        let current = next_value;
        next_value += 1;

        let (success, failure) = ordering_mode.random_compare_exchange_orderings(rng);
        operations.push(Operation::CompareExchange {
            thread: initial_threads[1],
            address: first_address,
            current,
            new: next_value,
            success,
            failure,
            weak: true,
        });
        next_value += 1;

        for (offset, operation_kind) in operation_kinds.into_iter().enumerate() {
            let index = offset + 2;
            let thread = initial_threads
                .get(index)
                .copied()
                .unwrap_or_else(|| rng.gen_range(0..threads));
            let address = initial_addresses
                .get(index)
                .copied()
                .unwrap_or_else(|| rng.gen_range(0..addresses));

            operations.push(match operation_kind {
                OperationKind::Load => Operation::Load {
                    thread,
                    address,
                    ordering: ordering_mode.random_load_ordering(rng),
                },
                OperationKind::Store => {
                    let operation = Operation::Store {
                        thread,
                        address,
                        value: next_value,
                        ordering: ordering_mode.random_store_ordering(rng),
                    };
                    next_value += 1;
                    operation
                }
                OperationKind::CompareExchange { expectation, weak } => {
                    let (success, failure) = ordering_mode.random_compare_exchange_orderings(rng);
                    let operation = Operation::CompareExchange {
                        thread,
                        address,
                        current: match expectation {
                            CompareExchangeExpectation::MustFail => usize::MAX,
                            CompareExchangeExpectation::Either => rng.gen_range(0..next_value),
                        },
                        new: next_value,
                        success,
                        failure,
                        weak,
                    };
                    next_value += 1;
                    operation
                }
                OperationKind::FetchOp => Operation::FetchOp {
                    thread,
                    address,
                    increment: rng.gen_range(1..=3),
                    ordering: ordering_mode.random_rmw_ordering(rng),
                },
                OperationKind::FetchUpdate => {
                    let (set_order, fetch_order) =
                        ordering_mode.random_compare_exchange_orderings(rng);
                    Operation::FetchUpdate {
                        thread,
                        address,
                        increment: rng.gen_bool(0.5).then(|| rng.gen_range(1..=3)),
                        set_order,
                        fetch_order,
                    }
                }
                OperationKind::Fence => Operation::Fence {
                    thread,
                    ordering: ordering_mode.random_fence_ordering(rng),
                },
            });
        }

        Self {
            threads,
            addresses,
            operations,
        }
    }
}

impl OrderingMode {
    fn random_load_ordering(self, rng: &mut ChaCha8Rng) -> Ordering {
        let orderings: &[Ordering] = match self {
            Self::All => &LOAD_ORDERINGS,
            Self::OnlySeqCst => &SEQ_CST_ORDERING,
            Self::WithoutSeqCst => &LOAD_ORDERINGS_WITHOUT_SEQ_CST,
        };
        orderings[rng.gen_range(0..orderings.len())]
    }

    fn random_store_ordering(self, rng: &mut ChaCha8Rng) -> Ordering {
        let orderings: &[Ordering] = match self {
            Self::All => &STORE_ORDERINGS,
            Self::OnlySeqCst => &SEQ_CST_ORDERING,
            Self::WithoutSeqCst => &STORE_ORDERINGS_WITHOUT_SEQ_CST,
        };
        orderings[rng.gen_range(0..orderings.len())]
    }

    fn random_compare_exchange_orderings(self, rng: &mut ChaCha8Rng) -> (Ordering, Ordering) {
        let orderings: &[(Ordering, Ordering)] = match self {
            Self::All => &COMPARE_EXCHANGE_ORDERINGS,
            Self::OnlySeqCst => &SEQ_CST_COMPARE_EXCHANGE_ORDERING,
            Self::WithoutSeqCst => &COMPARE_EXCHANGE_ORDERINGS_WITHOUT_SEQ_CST,
        };
        orderings[rng.gen_range(0..orderings.len())]
    }

    fn random_rmw_ordering(self, rng: &mut ChaCha8Rng) -> Ordering {
        let orderings: &[Ordering] = match self {
            Self::All => &RMW_ORDERINGS,
            Self::OnlySeqCst => &SEQ_CST_ORDERING,
            Self::WithoutSeqCst => &RMW_ORDERINGS_WITHOUT_SEQ_CST,
        };
        orderings[rng.gen_range(0..orderings.len())]
    }

    fn random_fence_ordering(self, rng: &mut ChaCha8Rng) -> Ordering {
        let orderings: &[Ordering] = match self {
            Self::All => &FENCE_ORDERINGS,
            Self::OnlySeqCst => &SEQ_CST_ORDERING,
            Self::WithoutSeqCst => &FENCE_ORDERINGS_WITHOUT_SEQ_CST,
        };
        orderings[rng.gen_range(0..orderings.len())]
    }
}

const LOAD_ORDERINGS: [Ordering; 3] = [Ordering::Relaxed, Ordering::Acquire, Ordering::SeqCst];
const STORE_ORDERINGS: [Ordering; 3] = [Ordering::Relaxed, Ordering::Release, Ordering::SeqCst];
const SEQ_CST_ORDERING: [Ordering; 1] = [Ordering::SeqCst];
const LOAD_ORDERINGS_WITHOUT_SEQ_CST: [Ordering; 2] = [Ordering::Relaxed, Ordering::Acquire];
const STORE_ORDERINGS_WITHOUT_SEQ_CST: [Ordering; 2] = [Ordering::Relaxed, Ordering::Release];
const RMW_ORDERINGS: [Ordering; 5] = [
    Ordering::Relaxed,
    Ordering::Acquire,
    Ordering::Release,
    Ordering::AcqRel,
    Ordering::SeqCst,
];
const RMW_ORDERINGS_WITHOUT_SEQ_CST: [Ordering; 4] = [
    Ordering::Relaxed,
    Ordering::Acquire,
    Ordering::Release,
    Ordering::AcqRel,
];
const FENCE_ORDERINGS: [Ordering; 4] = [
    Ordering::Acquire,
    Ordering::Release,
    Ordering::AcqRel,
    Ordering::SeqCst,
];
const FENCE_ORDERINGS_WITHOUT_SEQ_CST: [Ordering; 3] =
    [Ordering::Acquire, Ordering::Release, Ordering::AcqRel];
const COMPARE_EXCHANGE_ORDERINGS: [(Ordering, Ordering); 9] = [
    (Ordering::Relaxed, Ordering::Relaxed),
    (Ordering::Acquire, Ordering::Relaxed),
    (Ordering::Acquire, Ordering::Acquire),
    (Ordering::Release, Ordering::Relaxed),
    (Ordering::AcqRel, Ordering::Relaxed),
    (Ordering::AcqRel, Ordering::Acquire),
    (Ordering::SeqCst, Ordering::Relaxed),
    (Ordering::SeqCst, Ordering::Acquire),
    (Ordering::SeqCst, Ordering::SeqCst),
];
const SEQ_CST_COMPARE_EXCHANGE_ORDERING: [(Ordering, Ordering); 1] =
    [(Ordering::SeqCst, Ordering::SeqCst)];
const COMPARE_EXCHANGE_ORDERINGS_WITHOUT_SEQ_CST: [(Ordering, Ordering); 6] = [
    (Ordering::Relaxed, Ordering::Relaxed),
    (Ordering::Acquire, Ordering::Relaxed),
    (Ordering::Acquire, Ordering::Acquire),
    (Ordering::Release, Ordering::Relaxed),
    (Ordering::AcqRel, Ordering::Relaxed),
    (Ordering::AcqRel, Ordering::Acquire),
];

impl fmt::Display for Program {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            output,
            "program ({} threads, {} addresses):",
            self.threads, self.addresses
        )?;

        let mut text = String::new();
        for (index, operation) in self.operations.iter().enumerate() {
            text.clear();
            match operation {
                Operation::Load {
                    thread,
                    address,
                    ordering,
                } => write!(text, "load  t{thread} a{address} {ordering:?}").unwrap(),
                Operation::Store {
                    thread,
                    address,
                    value,
                    ordering,
                } => write!(text, "store t{thread} a{address} = {value} {ordering:?}").unwrap(),
                Operation::CompareExchange {
                    thread,
                    address,
                    current,
                    new,
                    success,
                    failure,
                    weak,
                } => {
                    let name = if *weak {
                        "compare_exchange_weak"
                    } else {
                        "compare_exchange"
                    };
                    write!(
                        text,
                        "{name} t{thread} a{address} {current} -> {new} \
                         success={success:?} failure={failure:?}"
                    )
                    .unwrap()
                }
                Operation::FetchOp {
                    thread,
                    address,
                    increment,
                    ordering,
                } => write!(
                    text,
                    "fetch_op t{thread} a{address} += {increment} {ordering:?}"
                )
                .unwrap(),
                Operation::FetchUpdate {
                    thread,
                    address,
                    increment,
                    set_order,
                    fetch_order,
                } => {
                    let update = increment
                        .map_or_else(|| "reject".to_owned(), |increment| format!("+={increment}"));
                    write!(
                        text,
                        "fetch_update t{thread} a{address} {update} \
                         set={set_order:?} fetch={fetch_order:?}"
                    )
                    .unwrap()
                }
                Operation::Fence { thread, ordering } => {
                    write!(text, "fence t{thread} {ordering:?}").unwrap()
                }
            }
            writeln!(output, "  {index}: {text}")?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CASES_PER_MODE: usize = 20_000;
    const MAX_OPERATIONS: usize = 10;
    const RUNS_PER_CASE: usize = 100;

    #[test]
    #[ignore = "long-running randomized equivalence campaign"]
    fn fuzz_log_against_graph_in_cxx11_mode() {
        let initial_seed = std::env::var("MEMLOG_EQUIVALENCE_SEED")
            .map(|seed| seed.parse().expect("MEMLOG_EQUIVALENCE_SEED must be a u64"))
            .unwrap_or_else(|_| std::time::UNIX_EPOCH.elapsed().unwrap().as_nanos() as u64);

        for (index, ordering_mode) in [
            OrderingMode::All,
            OrderingMode::OnlySeqCst,
            OrderingMode::WithoutSeqCst,
        ]
        .into_iter()
        .enumerate()
        {
            let seed = initial_seed.wrapping_add(index as u64);
            println!(
                "seed={seed} cases={CASES_PER_MODE} max_operations={MAX_OPERATIONS} \
                 ordering_mode={ordering_mode:?} runs_per_case={RUNS_PER_CASE}"
            );

            if let Err(difference) = run(Config {
                seed,
                cases: CASES_PER_MODE,
                max_operations: MAX_OPERATIONS,
                runs_per_case: RUNS_PER_CASE,
                ordering_mode,
            }) {
                panic!("{difference}");
            }
        }
    }
}
