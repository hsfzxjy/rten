//! Architecture-specific functionality.

/// Dummy arch which implements SIMD vector types for Rust scalars (i32, f32 etc.)
mod scalar;

#[cfg(target_arch = "x86_64")]
mod x86_64;

#[cfg(target_arch = "aarch64")]
mod aarch64;

#[cfg(target_arch = "wasm32")]
pub mod wasm;

use crate::{SimdFloat, SimdInt};

/// Fallback implementation for [`SimdFloat::gather_mask`], for CPUs where
/// a native gather implementation is unavailable or unusable.
///
/// The caller must set `LEN` to `S::LEN`.
///
/// # Safety
///
/// See notes in [`SimdFloat::gather_mask`]. In particular, `src` must point
/// to a non-empty buffer, so that `src[0]` is valid.
#[inline]
unsafe fn simd_gather_mask<S: SimdFloat, const LEN: usize>(
    src: *const f32,
    offsets: S::Int,
    mask: S::Mask,
) -> S {
    // Set offset to zero where masked out. `src` is required to point to
    // a non-empty buffer, so index zero can be loaded as a dummy. This avoids
    // an unpredictable branch.
    let offsets = S::Int::splat(0).blend(offsets, mask);
    let mut offset_array = [0; LEN];
    offsets.store(offset_array.as_mut_ptr());

    let values: [f32; LEN] = std::array::from_fn(|i| *src.add(offset_array[i] as usize));
    S::splat(0.).blend(S::load(values.as_ptr()), mask)
}
