//! money over time: PMT, IPMT, PPMT, PV, FV, NPER, RATE, NPV and IRR,
//! with excel's sign convention (money paid out is negative) and `type`
//! 1 for payments at the start of each period.

use crate::formula::Expr;
use crate::model::ErrorValue;

use super::super::eval::Evaluator;
use super::super::value::CalcValue;
use super::args::*;

pub const NAMES: &[&str] = &[
    "PMT", "IPMT", "PPMT", "PV", "FV", "NPER", "RATE", "NPV", "IRR",
];

/// RATE and IRR stop after this many newton steps (excel uses 20 for
/// RATE and 20 for IRR, then gives #NUM!)
const MAX_ITERATIONS: usize = 100;
/// and accept an answer this close (excel documents 0.0000001 for RATE)
const TOLERANCE: f64 = 1e-10;

pub fn call(ev: &Evaluator, name: &str, args: &[Expr]) -> Option<CalcValue> {
    let with = |defaults: &[f64], f: fn(&[f64]) -> Outcome<f64>| {
        done_number(arguments(ev, args, defaults).and_then(|n| f(&n)))
    };
    Some(match name {
        // (rate, nper, pv, [fv], [type])
        "PMT" => with(&[f64::NAN, f64::NAN, f64::NAN, 0.0, 0.0], |n| {
            payment(n[0], n[1], n[2], n[3], n[4])
        }),
        // (rate, per, nper, pv, [fv], [type])
        "IPMT" => with(&[f64::NAN, f64::NAN, f64::NAN, f64::NAN, 0.0, 0.0], |n| {
            interest_part(n[0], n[1], n[2], n[3], n[4], n[5])
        }),
        "PPMT" => with(&[f64::NAN, f64::NAN, f64::NAN, f64::NAN, 0.0, 0.0], |n| {
            let total = payment(n[0], n[2], n[3], n[4], n[5])?;
            Ok(total - interest_part(n[0], n[1], n[2], n[3], n[4], n[5])?)
        }),
        // (rate, nper, pmt, [fv], [type])
        "PV" => with(&[f64::NAN, f64::NAN, f64::NAN, 0.0, 0.0], |n| {
            present_value(n[0], n[1], n[2], n[3], n[4])
        }),
        // (rate, nper, pmt, [pv], [type])
        "FV" => with(&[f64::NAN, f64::NAN, f64::NAN, 0.0, 0.0], |n| {
            Ok(future_value(n[0], n[1], n[2], n[3], n[4]))
        }),
        // (rate, pmt, pv, [fv], [type])
        "NPER" => with(&[f64::NAN, f64::NAN, f64::NAN, 0.0, 0.0], |n| {
            periods(n[0], n[1], n[2], n[3], n[4])
        }),
        // (nper, pmt, pv, [fv], [type], [guess])
        "RATE" => with(&[f64::NAN, f64::NAN, f64::NAN, 0.0, 0.0, 0.1], |n| {
            rate(n[0], n[1], n[2], n[3], n[4], n[5])
        }),
        "NPV" => done_number(npv(ev, args)),
        "IRR" => done_number(irr(ev, args)),
        _ => return None,
    })
}

/// the arguments as numbers; NAN in `defaults` marks a required one
fn arguments(ev: &Evaluator, args: &[Expr], defaults: &[f64]) -> Outcome<Vec<f64>> {
    if args.len() > defaults.len() {
        return Err(ErrorValue::Value);
    }
    let mut values = Vec::with_capacity(defaults.len());
    for (index, default) in defaults.iter().enumerate() {
        let value = number_or(ev, args, index, *default)?;
        if value.is_nan() {
            return Err(ErrorValue::Value);
        }
        values.push(value);
    }
    Ok(values)
}

/// type 1 means payments at the start of each period
fn at_start(kind: f64) -> f64 {
    if kind != 0.0 {
        1.0
    } else {
        0.0
    }
}

pub fn future_value(rate: f64, nper: f64, pmt: f64, pv: f64, kind: f64) -> f64 {
    if rate == 0.0 {
        return -(pv + pmt * nper);
    }
    // growth - 1 through ln_1p and exp_m1 keeps its precision for rates near zero
    let growth_less_one = (nper * rate.ln_1p()).exp_m1();
    let growth = growth_less_one + 1.0;
    -(pv * growth + pmt * (1.0 + rate * at_start(kind)) * growth_less_one / rate)
}

fn payment(rate: f64, nper: f64, pv: f64, fv: f64, kind: f64) -> Outcome<f64> {
    if nper == 0.0 {
        return Err(ErrorValue::Num);
    }
    if rate == 0.0 {
        return Ok(-(pv + fv) / nper);
    }
    let growth = (1.0 + rate).powf(nper);
    Ok(-(fv + pv * growth) * rate / ((1.0 + rate * at_start(kind)) * (growth - 1.0)))
}

fn present_value(rate: f64, nper: f64, pmt: f64, fv: f64, kind: f64) -> Outcome<f64> {
    if rate == 0.0 {
        return Ok(-(fv + pmt * nper));
    }
    let growth = (1.0 + rate).powf(nper);
    Ok(-(fv + pmt * (1.0 + rate * at_start(kind)) * (growth - 1.0) / rate) / growth)
}

