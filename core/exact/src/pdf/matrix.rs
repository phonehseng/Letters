//! 2d affine matrices in pdf's row-vector convention: `[a b c d e f]`
//! maps (x, y) to (a x + c y + e, b x + d y + f).

pub type Matrix = [f64; 6];

pub const IDENTITY: Matrix = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

/// `m` then `n` (pdf writes this `m × n`).
pub fn multiply(m: &Matrix, n: &Matrix) -> Matrix {
    let [a, b, c, d, e, f] = *m;
    let [p, q, r, s, t, u] = *n;
    [
        a * p + b * r,
        a * q + b * s,
        c * p + d * r,
        c * q + d * s,
        e * p + f * r + t,
        e * q + f * s + u,
    ]
}

pub fn apply(m: &Matrix, x: f64, y: f64) -> (f64, f64) {
    (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5])
}

pub fn translation(x: f64, y: f64) -> Matrix {
    [1.0, 0.0, 0.0, 1.0, x, y]
}

/// the uniform scale of a matrix, used for line widths.
pub fn scale_of(m: &Matrix) -> f64 {
    (m[0] * m[3] - m[1] * m[2]).abs().sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multiply_then_apply() {
        // the prototype's case (test_prototype.py)
        let translate = translation(10.0, 20.0);
        let scale = [2.0, 0.0, 0.0, 3.0, 0.0, 0.0];
        assert_eq!(apply(&multiply(&scale, &translate), 1.0, 1.0), (12.0, 23.0));
    }

    #[test]
    fn scale_of_a_rotation_is_one() {
        let turn = [0.0, 1.0, -1.0, 0.0, 5.0, 5.0];
        assert!((scale_of(&turn) - 1.0).abs() < 1e-12);
    }
}
