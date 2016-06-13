use std;
use std::collections::BinaryHeap;
use ::heap_element::HeapElement;
use ::util;

#[derive(Debug)]
pub struct KdTree<T, U: AsRef<[f64]>> {
    // node
    left: Option<Box<KdTree<T, U>>>,
    right: Option<Box<KdTree<T, U>>>,
    // common
    dimensions: usize,
    capacity: usize,
    size: usize,
    min_bounds: Box<[f64]>,
    max_bounds: Box<[f64]>,
    // stem
    split_value: Option<f64>,
    split_dimension: Option<usize>,
    // leaf
    points: Option<Vec<U>>,
    bucket: Option<Vec<T>>,
}

#[derive(Debug, PartialEq)]
pub enum ErrorKind {
    WrongDimension,
    NonFiniteCoordinate,
    ZeroCapacity,
}

impl<T, U: AsRef<[f64]>> KdTree<T, U> {
    pub fn new(dims: usize) -> Self {
        KdTree::new_with_capacity(dims, 2usize.pow(4))
    }

    pub fn new_with_capacity(dimensions: usize, capacity: usize) -> Self {
        let min_bounds = vec![std::f64::INFINITY; dimensions];
        let max_bounds = vec![std::f64::NEG_INFINITY; dimensions];
        KdTree {
            left: None,
            right: None,
            dimensions: dimensions,
            capacity: capacity,
            size: 0,
            min_bounds: min_bounds.into_boxed_slice(),
            max_bounds: max_bounds.into_boxed_slice(),
            split_value: None,
            split_dimension: None,
            points: Some(vec![]),
            bucket: Some(vec![]),
        }
    }

    pub fn size(&self) -> usize {
        self.size
    }

    pub fn nearest<F>(&self,
                      point: &[f64],
                      num: usize,
                      distance: &F)
                      -> Result<Vec<(f64, &T)>, ErrorKind>
        where F: Fn(&[f64], &[f64]) -> f64
    {
        if let Err(err) = self.check_point(point.as_ref()) {
            return Err(err);
        }
        let num = std::cmp::min(num, self.size);
        if num <= 0 {
            return Ok(vec![]);
        }
        let mut pending = BinaryHeap::new();
        let mut evaluated = BinaryHeap::<HeapElement<&T>>::new();
        pending.push(HeapElement {
            distance: 0f64,
            element: self,
        });
        while !pending.is_empty() &&
              (evaluated.len() < num ||
               (-pending.peek().unwrap().distance < evaluated.peek().unwrap().distance)) {
            self.nearest_step(point, num, distance, &mut pending, &mut evaluated);
        }
        Ok(evaluated.into_sorted_vec().into_iter().take(num).map(Into::into).collect())
    }

    fn nearest_step<'b, F>(&self,
                           point: &[f64],
                           num: usize,
                           distance: &F,
                           pending: &mut BinaryHeap<HeapElement<&'b Self>>,
                           evaluated: &mut BinaryHeap<HeapElement<&'b T>>)
        where F: Fn(&[f64], &[f64]) -> f64
    {
        let mut curr = &*pending.pop().unwrap().element;
        let evaluated_dist = if evaluated.len() < num {
            std::f64::INFINITY
        } else {
            evaluated.peek().unwrap().distance
        };

        while !curr.is_leaf() {
            let candidate;
            if curr.belongs_in_left(point.as_ref()) {
                candidate = curr.right.as_ref().unwrap();
                curr = curr.left.as_ref().unwrap();
            } else {
                candidate = curr.left.as_ref().unwrap();
                curr = curr.right.as_ref().unwrap();
            }
            let candidate_to_space =
                util::distance_to_space(point, &*curr.min_bounds, &*curr.max_bounds, distance);
            if candidate_to_space <= evaluated_dist {
                pending.push(HeapElement {
                    distance: candidate_to_space * -1f64,
                    element: &**candidate,
                });
            }
        }

        let points = curr.points.as_ref().unwrap().iter();
        let bucket = curr.bucket.as_ref().unwrap().iter();
        let iter = points.zip(bucket).map(|(p, d)| {
            HeapElement {
                distance: distance(p.as_ref(), point),
                element: d,
            }
        });
        for element in iter {
            if evaluated.len() < num {
                evaluated.push(element);
            } else if element < *evaluated.peek().unwrap() {
                evaluated.pop();
                evaluated.push(element);
            }
        }
    }

