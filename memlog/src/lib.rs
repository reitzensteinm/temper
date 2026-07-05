pub mod log;
pub mod memgraph;

// The `rc11` feature enables the memgraph backend through Cargo feature
// dependencies. Keep this switch on `memgraph` so backend benchmarking can
// still select memgraph without also opting into RC11-labeled tests.
#[cfg(not(feature = "memgraph"))]
pub mod backend {
    pub use crate::log::MemorySystem;
}

#[cfg(feature = "memgraph")]
pub mod backend {
    pub use crate::memgraph::MemorySystem;
}
