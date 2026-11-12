//! maths functions: sums and products, the rounding family, integer
//! parts and remainders, powers and logarithms, trigonometry, factorials
//! and combinations, random numbers.

use crate::formula::Expr;
use crate::model::{ErrorValue, Value};

use super::super::eval::Evaluator;
use super::super::value::{round_significant, CalcValue, SIGNIFICANT_DIGITS};
use super::args::*;

pub const NAMES: &[&str] = &[
    "SUM",
    "PRODUCT",
    "SUMSQ",
    "SUMPRODUCT",
    "ROUND",
    "ROUNDUP",
    "ROUNDDOWN",
    "MROUND",
    "INT",
    "TRUNC",
    "MOD",
    "ABS",
    "SIGN",
    "SQRT",
    "POWER",
    "EXP",
    "LN",
    "LOG",
    "LOG10",
    "PI",
    "SIN",
    "COS",
    "TAN",
    "ASIN",
    "ACOS",
    "ATAN",
    "ATAN2",
    "SINH",
    "COSH",
    "TANH",
    "DEGREES",
    "RADIANS",
    "CEILING",
    "CEILING.MATH",
    "FLOOR",
    "FLOOR.MATH",
    "EVEN",
    "ODD",
    "FACT",
    "COMBIN",
    "PERMUT",
    "GCD",
    "LCM",
    "QUOTIENT",
    "RAND",
    "RANDBETWEEN",
    "SQRTPI",
];

pub fn call(ev: &Evaluator, name: &str, args: &[Expr]) -> Option<CalcValue> {
    let one = |f: fn(f64) -> Outcome<f64>| {
        lift_number(ev, args, &|n| {
            if n.len() == 1 {
                f(n[0])
            } else {
                Err(ErrorValue::Value)
            }
        })
    };
    Some(match name {
        "SUM" => done_number(numbers(ev, args).map(|n| n.iter().sum())),
        "PRODUCT" => done_number(numbers(ev, args).map(|n| {
            if n.is_empty() {
                0.0
            } else {
                n.iter().product()
            }
        })),
        "SUMSQ" => done_number(numbers(ev, args).map(|n| n.iter().map(|x| x * x).sum())),
        "SUMPRODUCT" => done_number(sum_product(ev, args)),
        "ROUND" | "ROUNDUP" | "ROUNDDOWN" => rounding(ev, name, args),
        "TRUNC" => lift_number(ev, args, &|n| {
            Ok(round_digits(
                n[0],
                n.get(1).copied().unwrap_or(0.0),
                Rounding::Down,
            ))
        }),
        "MROUND" => lift_number(ev, args, &|n| two(n).and_then(|(a, b)| mround(a, b))),
        "INT" => one(|n| Ok(n.floor())),
        "MOD" => lift_number(ev, args, &|n| two(n).and_then(|(a, b)| modulo(a, b))),
        "ABS" => one(|n| Ok(n.abs())),
        "SIGN" => one(|n| {
            Ok(if n > 0.0 {
                1.0
            } else if n < 0.0 {
                -1.0
            } else {
                0.0
            })
        }),
        "SQRT" => one(|n| {
            if n < 0.0 {
                Err(ErrorValue::Num)
            } else {
                Ok(n.sqrt())
            }
        }),
        "SQRTPI" => one(|n| {
            if n < 0.0 {
                Err(ErrorValue::Num)
            } else {
                Ok((n * std::f64::consts::PI).sqrt())
            }
        }),
        "POWER" => lift_number(ev, args, &|n| {
            two(n).and_then(|(a, b)| as_number(super::super::operators::power(a, b)))
        }),
        "EXP" => one(|n| Ok(n.exp())),
        "LN" => one(|n| {
            if n <= 0.0 {
                Err(ErrorValue::Num)
            } else {
                Ok(n.ln())
            }
        }),
        "LOG10" => one(|n| {
            if n <= 0.0 {
                Err(ErrorValue::Num)
            } else {
                Ok(n.log10())
            }
        }),
        "LOG" => lift_number(ev, args, &|n| {
            logarithm(n[0], n.get(1).copied().unwrap_or(10.0))
        }),
        "PI" => CalcValue::number(std::f64::consts::PI),
        "SIN" => one(|n| periodic(n).map(f64::sin)),
        "COS" => one(|n| periodic(n).map(f64::cos)),
        "TAN" => one(|n| periodic(n).map(f64::tan)),
        "ASIN" => one(|n| {
            if n.abs() > 1.0 {
                Err(ErrorValue::Num)
            } else {
                Ok(n.asin())
            }
        }),
        "ACOS" => one(|n| {
            if n.abs() > 1.0 {
                Err(ErrorValue::Num)
            } else {
                Ok(n.acos())
            }
        }),
        "ATAN" => one(|n| Ok(n.atan())),
        // excel's ATAN2 takes x first, then y
        "ATAN2" => lift_number(ev, args, &|n| {
            two(n).and_then(|(x, y)| {
                if x == 0.0 && y == 0.0 {
                    Err(ErrorValue::Div0)
                } else {
                    Ok(y.atan2(x))
                }
            })
        }),
        "SINH" => one(|n| Ok(n.sinh())),
        "COSH" => one(|n| Ok(n.cosh())),
        "TANH" => one(|n| Ok(n.tanh())),
        "DEGREES" => one(|n| Ok(n.to_degrees())),
        "RADIANS" => one(|n| Ok(n.to_radians())),
        "CEILING" => lift_number(ev, args, &|n| two(n).and_then(|(a, b)| ceiling(a, b))),
        "FLOOR" => lift_number(ev, args, &|n| two(n).and_then(|(a, b)| floor(a, b))),
        "CEILING.MATH" => lift_number(ev, args, &|n| {
            Ok(ceiling_math(
                n[0],
                n.get(1).copied().unwrap_or(1.0),
                n.get(2).copied().unwrap_or(0.0),
            ))
        }),
        "FLOOR.MATH" => lift_number(ev, args, &|n| {
            Ok(floor_math(
                n[0],
                n.get(1).copied().unwrap_or(1.0),
                n.get(2).copied().unwrap_or(0.0),
            ))
        }),
        "EVEN" => one(|n| Ok(away_to_multiple(n, 2.0, 0.0))),
        "ODD" => one(|n| Ok(odd(n))),
        "FACT" => one(factorial),
        "COMBIN" => lift_number(ev, args, &|n| two(n).and_then(|(a, b)| combinations(a, b))),
        "PERMUT" => lift_number(ev, args, &|n| two(n).and_then(|(a, b)| permutations(a, b))),
        "GCD" => done_number(numbers(ev, args).and_then(|n| gcd_all(&n))),
        "LCM" => done_number(numbers(ev, args).and_then(|n| lcm_all(&n))),
        "QUOTIENT" => lift_number(ev, args, &|n| {
            two(n).and_then(|(a, b)| {
                if b == 0.0 {
                    Err(ErrorValue::Div0)
                } else {
                    Ok((a / b).trunc())
                }
            })
        }),
        "RAND" => CalcValue::number(ev.ctx.random()),
        "RANDBETWEEN" => lift_number(ev, args, &|n| {
            two(n).and_then(|(a, b)| random_between(ev, a, b))
        }),
        _ => return None,
    })
}

