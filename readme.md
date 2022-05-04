# Temper.rs

### Memlog

* Note: Threads cannot send information forward in time
* Add volatile and detect torn reads / writes
* Detect data races https://en.cppreference.com/w/cpp/language/memory_model
* Expose API to declare what can be reordered

### Sprints

**Memlog Sprint**
* Finish Memlog tasks
* Implement Memlog backed version of Temper

**Queue Sprint**
* Build industrial queue
* Mechanism for crate swapping for release
* Deterministic testing with seeds and reproducibility

**Disk Sprint**
* System sharing
* TCP
* Disk w/ fsync, on dirs
* Get/Set with LSM server and client

**Low Level Sprint**
* Acquire/Release semantics for Atomics, Fences
* Locks
* CAS
* Non-coherent memory models
* Spin forever under contention

**Distributed Systems & CRDT Sprint**
* Model simple CRDT
* Turning off network connections?
* Implement Raft
* Netsplit

### Misc Ideas

1) Visualization exporting
2) Netsplits
3) False sharing analysis?
4) Fuzz corrupted messages/disk
5) Guards or linting to ensure we don't immediately consume values
6) Detect cache line contention