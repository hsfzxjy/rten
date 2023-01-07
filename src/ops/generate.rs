use std::ops;

use crate::ops::{Input, InputList, IntoOpResult, OpError, Operator, Output, Scalar};
use crate::tensor::Tensor;

pub fn constant_of_shape<T: Copy>(value: T, shape: &Tensor<i32>) -> Tensor<T> {
    let shape: Vec<_> = shape.elements().map(|el| el as usize).collect();
    let len = shape.iter().product();
    Tensor::from_data(shape, vec![value; len])
}

#[derive(Debug)]
pub struct ConstantOfShape {
    pub value: Scalar,
}

impl Operator for ConstantOfShape {
    fn name(&self) -> &str {
        "ConstantOfShape"
    }

    fn run(&self, inputs: InputList) -> Result<Vec<Output>, OpError> {
        let shape = inputs.require_as::<i32>(0)?;
        match self.value {
            Scalar::Int(value) => constant_of_shape(value, shape).into_op_result(),
            Scalar::Float(value) => constant_of_shape(value, shape).into_op_result(),
        }
    }
}

pub fn range<T: Copy + Default + ops::Add<Output = T> + PartialOrd>(
    start: T,
    limit: T,
    delta: T,
) -> Result<Tensor<T>, OpError> {
    if delta == T::default() {
        return Err(OpError::InvalidValue("delta must be non-zero"));
    }

    // This is not very efficient as it grows the output gradually instead of
    // allocating once. This however made the initial implementation easier by
    // minimizing the traits that T needs to implement.
    let mut output = Vec::new();
    let mut val = start;
    while (delta > T::default() && val < limit) || (delta < T::default() && val > limit) {
        output.push(val);
        val = val + delta;
    }
    Ok(Tensor::from_vec(output))
}

#[derive(Debug)]
pub struct Range {}

impl Operator for Range {
    fn name(&self) -> &str {
        "Range"
    }

    fn run(&self, inputs: InputList) -> Result<Vec<Output>, OpError> {
        let start = inputs.require(0)?;
        let limit = inputs.require(1)?;
        let delta = inputs.require(2)?;

        match start {
            Input::FloatTensor(_) => {
                let start = start.try_into()?;
                let limit = limit.try_into()?;
                let delta = delta.try_into()?;
                range::<f32>(start, limit, delta).into_op_result()
            }
            Input::IntTensor(_) => {
                let start = start.try_into()?;
                let limit = limit.try_into()?;
                let delta = delta.try_into()?;
                range::<i32>(start, limit, delta).into_op_result()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::ops::{range, ConstantOfShape, Input, InputList, OpError, Operator, Scalar};
    use crate::tensor::from_vec;

    #[test]
    fn test_constant_of_shape() {
        let op = ConstantOfShape {
            value: Scalar::Int(42),
        };
        let shape = from_vec(vec![1, 5, 10]);

        let result = op
            .run(InputList::from(&[Input::IntTensor(&shape)]))
            .unwrap()
            .remove(0)
            .into_int()
            .unwrap();

        assert_eq!(result.shape(), &[1, 5, 10]);
        assert_eq!(
            result.elements().collect::<Vec<_>>(),
            vec![42; result.shape().iter().product()]
        );
    }

    #[test]
    fn test_range() {
        // Int range from zero
        let r = range(0, 5, 1).unwrap();
        assert_eq!(r.elements_vec(), vec![0, 1, 2, 3, 4]);

        // Float range from zero
        let r = range(0., 5., 1.).unwrap();
        assert_eq!(r.elements_vec(), vec![0., 1., 2., 3., 4.]);

        // Int range from negative value with step > 1
        let r = range(-5, 5, 2).unwrap();
        assert_eq!(r.elements_vec(), vec![-5, -3, -1, 1, 3]);

        // Float range from negative value with step > 1
        let r = range(-5., 5., 2.).unwrap();
        assert_eq!(r.elements_vec(), vec![-5., -3., -1., 1., 3.]);

        // Negative step
        let r = range(10, 4, -2).unwrap();
        assert_eq!(r.elements_vec(), vec![10, 8, 6]);
    }

    #[test]
    fn test_range_invalid_inputs() {
        let r = range(0, 5, 0);
        assert_eq!(
            r.err(),
            Some(OpError::InvalidValue("delta must be non-zero"))
        );
    }
}