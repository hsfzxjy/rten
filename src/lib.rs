mod graph;
mod linalg;
mod model;
mod number;
mod timer;
mod wasm_api;

// Temporarily included in this crate. These functions should be moved into
// a separate crate in future.
pub mod ctc;

pub mod ops;

pub use graph::{Dimension, RunOptions};
pub use model::Model;
pub use ops::{Input, Output};
pub use timer::Timer;

#[allow(clippy::extra_unused_lifetimes, dead_code, unused_imports)]
mod schema_generated;

mod model_builder;
pub use model_builder::{ModelBuilder, OpType};
