use crate::ops::layout::squeeze_in_place;
use crate::ops::{resolve_axes, InputList, IntoOpResult, OpError, Operator, Output};
use crate::tensor::{IndexIterator, SliceRange, Tensor};

/// Trait for reducing a subset of elements from a tensor to a single value.
///
/// This is a trait rather than a closure to support being invoked with
/// dynamically chosen iterator types.
trait Reducer<T: Copy> {
    fn reduce<I: ExactSizeIterator<Item = T>>(&self, iter: I) -> T;
}

fn reduce<T: Copy + Default, R: Reducer<T>>(
    input: &Tensor<T>,
    axes: Option<&[i32]>,
    keep_dims: bool,
    reducer: R,
) -> Result<Tensor<T>, OpError> {
    let mut resolved_axes = match axes {
        Some(axes) if !axes.is_empty() => resolve_axes(input.ndim(), axes)?,
        _ => (0..input.ndim()).collect(),
    };
    resolved_axes.sort();

    if input.ndim() == 0 {
        return Ok(Tensor::from_scalar(reducer.reduce(input.elements())));
    }

    // Number of innermost dims being iterated over, or None if we're not
    // iterating over innermost dims.
    let reduced_inner_dims: Option<usize> = resolved_axes
        .iter()
        .enumerate()
        .all(|(i, &axis)| axis == input.ndim() - 1 - i)
        .then_some(resolved_axes.len());

    let reduced_shape: Vec<usize> = input
        .shape()
        .iter()
        .enumerate()
        .map(|(dim, &size)| {
            if resolved_axes.contains(&dim) {
                1
            } else {
                size
            }
        })
        .collect();
    let mut reduced_data = Vec::with_capacity(reduced_shape.iter().product());

    match (reduced_inner_dims, input.is_contiguous()) {
        (Some(ndims), true) => {
            // Fast path for reducing over contiguous chunks of the input.
            let slice_len = input.stride(input.ndim() - 1 - ndims);
            reduced_data.extend((0..input.len()).step_by(slice_len).map(|offset| {
                let slice = &input.data()[offset..offset + slice_len];
                reducer.reduce(slice.iter().copied())
            }));
        }
        _ => {
            let outer_range: Vec<_> = (0..input.ndim())
                .map(|dim| {
                    if resolved_axes.contains(&dim) {
                        0..1
                    } else {
                        0..input.shape()[dim]
                    }
                })
                .collect();

            let mut outer_iter = IndexIterator::from_ranges(&outer_range);
            let mut inner_range = Vec::with_capacity(input.ndim());

            while let Some(index) = outer_iter.next() {
                inner_range.clear();
                inner_range.extend(index.iter().enumerate().map(|(dim, &idx)| {
                    if resolved_axes.contains(&dim) {
                        SliceRange::new(0, input.shape()[dim] as isize, 1)
                    } else {
                        SliceRange::new(idx as isize, idx as isize + 1, 1)
                    }
                }));
                let reduced = reducer.reduce(input.slice_elements(&inner_range));
                reduced_data.push(reduced);
            }
        }
    }

    let mut reduced = Tensor::<T>::from_data(reduced_shape, reduced_data);

    if !keep_dims {
        squeeze_in_place(&mut reduced, Some(&resolved_axes));
    }

    Ok(reduced)
}

pub fn reduce_mean(
    input: &Tensor,
    axes: Option<&[i32]>,
    keep_dims: bool,
) -> Result<Tensor, OpError> {
    struct MeanReducer {}
    impl Reducer<f32> for MeanReducer {
        fn reduce<I: ExactSizeIterator<Item = f32>>(&self, iter: I) -> f32 {
            let len = iter.len() as f32;
            iter.sum::<f32>() / len
        }
    }

    reduce(input, axes, keep_dims, MeanReducer {})
}

#[derive(Debug)]
pub struct ReduceMean {
    pub axes: Option<Vec<i32>>,
    pub keep_dims: bool,
}

impl Operator for ReduceMean {
    fn name(&self) -> &str {
        "ReduceMean"
    }

    fn run(&self, inputs: InputList) -> Result<Vec<Output>, OpError> {
        let input = inputs.require_as(0)?;
        reduce_mean(
            input,
            self.axes.as_ref().map(|axis| &axis[..]),
            self.keep_dims,
        )
        .into_op_result()
    }
}

#[cfg(test)]
mod tests {
    use crate::ops::{reduce_mean, OpError};
    use crate::tensor::{from_data, from_scalar, from_vec};
    use crate::test_util::expect_equal;

    #[test]
    fn test_reduce_mean() -> Result<(), String> {
        let input = from_data(vec![3, 3], vec![1., 2., 3., 4., 5., 6., 7., 8., 9.]);

        // Test with `keep_dims` off
        let result = reduce_mean(&input, Some(&[-1]), false /* keep_dims */).unwrap();
        let expected = from_vec(vec![2., 5., 8.]);
        expect_equal(&result, &expected)?;

        // Test with `keep_dims` on
        let result = reduce_mean(&input, Some(&[-1]), true /* keep_dims */).unwrap();
        let expected = from_data(vec![3, 1], vec![2., 5., 8.]);
        expect_equal(&result, &expected)?;

        // Reduce first dim
        let result = reduce_mean(&input, Some(&[0]), false /* keep_dims */).unwrap();
        let expected = from_vec(vec![4., 5., 6.]);
        expect_equal(&result, &expected)?;

        // Reduce all axes
        let result = reduce_mean(&input, None, false /* keep_dims */).unwrap();
        let expected = from_scalar(5.);
        expect_equal(&result, &expected)?;

        // Reduce all axes (specified via empty array)
        let result = reduce_mean(&input, Some(&[]), false /* keep_dims */).unwrap();
        let expected = from_scalar(5.);
        expect_equal(&result, &expected)?;

        // Test case from ONNX spec
        let input = from_data(
            vec![3, 2, 2],
            vec![5., 1., 20., 2., 30., 1., 40., 2., 55., 1., 60., 2.],
        );
        let expected = from_data(vec![3, 2], vec![12.5, 1.5, 35., 1.5, 57.5, 1.5]);
        let result = reduce_mean(&input, Some(&[1]), false /* keep_dims */).unwrap();
        expect_equal(&result, &expected)?;

        // Reduce a scalar value
        let result = reduce_mean(&from_scalar(5.0), Some(&[]), false /* keep_dims */).unwrap();
        assert_eq!(result.item(), Some(5.0));

        Ok(())
    }

    #[test]
    fn test_reduce_mean_invalid_inputs() {
        let input = from_data(vec![3, 3], vec![1., 2., 3., 4., 5., 6., 7., 8., 9.]);

        let result = reduce_mean(&input, Some(&[3]), false /* keep_dims */);
        assert_eq!(result.err(), Some(OpError::InvalidValue("axis is invalid")));

        let result = reduce_mean(&input, Some(&[-3]), false /* keep_dims */);
        assert_eq!(result.err(), Some(OpError::InvalidValue("axis is invalid")));
    }
}
