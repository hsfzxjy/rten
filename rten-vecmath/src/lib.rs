//! SIMD-vectorized implementations of various math functions that are commonly
//! used in neural networks.
//!
//! For each function in this library there are multiple variants, which
//! typically include:
//!
//!  - A version that operates on scalars
//!  - A version that reads values from an input slice and writes to the
//!    corresponding position in an equal-length output slice. These have a
//!    `vec_` prefix.
//!  - A version that reads values from a mutable input slice and writes
//!    the computed values back in-place. These have a `vec_` prefix and
//!    `_in_place` suffix.
//!
//! All variants use the same underlying implementation and should have the
//! same accuracy.
//!
//! See the source code for comments on accuracy.

mod erf;
mod exp;
mod softmax;
mod tanh;

#[cfg(test)]
mod ulp;

#[cfg(test)]
mod testing;

pub use erf::{erf, vec_erf, vec_erf_in_place};
pub use exp::{exp, sigmoid, vec_exp, vec_exp_in_place, vec_sigmoid, vec_sigmoid_in_place};
pub use softmax::{vec_softmax, vec_softmax_in_place};
pub use tanh::{tanh, vec_tanh, vec_tanh_in_place};
