//! the small pieces of numerical fitting the matcher needs: a straight
//! line through points by least squares, two unknowns by least squares,
//! and finding the lowest point of a one-dimensional function.

/// the line y = a·x + b closest (least squares) to the points. a flat
/// cloud (all x the same) gives a slope of 1 through their mean offset.
pub fn fit_line(points: &[(f64, f64)]) -> (f64, f64) {
    let n = points.len() as f64;
    if points.is_empty() {
        return (1.0, 0.0);
    }
    let mean_x = points.iter().map(|p| p.0).sum::<f64>() / n;
    let mean_y = points.iter().map(|p| p.1).sum::<f64>() / n;
    let spread: f64 = points.iter().map(|p| (p.0 - mean_x).powi(2)).sum();
    if spread < 1e-12 {
        return (1.0, mean_y - mean_x);
    }
    let covariance: f64 = points.iter().map(|p| (p.0 - mean_x) * (p.1 - mean_y)).sum();
    let slope = covariance / spread;
    (slope, mean_y - slope * mean_x)
}

/// solves target ≈ a·u + b·v for (a, b) by least squares, given rows of
/// (u, v, target). None when the rows don't pin both down.
pub fn least_squares_2(rows: &[(f64, f64, f64)]) -> Option<(f64, f64)> {
    let (mut uu, mut uv, mut vv, mut ut, mut vt) = (0.0, 0.0, 0.0, 0.0, 0.0);
    for (u, v, t) in rows {
        uu += u * u;
        uv += u * v;
        vv += v * v;
        ut += u * t;
        vt += v * t;
    }
    let determinant = uu * vv - uv * uv;
    if determinant.abs() < 1e-12 * (uu * vv).max(1e-12) {
        return None;
    }
    Some((
        (ut * vv - vt * uv) / determinant,
        (vt * uu - ut * uv) / determinant,
    ))
}

/// the x between `low` and `high` where `f` is lowest, by golden section
/// search (f should have one dip there). `steps` narrows the range by
/// 0.618 each.
pub fn golden_minimum(mut f: impl FnMut(f64) -> f64, low: f64, high: f64, steps: usize) -> f64 {
    let ratio = (5f64.sqrt() - 1.0) / 2.0;
    let (mut a, mut b) = (low.min(high), low.max(high));
    let mut c = b - ratio * (b - a);
    let mut d = a + ratio * (b - a);
    let mut fc = f(c);
    let mut fd = f(d);
    for _ in 0..steps {
        if fc < fd {
            b = d;
            d = c;
            fd = fc;
            c = b - ratio * (b - a);
            fc = f(c);
        } else {
            a = c;
            c = d;
            fc = fd;
            d = a + ratio * (b - a);
            fd = f(d);
        }
    }
    if fc < fd {
        c
    } else {
        d
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_through_exact_points() {
        let (a, b) = fit_line(&[(0.0, 1.0), (1.0, 3.0), (2.0, 5.0)]);
        assert!((a - 2.0).abs() < 1e-12 && (b - 1.0).abs() < 1e-12);
        assert_eq!(fit_line(&[(2.0, 5.0), (2.0, 7.0)]), (1.0, 4.0));
    }

    #[test]
    fn two_unknowns_recovered() {
        // target = 1.1 u + 0.03 v, with v = 2 (twice the offset)
        let rows: Vec<(f64, f64, f64)> = [0.3, 0.5, 0.08, 0.6, 0.1]
            .iter()
            .map(|u| (*u, 2.0, 1.1 * u + 0.06))
            .collect();
        let (a, b) = least_squares_2(&rows).unwrap();
        assert!((a - 1.1).abs() < 1e-9 && (b - 0.03).abs() < 1e-9);
        assert!(least_squares_2(&[(1.0, 2.0, 3.0), (2.0, 4.0, 6.0)]).is_none());
    }

    #[test]
    fn the_dip_is_found() {
        let x = golden_minimum(|x| (x - 0.3).powi(2), -1.0, 1.0, 40);
        assert!((x - 0.3).abs() < 1e-6);
    }
}
