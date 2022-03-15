# Temper.rs

### Sprints

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