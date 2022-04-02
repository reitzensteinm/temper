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

### Things to consider

1) Do we go with LoadLoad, StoreStore etc barrier designation?
2) How is a LoadStore barrier different to Sequential Consistency?
3) Do we need to rename atomics?
4) Detecting sheared shared memory buffer?
5) ARM has dependent load memory ordering
6) Use condition variables to properly sleep threads