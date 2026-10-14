//! Minimal 2D vector on the ground plane (x, z). The sim never needs height.

use serde::{Deserialize, Serialize};
use std::ops::{Add, AddAssign, Mul, Neg, Sub};

#[derive(Clone, Copy, Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(from = "[f32; 2]", into = "[f32; 2]")]
pub struct V2 {
    pub x: f32,
    pub z: f32,
}

impl V2 {
    pub const ZERO: V2 = V2 { x: 0.0, z: 0.0 };

    pub const fn new(x: f32, z: f32) -> Self {
        V2 { x, z }
    }

    pub fn len_sq(self) -> f32 {
        self.x * self.x + self.z * self.z
    }

    pub fn len(self) -> f32 {
        self.len_sq().sqrt()
    }

    pub fn dist(self, other: V2) -> f32 {
        (self - other).len()
    }

    pub fn normalize_or_zero(self) -> V2 {
        let l = self.len();
        if l > 1e-6 { self * (1.0 / l) } else { V2::ZERO }
    }

    /// Rotate counter-clockwise (in the x/z plane) by `angle` radians.
    pub fn rotate(self, angle: f32) -> V2 {
        let (s, c) = angle.sin_cos();
        V2::new(self.x * c - self.z * s, self.x * s + self.z * c)
    }

    /// Heading angle such that a rotation about +Y by this angle maps +Z onto `self`.
    pub fn heading(self) -> f32 {
        self.x.atan2(self.z)
    }

    pub fn from_heading(angle: f32) -> V2 {
        V2::new(angle.sin(), angle.cos())
    }

    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.z.is_finite()
    }
}

impl From<[f32; 2]> for V2 {
    fn from(a: [f32; 2]) -> Self {
        V2::new(a[0], a[1])
    }
}

impl From<V2> for [f32; 2] {
    fn from(v: V2) -> Self {
        [v.x, v.z]
    }
}

impl Add for V2 {
    type Output = V2;
    fn add(self, o: V2) -> V2 {
        V2::new(self.x + o.x, self.z + o.z)
    }
}

impl AddAssign for V2 {
    fn add_assign(&mut self, o: V2) {
        self.x += o.x;
        self.z += o.z;
    }
}

impl Sub for V2 {
    type Output = V2;
    fn sub(self, o: V2) -> V2 {
        V2::new(self.x - o.x, self.z - o.z)
    }
}

impl Mul<f32> for V2 {
    type Output = V2;
    fn mul(self, s: f32) -> V2 {
        V2::new(self.x * s, self.z * s)
    }
}

impl Neg for V2 {
    type Output = V2;
    fn neg(self) -> V2 {
        V2::new(-self.x, -self.z)
    }
}
