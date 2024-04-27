use std::iter::zip;

use rten_tensor::prelude::*;
use rten_tensor::{Tensor, TensorView};

use crate::ops::binary_elementwise::broadcast_shapes;
use crate::ops::reduce::{cmp_nan_greater, cmp_nan_less};
use crate::ops::{Input, InputList, IntoOpResult, OpError, Operator, Output};
use crate::tensor_pool::TensorPool;

/// Apply an elementwise reduction to a sequence of tensors.
///
/// All inputs must be broadcastable to the same shape.
fn reduce_elementwise<T: Copy, R: Fn(&[T]) -> T>(
    pool: &TensorPool,
    inputs: &[TensorView<T>],
    reduce: &R,
) -> Result<Tensor<T>, OpError> {
    match inputs {
        [] => Err(OpError::InvalidValue("Expected at least one input")),
        [a, b] => {
            // Specialize common case of binary input.
            let Some(out_shape) = broadcast_shapes(a.shape(), b.shape()) else {
                return Err(OpError::IncompatibleInputShapes(
                    "Cannot broadcast inputs to same shape",
                ));
            };

            let mut result = Tensor::uninit_in(pool, &out_shape);
            for (out, (&a, &b)) in zip(
                result.iter_mut(),
                zip(a.broadcast_iter(&out_shape), b.broadcast_iter(&out_shape)),
            ) {
                out.write(reduce(&[a, b]));
            }

            // Safety: We initialized all output elements.
            Ok(unsafe { result.assume_init() })
        }
        _ => {
            let Some(out_shape) = inputs
                .iter()
                .try_fold(inputs[0].shape().to_vec(), |out_shape, input| {
                    broadcast_shapes(&out_shape, input.shape())
                })
            else {
                return Err(OpError::IncompatibleInputShapes(
                    "Cannot broadcast inputs to same shape",
                ));
            };

            let mut iters: Vec<_> = inputs
                .iter()
                .map(|view| view.broadcast_iter(&out_shape))
                .collect();
            let mut elts = Vec::with_capacity(inputs.len());
            let mut output = Tensor::uninit_in(pool, &out_shape);

            for out in output.iter_mut() {
                elts.extend(iters.iter_mut().map(|it| it.next().unwrap()));
                out.write(reduce(&elts));
                elts.clear();
            }

            // Safety: We initialized all output elements.
            Ok(unsafe { output.assume_init() })
        }
    }
}

/// Extract operator inputs as a vec of tensor views of the same type.
fn typed_views<'a, T>(inputs: &'a InputList) -> Result<Vec<TensorView<'a, T>>, OpError>
where
    Input<'a>: TryInto<TensorView<'a, T>, Error = OpError>,
{
    inputs.iter().try_fold(Vec::new(), |mut acc, input| {
        acc.push(input.try_into()?);
        Ok(acc)
    })
}

pub fn max<T: Copy + PartialOrd>(
    pool: &TensorPool,
    inputs: &[TensorView<T>],
) -> Result<Tensor<T>, OpError> {
    reduce_elementwise(pool, inputs, &|elts: &[T]| {
        elts.iter()
            .max_by(|a, b| cmp_nan_greater(*a, *b))
            .copied()
            .unwrap()
    })
}

macro_rules! run_typed_op {
    ($pool:expr, $inputs:ident, $op:ident) => {{
        let first = $inputs.require(0)?;
        match first {
            Input::FloatTensor(_) => {
                let inputs: Vec<TensorView<f32>> = typed_views(&$inputs)?;
                $op($pool, &inputs).into_op_result()
            }
            Input::IntTensor(_) => {
                let inputs: Vec<TensorView<i32>> = typed_views(&$inputs)?;
                $op($pool, &inputs).into_op_result()
            }
        }
    }};
}

#[derive(Debug)]
pub struct Max {}

impl Operator for Max {
    fn name(&self) -> &str {
        "Max"
    }

    fn run(&self, pool: &TensorPool, inputs: InputList) -> Result<Vec<Output>, OpError> {
        run_typed_op!(pool, inputs, max)
    }
}

pub fn mean(pool: &TensorPool, inputs: &[TensorView]) -> Result<Tensor, OpError> {
    let mut result = reduce_elementwise(pool, inputs, &|elts| elts.iter().sum())?;
    result.apply(|x| x / inputs.len() as f32);
    Ok(result)
}

#[derive(Debug)]
pub struct Mean {}

impl Operator for Mean {
    fn name(&self) -> &str {
        "Mean"
    }

    fn run(&self, pool: &TensorPool, inputs: InputList) -> Result<Vec<Output>, OpError> {
        let inputs: Vec<TensorView<f32>> = typed_views(&inputs)?;
        mean(pool, &inputs).into_op_result()
    }
}

pub fn min<T: Copy + PartialOrd>(
    pool: &TensorPool,
    inputs: &[TensorView<T>],
) -> Result<Tensor<T>, OpError> {
    reduce_elementwise(pool, inputs, &|elts: &[T]| {
        elts.iter()
            .min_by(|a, b| cmp_nan_less(*a, *b))
            .copied()
            .unwrap()
    })
}

#[derive(Debug)]
pub struct Min {}

impl Operator for Min {
    fn name(&self) -> &str {
        "Min"
    }

    fn run(&self, pool: &TensorPool, inputs: InputList) -> Result<Vec<Output>, OpError> {
        run_typed_op!(pool, inputs, min)
    }
}

pub fn sum<T: Copy + std::iter::Sum>(
    pool: &TensorPool,
    inputs: &[TensorView<T>],
) -> Result<Tensor<T>, OpError> {
    reduce_elementwise(pool, inputs, &|elts: &[T]| elts.iter().copied().sum())
}

#[derive(Debug)]
pub struct Sum {}

