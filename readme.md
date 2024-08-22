# Temper

## About

Temper is a framework for modelling concurrency and failure in distributed systems. The name comes from Temporal Fuzzing, a term coined by [Rachel Kroll](https://rachelbythebay.com/w/2011/11/19/sleep/).

Similar to [Loom](https://github.com/tokio-rs/loom), programs targeting Temper should be able to switch between simulation mode, and calling real APIs in release mode.

It is in early development, and is not yet more than an experiment. It currently features:

* Low level x86/ARM memory models
* Rust/C++ 11 memory model

Planned features:

* MESI protocol simulation to measure cache line contention and false sharing
* Data race detection
* TCP/IP, including congestion, asymmetric net splits, and Byzantine faults
* Disk operations, including fsync and [power failure corruption](https://danluu.com/file-consistency/)
* SQL transactional isolation

Related Work:

* [Madsim](https://github.com/madsim-rs/madsim), a similar project with an emphasis on networking
* FoundationDB's [testing strategy](https://www.youtube.com/watch?v=4fFDFbi3toc)
* TigerBeetle's [fault injection](https://www.youtube.com/watch?v=BH2jvJ74npM) testing
* [Loom](https://github.com/tokio-rs/loom), which exhaustively tests on a single node
* [Timecraft](https://github.com/stealthrocket/timecraft), a distributed system testing tool for WebAssembly
* [Coyote](https://microsoft.github.io/coyote), a similar project for the .Net runtime
* [Antithesis](https://antithesis.com/), a deterministic hypervisor that can test arbitrary software
* [Turmoil](https://github.com/tokio-rs/turmoil), a testing framework for Tokio that also does network fault injection
* FrostDB's [testing strategy](https://www.polarsignals.com/blog/posts/2024/05/28/mostly-dst-in-go), compiling Go to
  WASM

Reading:

* [Files are fraught with peril](https://danluu.com/deconstruct-files/) by Dan Luu
* [What's the big deal about Deterministic Simulation Testing?](https://notes.eatonphil.com/2024-08-20-deterministic-simulation-testing.html)
  by Phil Eaton

## Components

### Memlog

Memlog simulates the Rust memory model (C++ 11 without Consume). Combined with operation reordering in Temper, its goal is full coverage. It contains a series of test cases dervied from [Preshing on Programming](https://preshing.com/), [C++ Concurrency in Action](https://www.amazon.com.au/C-Concurrency-Action-Practical-Multithreading/dp/1933988770), the [C++ Standard](https://en.cppreference.com/w/cpp/atomic/atomic_thread_fence), [blog posts](https://puzpuzpuz.dev/seqlock-based-atomic-memory-snapshots) and [many](https://stackoverflow.com/questions/47520748/c-memory-model-do-seq-cst-loads-synchronize-with-seq-cst-stores) [Stack](https://stackoverflow.com/questions/52606524/what-exact-rules-in-the-c-memory-model-prevent-reordering-before-acquire-opera) [Overflow](https://stackoverflow.com/questions/71509935/how-does-mixing-relaxed-and-acquire-release-accesses-on-the-same-atomic-variable) [questions](https://stackoverflow.com/questions/67693687/possible-orderings-with-memory-order-seq-cst-and-memory-order-release).

Todo:
* Detect [data races](https://en.cppreference.com/w/cpp/language/memory_model) in non-atomic datatypes
* Expose API to declare what can be reordered
* MESI protocol simulation
* Locks
* Seeded randomness
* Reentry support for fetch_update
* Support multiple datatypes

### Low Level

Temper contains a low level simulation of x86/ARM memory models. It is intended for experimentation, as the operations cannot be translated to standard Rust calls in release mode.

Todo: 
* Non-coherent memory models (Alpha)
* Locks
* CAS
* Platform specific barriers
* Spin forever under contention

### Future Work

* Crate swap mechanism for release
* Sample lock free algorithms, such as a MPMC queue
* Deterministic testing with seeds and reproducibility
* Disk w/ fsync, power failure, corruption
* Sample Disk LSM system
* TCP with net splits, latency and Byzantine faults
* Sample Raft protocol
* Visualisation