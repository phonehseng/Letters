//! drawingml shape guides: named values computed by small formulas
//! ("*/ w adj 100000") from the shape's size, its handle values and the
//! guides before them (ecma-376 part 1, 20.1.9.11 lists the operators).

use std::collections::HashMap;

/// a full circle in drawingml's angle unit (60000ths of a degree)
const FULL_CIRCLE: f64 = 21_600_000.0;

/// the guides of one shape, by name.
pub struct Guides {
    values: HashMap<String, f64>,
}

impl Guides {
    /// the built-in guides every shape has, for a shape `width` by `height`.
    pub fn new(width: f64, height: f64) -> Guides {
        let mut values = HashMap::new();
        let short = width.min(height);
        let long = width.max(height);
        let mut set = |name: &str, value: f64| {
            values.insert(name.to_string(), value);
        };
        set("w", width);
        set("h", height);
        set("l", 0.0);
        set("t", 0.0);
        set("r", width);
        set("b", height);
        set("hc", width / 2.0);
        set("vc", height / 2.0);
        set("ss", short);
        set("ls", long);
        for divisor in [2, 3, 4, 5, 6, 8, 10, 12, 16, 32] {
            set(&format!("wd{divisor}"), width / f64::from(divisor));
            set(&format!("hd{divisor}"), height / f64::from(divisor));
            set(&format!("ssd{divisor}"), short / f64::from(divisor));
        }
        let angles = [
            ("cd2", 10_800_000.0),
            ("cd4", 5_400_000.0),
            ("cd8", 2_700_000.0),
            ("3cd4", 16_200_000.0),
            ("3cd8", 8_100_000.0),
            ("5cd8", 13_500_000.0),
            ("7cd8", 18_900_000.0),
        ];
        for (name, value) in angles {
            set(name, value);
        }
        Guides { values }
    }

    pub fn set(&mut self, name: &str, value: f64) {
        self.values.insert(name.to_string(), value);
    }

    /// a guide's value, or a number written in place of a guide name.
    pub fn value(&self, token: &str) -> f64 {
        if let Some(value) = self.values.get(token) {
            return *value;
        }
        token.parse().unwrap_or(0.0)
    }

    /// evaluates one formula and stores it under `name`.
    pub fn define(&mut self, name: &str, formula: &str) {
        let value = self.evaluate(formula);
        self.set(name, value);
    }

    pub fn evaluate(&self, formula: &str) -> f64 {
        let mut tokens = formula.split_whitespace();
        let operator = tokens.next().unwrap_or("val");
        let arguments: Vec<f64> = tokens.map(|token| self.value(token)).collect();
        let argument = |index: usize| arguments.get(index).copied().unwrap_or(0.0);
        let (x, y, z) = (argument(0), argument(1), argument(2));
        match operator {
            "*/" => safe_divide(x * y, z),
            "+-" => x + y - z,
            "+/" => safe_divide(x + y, z),
            "?:" => {
                if x > 0.0 {
                    y
                } else {
                    z
                }
            }
            "abs" => x.abs(),
            "at2" => to_angle(y.atan2(x)),
            "cat2" => x * z.atan2(y).cos(),
            "sat2" => x * z.atan2(y).sin(),
            "cos" => x * to_radians(y).cos(),
            "sin" => x * to_radians(y).sin(),
            "tan" => x * to_radians(y).tan(),
            "max" => x.max(y),
            "min" => x.min(y),
            "mod" => (x * x + y * y + z * z).sqrt(),
            "pin" => y.clamp(x.min(z), z.max(x)),
            "sqrt" => x.max(0.0).sqrt(),
            _ => x,
        }
    }
}

/// a division by zero in a degenerate shape (zero width) gives 0, not
/// infinity, so a flat shape draws as a line instead of nothing
fn safe_divide(numerator: f64, denominator: f64) -> f64 {
    if denominator == 0.0 {
        0.0
    } else {
        numerator / denominator
    }
}

pub fn to_radians(angle: f64) -> f64 {
    angle / FULL_CIRCLE * std::f64::consts::TAU
}

fn to_angle(radians: f64) -> f64 {
    let angle = radians / std::f64::consts::TAU * FULL_CIRCLE;
    if angle < 0.0 {
        angle + FULL_CIRCLE
    } else {
        angle
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_guides_follow_the_shape_size() {
        let guides = Guides::new(200.0, 100.0);
        assert_eq!(guides.value("ss"), 100.0);
        assert_eq!(guides.value("wd4"), 50.0);
        assert_eq!(guides.value("cd4"), 5_400_000.0);
    }

    #[test]
    fn formulas_compute_as_the_specification_says() {
        let mut guides = Guides::new(200.0, 100.0);
        guides.set("adj", 16667.0);
        guides.define("a", "pin 0 adj 50000");
        guides.define("x1", "*/ ss a 100000");
        assert!((guides.value("x1") - 16.667).abs() < 1e-9);
        assert_eq!(guides.evaluate("+- r 0 x1"), 200.0 - guides.value("x1"));
        assert!((guides.evaluate("cos 100 5400000")).abs() < 1e-9);
        assert!((guides.evaluate("at2 1 1") - 2_700_000.0).abs() < 1e-6);
        assert_eq!(guides.evaluate("pin 0 70000 50000"), 50000.0);
        assert_eq!(guides.evaluate("*/ 1 2 0"), 0.0);
    }
}