/// the interest part of payment `per`
fn interest_part(rate: f64, per: f64, nper: f64, pv: f64, fv: f64, kind: f64) -> Outcome<f64> {
    if per < 1.0 || per > nper {
        return Err(ErrorValue::Num);
    }
    let pmt = payment(rate, nper, pv, fv, kind)?;
    if at_start(kind) == 0.0 {
        // the balance after per - 1 payments, times the rate
        return Ok(future_value(rate, per - 1.0, pmt, pv, kind) * rate);
    }
    // payments in advance: nothing is owed on the first one
    if per == 1.0 {
        return Ok(0.0);
    }
    Ok(future_value(rate, per - 1.0, pmt, pv, kind) * rate / (1.0 + rate))
}

fn periods(rate: f64, pmt: f64, pv: f64, fv: f64, kind: f64) -> Outcome<f64> {
    if rate == 0.0 {
        if pmt == 0.0 {
            return Err(ErrorValue::Num);
        }
        return Ok(-(pv + fv) / pmt);
    }
    let adjusted = pmt * (1.0 + rate * at_start(kind)) / rate;
    let ratio = (adjusted - fv) / (adjusted + pv);
    if ratio <= 0.0 {
        return Err(ErrorValue::Num);
    }
    Ok(ratio.ln() / (1.0 + rate).ln())
}

/// RATE by newton's method on the future value equation
fn rate(nper: f64, pmt: f64, pv: f64, fv: f64, kind: f64, guess: f64) -> Outcome<f64> {
    let f = |r: f64| -future_value(r, nper, pmt, pv, kind) - fv;
    let mut r = guess;
    for _ in 0..MAX_ITERATIONS {
        let step = 1e-6 * r.abs().max(1e-6);
        let value = f(r);
        let slope = (f(r + step) - value) / step;
        if slope == 0.0 || !slope.is_finite() {
            return Err(ErrorValue::Num);
        }
        let next = r - value / slope;
        if (next - r).abs() < TOLERANCE {
            return Ok(next);
        }
        r = next;
    }
    Err(ErrorValue::Num)
}

/// NPV(rate, values...): values at the end of periods 1, 2, ...
fn npv(ev: &Evaluator, args: &[Expr]) -> Outcome<f64> {
    if args.len() < 2 {
        return Err(ErrorValue::Value);
    }
    let rate = number(ev, &args[0])?;
    if rate == -1.0 {
        return Err(ErrorValue::Div0);
    }
    let flows = numbers(ev, &args[1..])?;
    Ok(flows
        .iter()
        .enumerate()
        .map(|(i, v)| v / (1.0 + rate).powi(i as i32 + 1))
        .sum())
}

fn net_present(rate: f64, flows: &[f64]) -> f64 {
    flows
        .iter()
        .enumerate()
        .map(|(i, v)| v / (1.0 + rate).powi(i as i32))
        .sum()
}

/// IRR(values, [guess]): the rate where the net present value is zero;
/// needs at least one payment and one receipt
fn irr(ev: &Evaluator, args: &[Expr]) -> Outcome<f64> {
    arity(args, 1, 2)?;
    let flows = numbers(ev, &args[..1])?;
    if !flows.iter().any(|v| *v > 0.0) || !flows.iter().any(|v| *v < 0.0) {
        return Err(ErrorValue::Num);
    }
    let mut r = number_or(ev, args, 1, 0.1)?;
    for _ in 0..MAX_ITERATIONS {
        let value = net_present(r, &flows);
        let slope: f64 = flows
            .iter()
            .enumerate()
            .map(|(i, v)| -(i as f64) * v / (1.0 + r).powi(i as i32 + 1))
            .sum();
        if slope == 0.0 || !slope.is_finite() {
            return Err(ErrorValue::Num);
        }
        let next = r - value / slope;
        if (next - r).abs() < TOLERANCE {
            return Ok(next);
        }
        r = next;
    }
    Err(ErrorValue::Num)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9 * b.abs().max(1.0)
    }

    #[test]
    fn loan_arithmetic_matches_excels_examples() {
        // excel's help: PMT(8%/12, 10, 10000) = -1037.03
        assert!(close(
            payment(0.08 / 12.0, 10.0, 10000.0, 0.0, 0.0).unwrap(),
            -1_037.032_089_359_164_4
        ));
        assert!((future_value(0.06 / 12.0, 10.0, -200.0, -500.0, 1.0) - 2581.4034).abs() < 1e-3);
        assert!(close(
            present_value(0.08 / 12.0, 240.0, 500.0, 0.0, 0.0).unwrap(),
            -59_777.145_851_187_8
        ));
        assert!(close(
            periods(0.12 / 12.0, -100.0, -1000.0, 10000.0, 1.0).unwrap(),
            59.673_865_674_294_6
        ));
        assert!(close(
            rate(48.0, -200.0, 8000.0, 0.0, 0.0, 0.1).unwrap(),
            0.007_701_472_488_246_0
        ));
        assert!(close(net_present(0.1, &[-100.0, 110.0]), 0.0));
    }
}