impl Operator for Sum {
    fn name(&self) -> &str {
        "Sum"
    }

    fn run(&self, pool: &TensorPool, inputs: InputList) -> Result<Vec<Output>, OpError> {
        run_typed_op!(pool, inputs, sum)
    }
}

#[cfg(test)]
mod tests {
    use rten_tensor::prelude::*;
    use rten_tensor::test_util::eq_with_nans;
    use rten_tensor::{tensor, Tensor, TensorView};

    use crate::ops::tests::new_pool;
    use crate::ops::{max, mean, min, sum, Input, InputList, Max, Min, OpError, Operator, Sum};

    fn run_operator<Op: Operator>(op: &Op, inputs: &[TensorView]) -> Tensor {
        let inputs: Vec<Input> = inputs.iter().cloned().map(|i| i.into()).collect();
        let pool = new_pool();
        let mut outputs = op.run(&pool, InputList::from(inputs.as_slice())).unwrap();
        outputs.remove(0).try_into().unwrap()
    }

    // nb. Most of the tests are written for the `max` operator only, as the
    // other elementwise reductions share most of the implementation.
    #[test]
    fn test_max() {
        struct Case {
            inputs: Vec<Tensor>,
            expected: Result<Tensor, OpError>,
        }

        let cases = [
            // Zero inputs
            Case {
                inputs: vec![],
                expected: Err(OpError::InvalidValue("Expected at least one input")),
            },
            // One input
            Case {
                inputs: vec![tensor!([1., 2., 3., 4.])],
                expected: Ok(tensor!([1., 2., 3., 4.])),
            },
            // Two inputs
            Case {
                inputs: vec![tensor!([1., 2., 3.]), tensor!([4., 1., 3.])],
                expected: Ok(tensor!([4., 2., 3.])),
            },
            // Two inputs with NaNs
            Case {
                inputs: vec![tensor!([1., 2., f32::NAN]), tensor!([4., 1., 3.])],
                expected: Ok(tensor!([4., 2., f32::NAN])),
            },
            // Three inputs
            Case {
                inputs: vec![tensor!([1., 2.]), tensor!([5., 1.]), tensor!([2., 3.])],
                expected: Ok(tensor!([5., 3.])),
            },
            // Two inputs, broadcasted
            Case {
                inputs: vec![tensor!([2., 4.]), tensor!((2, 2); [1., 2., 3., 4.])],
                expected: Ok(tensor!((2, 2); [
                    2., 4.,
                    3., 4.
                ])),
            },
            // Three inputs, broadcasted
            Case {
                inputs: vec![
                    tensor!([2., 4.]),
                    tensor!(3.),
                    tensor!((2, 2); [1., 2., 3., 4.]),
                ],
                expected: Ok(tensor!((2, 2); [
                    3., 4.,
                    3., 4.
                ])),
            },
            // Two inputs, incompatible broadcast
            Case {
                inputs: vec![tensor!([4., 5., 6.]), tensor!((2, 2); [1., 2., 3., 4.])],
                expected: Err(OpError::IncompatibleInputShapes(
                    "Cannot broadcast inputs to same shape",
                )),
            },
            // Three inputs, incompatible broadcast
            Case {
                inputs: vec![
                    tensor!([2., 4., 5.]),
                    tensor!(3.),
                    tensor!((2, 2); [1., 2., 3., 4.]),
                ],
                expected: Err(OpError::IncompatibleInputShapes(
                    "Cannot broadcast inputs to same shape",
                )),
            },
        ];

        let pool = new_pool();
        for case in cases {
            let views: Vec<_> = case.inputs.iter().map(|t| t.view()).collect();
            let result = max(&pool, &views);
            match (result, case.expected) {
                (Ok(result), Ok(expected)) => assert!(eq_with_nans(result.view(), expected.view())),
                (result, expected) => assert_eq!(result, expected),
            }
        }

        // Test the `Max` Operator impl
        let a = tensor!([1., 2., 7., 8.]);
        let b = tensor!([5., 6., 3., 4.]);
        let expected = tensor!([5., 6., 7., 8.]);
        let op_result = run_operator(&Max {}, &[a.view(), b.view()]);
        assert_eq!(op_result, expected);
    }

    #[test]
    fn test_mean() {
        let a = tensor!([1., 2., 3., 4.]);
        let b = tensor!([5., 6., 7., 8.]);
        let pool = new_pool();
        assert_eq!(
            mean(&pool, &[a.view(), b.view()]),
            Ok(tensor!([3., 4., 5., 6.]))
        );
    }

    #[test]
    fn test_min() {
        let pool = new_pool();

        let (a, b) = (tensor!([1., 2., 3.]), tensor!([4., 1., 3.]));
        let expected = tensor!([1., 1., 3.]);
        assert_eq!(min(&pool, &[a.view(), b.view()]), Ok(expected.clone()));

        let output = run_operator(&Min {}, &[a.view(), b.view()]);
        assert_eq!(output, expected);

        let (a, b) = (tensor!([1., 2., f32::NAN]), tensor!([4., 1., 3.]));
        let result = min(&pool, &[a.view(), b.view()]).unwrap();
        assert!(eq_with_nans(
            result.view(),
            tensor!([1., 1., f32::NAN]).view()
        ));
    }

    #[test]
    fn test_sum() {
        let pool = new_pool();
        let a = tensor!([1., 2., 3., 4.]);
        let b = tensor!([5., 6., 7., 8.]);
        let expected = tensor!([6., 8., 10., 12.]);

        assert_eq!(sum(&pool, &[a.view(), b.view()]), Ok(expected.clone()));

        let output = run_operator(&Sum {}, &[a.view(), b.view()]);
        assert_eq!(output, expected);
    }
}
