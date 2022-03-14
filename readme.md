# Temper.rs

Todo

1) Store/Release
2) Fences with Access Levels
3) Lock Free Concurrent Queue
4) How do we have multiple systems, e.g. Disk + Memory
5) Drop in replacement API that doesn't test in release
6) TCP
7) Disk with fsync, fsync on directories
8) LSM trees with merge
9) CRDTs
10) Visualization exporting
11) Memory that's not coherent
12) Locks
14) Netsplits
15) False sharing analysis?
16) Test determinism and fix seeds
17) Fuzz corrupted messages/disk

**Queue Sprint**
* Build industrial queue
* Mechanism for crate swapping
* Deterministic testing with seeds and reproducibility
* Acquire/Release semantics + Fences

**Disk Sprint**
* System sharing
* TCP
* Disk w/ fsync
* Get/Set with LSM server and client

**Low Level Sprint**
* Locks
* CAS
* Non-coherent memory models
* Spin forever under contention

**Distributed Systems & CRDT Sprint**
* Model simple CRDT
* Turning off network connections?
* Implement Raft
* Netsplit