/// excel refuses angles of 2^27 and more for SIN, COS and TAN (#NUM!),
/// where a double no longer has the precision to place the angle
const LARGEST_ANGLE: f64 = 134_217_728.0;

fn periodic(n: f64) -> Outcome<f64> {
    if n.abs() >= LARGEST_ANGLE {
        Err(ErrorValue::Num)
    } else {
        Ok(n)
    }
}

fn two(n: &[f64]) -> Outcome<(f64, f64)> {
    match n {
        [a, b] => Ok((*a, *b)),
        _ => Err(ErrorValue::Value),
    }
}

fn as_number(value: Value) -> Outcome<f64> {
    match value {
        Value::Number(n) => Ok(n),
        Value::Error(e) => Err(e),
        _ => Err(ErrorValue::Value),
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum Rounding {
    /// half away from zero (ROUND)
    Nearest,
    /// away from zero (ROUNDUP)
    Up,
    /// toward zero (ROUNDDOWN, TRUNC)
    Down,
}

fn rounding(ev: &Evaluator, name: &str, args: &[Expr]) -> CalcValue {
    let mode = match name {
        "ROUNDUP" => Rounding::Up,
        "ROUNDDOWN" => Rounding::Down,
        _ => Rounding::Nearest,
    };
    lift_number(ev, args, &|n| {
        two(n).map(|(x, digits)| round_digits(x, digits, mode))
    })
}

/// rounds to `digits` decimals (negative: tens, hundreds) on the number's
/// 15 significant digit decimal form, which is what excel rounds: ROUND
/// of 2.675 to 2 places is 2.68 although the stored double is just below
pub fn round_digits(x: f64, digits: f64, mode: Rounding) -> f64 {
    if x == 0.0 || !x.is_finite() {
        return x;
    }
    let digits = digits.trunc() as i32;
    let text = format!("{:.*e}", SIGNIFICANT_DIGITS - 1, x.abs());
    let (mantissa, exponent) = text.split_once('e').unwrap_or((&text, "0"));
    let exponent: i32 = exponent.parse().unwrap_or(0);
    let all: Vec<u8> = mantissa
        .bytes()
        .filter(u8::is_ascii_digit)
        .map(|b| b - b'0')
        .collect();
    // how many leading digits survive
    let keep = exponent + 1 + digits;
    if keep >= all.len() as i32 {
        return x;
    }
    let (kept, dropped): (&[u8], &[u8]) = if keep <= 0 {
        (&[], &all[..])
    } else {
        all.split_at(keep as usize)
    };
    let mut whole = kept.iter().fold(0f64, |n, d| n * 10.0 + f64::from(*d));
    let first_dropped = if keep < 0 {
        0
    } else {
        dropped.first().copied().unwrap_or(0)
    };
    let any_dropped = dropped.iter().any(|d| *d != 0);
    let bump = match mode {
        Rounding::Nearest => first_dropped >= 5,
        Rounding::Up => any_dropped,
        Rounding::Down => false,
    };
    if bump {
        whole += 1.0;
    }
    let scaled = if digits >= 0 {
        whole / 10f64.powi(digits)
    } else {
        whole * 10f64.powi(-digits)
    };
    x.signum() * scaled
}

fn mround(n: f64, multiple: f64) -> Outcome<f64> {
    if multiple == 0.0 {
        return Ok(0.0);
    }
    if n * multiple < 0.0 {
        return Err(ErrorValue::Num);
    }
    let units = round_digits(round_significant(n / multiple), 0.0, Rounding::Nearest);
    Ok(units * multiple)
}

/// excel's MOD: the result has the divisor's sign; very large quotients
/// are refused with #NUM! as excel does
fn modulo(n: f64, divisor: f64) -> Outcome<f64> {
    if divisor == 0.0 {
        return Err(ErrorValue::Div0);
    }
    let quotient = n / divisor;
    if quotient.abs() >= 1.0e15 {
        return Err(ErrorValue::Num);
    }
    let result = n - divisor * round_significant(quotient).floor();
    Ok(round_significant(result))
}

fn logarithm(n: f64, base: f64) -> Outcome<f64> {
    if n <= 0.0 || base <= 0.0 {
        return Err(ErrorValue::Num);
    }
    if base == 1.0 {
        return Err(ErrorValue::Div0);
    }
    Ok(n.ln() / base.ln())
}

/// excel 2010's CEILING: toward +infinity for a positive significance;
/// a positive number with a negative significance is #NUM!
fn ceiling(n: f64, significance: f64) -> Outcome<f64> {
    if significance == 0.0 {
        return Ok(0.0);
    }
    if n > 0.0 && significance < 0.0 {
        return Err(ErrorValue::Num);
    }
    Ok(round_significant(n / significance).ceil() * significance)
}

fn floor(n: f64, significance: f64) -> Outcome<f64> {
    if significance == 0.0 {
        return if n == 0.0 {
            Ok(0.0)
        } else {
            Err(ErrorValue::Div0)
        };
    }
    if n > 0.0 && significance < 0.0 {
        return Err(ErrorValue::Num);
    }
    Ok(round_significant(n / significance).floor() * significance)
}

fn ceiling_math(n: f64, significance: f64, mode: f64) -> f64 {
    let s = significance.abs();
    if s == 0.0 {
        return 0.0;
    }
    let units = round_significant(n / s);
    let rounded = if n < 0.0 && mode != 0.0 {
        units.floor()
    } else {
        units.ceil()
    };
    rounded * s
}

fn floor_math(n: f64, significance: f64, mode: f64) -> f64 {
    let s = significance.abs();
    if s == 0.0 {
        return 0.0;
    }
    let units = round_significant(n / s);
    let rounded = if n < 0.0 && mode != 0.0 {
        units.ceil()
    } else {
        units.floor()
    };
    rounded * s
}

/// rounds away from zero to the next multiple of `step` plus `offset`
fn away_to_multiple(n: f64, step: f64, offset: f64) -> f64 {
    let magnitude = ((n.abs() - offset) / step).ceil() * step + offset;
    n.signum() * magnitude.max(offset)
}

fn odd(n: f64) -> f64 {
    if n == 0.0 {
        return 1.0;
    }
    let magnitude = ((n.abs() - 1.0) / 2.0).ceil() * 2.0 + 1.0;
    n.signum() * magnitude
}

fn factorial(n: f64) -> Outcome<f64> {
    if n < 0.0 {
        return Err(ErrorValue::Num);
    }
    let n = n.trunc() as u32;
    if n > 170 {
        return Err(ErrorValue::Num);
    }
    Ok((1..=n).fold(1.0, |product, k| product * f64::from(k)))
}

fn combinations(n: f64, k: f64) -> Outcome<f64> {
    let (n, k) = (n.trunc(), k.trunc());
    if n < 0.0 || k < 0.0 || k > n {
        return Err(ErrorValue::Num);
    }
    let k = k.min(n - k);
    let mut result = 1.0;
    for i in 0..k as u64 {
        result = result * (n - i as f64) / (i as f64 + 1.0);
    }
    Ok(result.round())
}

fn permutations(n: f64, k: f64) -> Outcome<f64> {
    let (n, k) = (n.trunc(), k.trunc());
    if n < 0.0 || k < 0.0 || k > n {
        return Err(ErrorValue::Num);
    }
    Ok((0..k as u64).fold(1.0, |product, i| product * (n - i as f64)))
}

fn gcd(a: f64, b: f64) -> f64 {
    let (mut a, mut b) = (a, b);
    while b != 0.0 {
        let r = a % b;
        a = b;
        b = r;
    }
    a
}

fn whole_numbers(n: &[f64]) -> Outcome<Vec<f64>> {
    if n.iter().any(|x| *x < 0.0) {
        return Err(ErrorValue::Num);
    }
    Ok(n.iter().map(|x| x.trunc()).collect())
}

fn gcd_all(n: &[f64]) -> Outcome<f64> {
    Ok(whole_numbers(n)?.into_iter().fold(0.0, gcd))
}

fn lcm_all(n: &[f64]) -> Outcome<f64> {
    let n = whole_numbers(n)?;
    if n.contains(&0.0) {
        return Ok(0.0);
    }
    Ok(n.into_iter().fold(1.0, |a, b| a / gcd(a, b) * b))
}

fn random_between(ev: &Evaluator, low: f64, high: f64) -> Outcome<f64> {
    let (low, high) = (low.ceil(), high.floor());
    if low > high {
        return Err(ErrorValue::Num);
    }
    Ok(low + (ev.ctx.random() * (high - low + 1.0)).floor())
}

/// SUMPRODUCT: arrays of the same shape multiplied item by item and
/// summed; items that are not numbers count as 0
fn sum_product(ev: &Evaluator, args: &[Expr]) -> Outcome<f64> {
    let grids: Vec<_> = args
        .iter()
        .map(|arg| grid(ev, arg))
        .collect::<Outcome<_>>()?;
    let Some(first) = grids.first() else {
        return Err(ErrorValue::Value);
    };
    if grids
        .iter()
        .any(|g| g.rows != first.rows || g.cols != first.cols)
    {
        return Err(ErrorValue::Value);
    }
    let mut total = 0.0;
    for index in 0..first.items.len() {
        let mut product = 1.0;
        for grid in &grids {
            match &grid.items[index] {
                Value::Number(n) => product *= n,
                Value::Error(e) => return Err(*e),
                _ => product = 0.0,
            }
        }
        total += product;
    }
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounding_matches_excel_on_decimal_ties() {
        assert_eq!(round_digits(2.675, 2.0, Rounding::Nearest), 2.68);
        assert_eq!(round_digits(-2.5, 0.0, Rounding::Nearest), -3.0);
        assert_eq!(round_digits(1234.5678, -2.0, Rounding::Nearest), 1200.0);
        assert_eq!(round_digits(5.14159, 3.0, Rounding::Up), 5.142);
        assert_eq!(round_digits(-5.14159, 3.0, Rounding::Down), -5.141);
        assert_eq!(round_digits(0.0001234, 5.0, Rounding::Nearest), 0.00012);
        assert_eq!(round_digits(5.0, -1.0, Rounding::Nearest), 10.0);
        assert_eq!(round_digits(4.0, -1.0, Rounding::Nearest), 0.0);
        assert_eq!(round_digits(0.4, -3.0, Rounding::Up), 1000.0);
    }

    #[test]
    fn integer_parts_and_multiples() {
        assert_eq!(modulo(-3.0, 2.0), Ok(1.0));
        assert_eq!(modulo(3.0, -2.0), Ok(-1.0));
        assert_eq!(ceiling(-2.5, 2.0), Ok(-2.0));
        assert_eq!(ceiling(2.5, -2.0), Err(ErrorValue::Num));
        assert_eq!(floor(-2.5, 2.0), Ok(-4.0));
        assert_eq!(ceiling_math(-5.5, 2.0, 1.0), -6.0);
        assert_eq!(floor_math(-5.5, 2.0, 1.0), -4.0);
        assert_eq!(mround(10.0, 3.0), Ok(9.0));
        assert_eq!(away_to_multiple(-1.5, 2.0, 0.0), -2.0);
        assert_eq!(odd(2.0), 3.0);
        assert_eq!(odd(-2.0), -3.0);
        assert_eq!(combinations(8.0, 2.0), Ok(28.0));
        assert_eq!(factorial(5.0), Ok(120.0));
        assert_eq!(gcd_all(&[24.0, 36.0]), Ok(12.0));
        assert_eq!(lcm_all(&[4.0, 6.0]), Ok(12.0));
    }
}