    pub fn add(&mut self, point: U, data: T) -> Result<(), ErrorKind> {
        if self.capacity == 0 {
            return Err(ErrorKind::ZeroCapacity);
        }
        if let Err(err) = self.check_point(point.as_ref()) {
            return Err(err);
        }
        self.add_unchecked(point, data)
    }

    fn add_unchecked(&mut self, point: U, data: T) -> Result<(), ErrorKind> {
        if self.is_leaf() {
            self.add_to_bucket(point, data);
            return Ok(());
        }
        self.extend(point.as_ref());
        self.size += 1;
        let next = if self.belongs_in_left(point.as_ref()) {
            self.left.as_mut()
        } else {
            self.right.as_mut()
        };
        next.unwrap().add_unchecked(point, data)
    }


    fn add_to_bucket(&mut self, point: U, data: T) {
        self.extend(point.as_ref());
        let mut points = self.points.take().unwrap();
        let mut bucket = self.bucket.take().unwrap();
        points.push(point);
        bucket.push(data);
        self.size += 1;
        if self.size > self.capacity {
            self.split(points, bucket);
        } else {
            self.points = Some(points);
            self.bucket = Some(bucket);
        }
    }

    fn split(&mut self, mut points: Vec<U>, mut bucket: Vec<T>) {
        let mut max = 0f64;
        for dim in 0..self.dimensions {
            let diff = self.max_bounds[dim] - self.min_bounds[dim];
            if !diff.is_nan() && diff > max {
                max = diff;
                self.split_dimension = Some(dim);
            }
        }
        match self.split_dimension {
            None => {
                self.points = Some(points);
                self.bucket = Some(bucket);
                return;
            }
            Some(dim) => {
                let min = self.min_bounds[dim];
                let max = self.max_bounds[dim];
                self.split_value = Some(min + (max - min) / 2f64);
            }
        };
        let mut left = Box::new(KdTree::new_with_capacity(self.dimensions, self.capacity));
        let mut right = Box::new(KdTree::new_with_capacity(self.dimensions, self.capacity));
        while !points.is_empty() {
            let point = points.swap_remove(0);
            let data = bucket.swap_remove(0);
            if self.belongs_in_left(point.as_ref()) {
                left.add_to_bucket(point, data);
            } else {
                right.add_to_bucket(point, data);
            }
        }
        self.left = Some(left);
        self.right = Some(right);
    }

    fn belongs_in_left(&self, point: &[f64]) -> bool {
        point[self.split_dimension.unwrap()] < self.split_value.unwrap()
    }

    fn extend(&mut self, point: &[f64]) {
        let min = self.min_bounds.iter_mut();
        let max = self.max_bounds.iter_mut();
        for ((l, h), v) in min.zip(max).zip(point.iter()) {
            if v < l {
                *l = *v
            }
            if v > h {
                *h = *v
            }
        }
    }

    fn is_leaf(&self) -> bool {
        self.bucket.is_some() && self.points.is_some() && self.split_value.is_none() &&
        self.split_dimension.is_none() && self.left.is_none() && self.right.is_none()
    }

    fn check_point(&self, point: &[f64]) -> Result<(), ErrorKind> {
        if self.dimensions != point.len() {
            return Err(ErrorKind::WrongDimension);
        }
        for n in point {
            if !n.is_finite() {
                return Err(ErrorKind::NonFiniteCoordinate);
            }
        }
        Ok(())
    }
}
