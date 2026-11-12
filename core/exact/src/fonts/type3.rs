//! type 3 glyphs: the filled paths of each glyph procedure become outlines
//! (recipe 3.1).
//!
//! a procedure's coordinates go through its `cm` and the font matrix
//! times 1000 into a 1000-unit em with y up (browsers flip y with a
//! negative d). strokes, pictures and text inside a glyph are not outlines;
//! they are reported instead of rebuilt.

use super::outline::{Outline, PathOp};
use crate::pdf::lexer::{parse, Operand};
use crate::pdf::matrix::{apply, multiply, Matrix, IDENTITY};

/// the filled paths of one glyph procedure, and what could not be rebuilt.
pub fn type3_outline(
    procedure: &[u8],
    font_matrix: &[f64; 6],
    problems: &mut Vec<String>,
) -> Outline {
    let em: Matrix = font_matrix.map(|v| v * 1000.0);
    let mut stack: Vec<Matrix> = Vec::new();
    let mut ctm = IDENTITY;
    let mut outline = Outline::default();
    let mut path: Vec<(String, Vec<f64>)> = Vec::new();
    for operation in parse(procedure) {
        let op = operation.operator.as_str();
        let values: Vec<f64> = operation
            .operands
            .iter()
            .filter_map(Operand::number)
            .collect();
        match op {
            "q" => stack.push(ctm),
            "Q" => {
                if let Some(saved) = stack.pop() {
                    ctm = saved;
                }
            }
            "cm" if values.len() == 6 => {
                let m: Matrix = [
                    values[0], values[1], values[2], values[3], values[4], values[5],
                ];
                ctm = multiply(&m, &ctm);
            }
            "m" | "l" | "c" | "v" | "y" | "h" | "re" => path.push((op.to_string(), values)),
            "f" | "F" | "f*" | "B" | "B*" | "b" | "b*" => {
                let to_em = multiply(&ctm, &em);
                replay_path(&path, &to_em, &mut outline);
                path.clear();
            }
            "S" | "s" | "n" => {
                if op != "n" && !path.is_empty() {
                    problems.push("type3 glyph strokes a path; strokes are not rebuilt".into());
                }
                path.clear();
            }
            "Do" | "BI" | "BT" => problems.push(format!(
                "type3 glyph uses {op}; only filled paths are rebuilt"
            )),
            _ => {}
        }
    }
    outline
}

fn replay_path(path: &[(String, Vec<f64>)], to_em: &Matrix, outline: &mut Outline) {
    let at = |x: f64, y: f64| apply(to_em, x, y);
    let mut current = (0.0, 0.0);
    let mut start = (0.0, 0.0);
    let mut open = false;
    for (op, v) in path {
        match (op.as_str(), v.as_slice()) {
            ("re", [x, y, w, h]) => {
                if open {
                    outline.ops.push(PathOp::Close);
                }
                let corners = [at(*x, *y), at(x + w, *y), at(x + w, y + h), at(*x, y + h)];
                outline.ops.push(PathOp::Move(corners[0].0, corners[0].1));
                for corner in &corners[1..] {
                    outline.ops.push(PathOp::Line(corner.0, corner.1));
                }
                outline.ops.push(PathOp::Close);
                open = false;
                current = (*x, *y);
                start = current;
            }
            ("m", [x, y]) => {
                if open {
                    outline.ops.push(PathOp::Close);
                }
                current = (*x, *y);
                start = current;
                let p = at(*x, *y);
                outline.ops.push(PathOp::Move(p.0, p.1));
                open = true;
            }
            ("l", [x, y]) => {
                current = (*x, *y);
                let p = at(*x, *y);
                outline.ops.push(PathOp::Line(p.0, p.1));
            }
            ("c", [x1, y1, x2, y2, x3, y3]) => {
                push_curve(outline, at(*x1, *y1), at(*x2, *y2), at(*x3, *y3));
                current = (*x3, *y3);
            }
            ("v", [x2, y2, x3, y3]) => {
                push_curve(
                    outline,
                    at(current.0, current.1),
                    at(*x2, *y2),
                    at(*x3, *y3),
                );
                current = (*x3, *y3);
            }
            ("y", [x1, y1, x3, y3]) => {
                push_curve(outline, at(*x1, *y1), at(*x3, *y3), at(*x3, *y3));
                current = (*x3, *y3);
            }
            ("h", _) if open => {
                outline.ops.push(PathOp::Close);
                open = false;
                current = start;
            }
            _ => {}
        }
    }
    if open {
        outline.ops.push(PathOp::Close);
    }
}

fn push_curve(outline: &mut Outline, a: (f64, f64), b: (f64, f64), c: (f64, f64)) {
    outline
        .ops
        .push(PathOp::Cubic(a.0, a.1, b.0, b.1, c.0, c.1));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filled_path_with_flipped_font_matrix() {
        // the prototype's case (test_writer_model.py)
        let procedure = b"500 0 0 -700 400 0 d1\n0 0 m\n400 0 l\n400 -700 l\nh\nf\n";
        let mut problems = Vec::new();
        let outline = type3_outline(
            procedure,
            &[0.001, 0.0, 0.0, -0.001, 0.0, 0.0],
            &mut problems,
        );
        let points: Vec<(f64, f64)> = outline
            .ops
            .iter()
            .filter_map(|op| match op {
                PathOp::Move(x, y) | PathOp::Line(x, y) => Some((*x, *y)),
                _ => None,
            })
            .collect();
        assert_eq!(points, [(0.0, 0.0), (400.0, 0.0), (400.0, 700.0)]);
        assert!(problems.is_empty());
    }

    #[test]
    fn strokes_are_reported() {
        let mut problems = Vec::new();
        type3_outline(
            b"0 0 m 10 10 l S",
            &[0.001, 0.0, 0.0, 0.001, 0.0, 0.0],
            &mut problems,
        );
        assert!(!problems.is_empty());
    }
}
