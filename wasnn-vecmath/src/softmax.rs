use crate::dispatch_simd;
use crate::exp::simd_exp;
use crate::simd_vec::SimdFloat;
use crate::{vec_fold, vec_unary_op, MutPtrLen, PtrLen};

/// Apply the softmax operation over elements in `xs` and write results to
/// `out`.
///
/// The implementation uses a three-pass approach for numerical stability.
/// See https://ogunlao.github.io/2020/04/26/you_dont_really_know_softmax.html
/// and https://arxiv.org/abs/2001.04438.
unsafe fn simd_softmax<S: SimdFloat>(xs: PtrLen<f32>, out: MutPtrLen<f32>) {
    let max_val = vec_fold(
        xs,
        S::splat(f32::MIN),
        |max, x| max.max(x),
        f32::MIN, /* pad */
    );
    let max_val = max_val.fold_splat(f32::MIN, |max: f32, x: f32| max.max(x));

    // *x = (*x - max_val).exp()
    let mut exp_sum = S::zero();
    let exp_pad = f32::NEG_INFINITY; // exp(-inf) = 0, so won't affect `exp_sum`
    vec_unary_op(
        xs,
        out,
        |x: S| {
            let y = simd_exp(x.sub(max_val));
            exp_sum = exp_sum.add(y);
            y
        },
        exp_pad,
    );

    // *x /= exp_sum
    let exp_sum = exp_sum.fold_splat(0., |sum, x| sum + x);
    vec_unary_op(out.into(), out, |x: S| x.div(exp_sum), 1. /* pad */);
}

/// Computes the [softmax][softmax] function over a slice of floats.
///
/// [softmax]: https://en.wikipedia.org/wiki/Softmax_function
pub fn vec_softmax(xs: &[f32], out: &mut [f32]) {
    dispatch_simd!(simd_softmax, xs.into(), out.into());
}

/// Computes the [softmax][softmax] function over a slice of floats.
///
/// [softmax]: https://en.wikipedia.org/wiki/Softmax_function
pub fn vec_softmax_in_place(xs: &mut [f32]) {
    dispatch_simd!(simd_softmax, xs.into(), xs.into());
}

#[cfg(test)]
mod tests {
    use super::vec_softmax;

    use crate::testing::{check_f32s_are_equal_ulps, triples};

    #[test]
    fn test_vec_softmax() {
        let input = vec![0.1634, 0.8647, 0.6401, 0.8265, 0.0560, 0.2304];
        let expected = &([
            0.11715934, 0.23623686, 0.18871443, 0.2273828, 0.10522857, 0.12527795,
        ]);
        let mut actual = vec![0.; input.len()];

        vec_softmax(&input, &mut actual);

        check_f32s_are_equal_ulps(triples(&input, &actual, expected), 0. /* max ULPs */);
    }
}
