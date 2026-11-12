//! reading a generator's reply strictly: json, path data, closed
//! contours, bounds, crossings and advances, each refusal with its reason.

use super::*;

/// why a generated glyph was refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Rejection {
    Unreadable,
    NotAsked,
    BadPath(String),
    OpenContour,
    TooManyPoints,
    OutOfBounds,
    TinyContour,
    Crossing,
    BadAdvance,
}

/// one generated glyph that passed every check, in em units.
#[derive(Clone, Debug, PartialEq)]
pub struct GeneratedGlyph {
    pub character: char,
    pub glyph: GlyphData,
}

#[derive(Deserialize)]
pub(crate) struct Reply {
    pub(crate) glyphs: Vec<ReplyGlyph>,
}

#[derive(Deserialize)]
pub(crate) struct ReplyGlyph {
    #[serde(rename = "char")]
    pub(crate) character: String,
    pub(crate) advance: f64,
    pub(crate) path: String,
}

/// reads and checks a reply. each glyph either passes every check or is
/// listed with why it was refused. `expected_advance` gives, per
/// character, the advance it should roughly have.
pub fn read_reply(
    text: &str,
    asked: &[char],
    expected_advance: &dyn Fn(char) -> f64,
) -> (Vec<GeneratedGlyph>, Vec<(char, Rejection)>) {
    let mut accepted = Vec::new();
    let mut refused = Vec::new();
    if text.len() > MAX_REPLY_BYTES {
        return (accepted, vec![('\u{FFFD}', Rejection::Unreadable)]);
    }
    // providers sometimes wrap json in prose or a code block
    let json = text
        .find('{')
        .and_then(|start| text.rfind('}').map(|end| &text[start..=end]))
        .unwrap_or("");
    let Ok(reply) = serde_json::from_str::<Reply>(json) else {
        return (accepted, vec![('\u{FFFD}', Rejection::Unreadable)]);
    };
    for item in reply.glyphs.into_iter().take(MAX_WANTED) {
        let mut chars = item.character.chars();
        let (Some(c), None) = (chars.next(), chars.next()) else {
            refused.push(('\u{FFFD}', Rejection::Unreadable));
            continue;
        };
        if !asked.contains(&c) || accepted.iter().any(|g: &GeneratedGlyph| g.character == c) {
            refused.push((c, Rejection::NotAsked));
            continue;
        }
        match check_glyph(&item.path, item.advance / GRID, expected_advance(c)) {
            Ok(glyph) => accepted.push(GeneratedGlyph {
                character: c,
                glyph,
            }),
            Err(why) => refused.push((c, why)),
        }
    }
    (accepted, refused)
}

/// parses and checks one glyph's path and advance.
pub fn check_glyph(
    path: &str,
    advance: f64,
    expected_advance: f64,
) -> Result<GlyphData, Rejection> {
    let shape = parse_path(path)?;
    if shape.point_count() > MAX_POINTS {
        return Err(Rejection::TooManyPoints);
    }
    let bounds = shape.bounds().ok_or(Rejection::TinyContour)?;
    let inside =
        bounds.x_min >= -0.3 && bounds.x_max <= 1.6 && bounds.y_min >= -0.5 && bounds.y_max <= 1.2;
    if !inside {
        return Err(Rejection::OutOfBounds);
    }
    // crossings first: a bow tie's halves cancel, so it would also look
    // like a contour of no area
    if has_crossings(&shape) {
        return Err(Rejection::Crossing);
    }
    if shape
        .contours
        .iter()
        .any(|c| c.nodes.len() < 3 || c.signed_area().abs() < 1e-5)
    {
        return Err(Rejection::TinyContour);
    }
    let believable = advance.is_finite()
        && advance >= bounds.width() * 0.8
        && advance >= expected_advance * 0.5
        && advance <= expected_advance * 1.8;
    if !believable {
        return Err(Rejection::BadAdvance);
    }
    Ok(GlyphData {
        shape: shape.with_nesting_direction(),
        advance,
    })
}

/// reads absolute M, L, Q, C and Z path data on the grid. every contour
/// must end with Z.
pub fn parse_path(path: &str) -> Result<Shape, Rejection> {
    let bad = |why: &str| Rejection::BadPath(why.to_string());
    if path.len() > 64 * 1024 {
        return Err(Rejection::TooManyPoints);
    }
    let mut tokens = tokenise(path).ok_or_else(|| bad("unreadable"))?.into_iter();
    let mut builder = ShapeBuilder::new(1.0 / GRID);
    let mut open = false;
    while let Some(token) = tokens.next() {
        let Token::Command(command) = token else {
            return Err(bad("a number without a command"));
        };
        match command {
            'M' => {
                if open {
                    return Err(Rejection::OpenContour);
                }
                let v = numbers(&mut tokens, 2)?;
                builder.move_to(v[0], v[1]);
                open = true;
            }
            'L' | 'Q' | 'C' if !open => return Err(bad("a drawing command before M")),
            'L' => {
                let v = numbers(&mut tokens, 2)?;
                builder.line_to(v[0], v[1]);
            }
            'Q' => {
                let v = numbers(&mut tokens, 4)?;
                builder.quad_to(v[0], v[1], v[2], v[3]);
            }
            'C' => {
                let v = numbers(&mut tokens, 6)?;
                builder.cubic_to(v[0], v[1], v[2], v[3], v[4], v[5]);
            }
            'Z' => {
                if !open {
                    return Err(bad("Z without a contour"));
                }
                builder.close();
                open = false;
            }
            _ => return Err(bad("only M, L, Q, C and Z are allowed")),
        }
    }
    if open {
        return Err(Rejection::OpenContour);
    }
    let shape = builder.finish().cleaned();
    if shape.is_empty() {
        return Err(Rejection::TinyContour);
    }
    Ok(shape)
}

/// the next `count` numbers of a command.
pub(crate) fn numbers(
    tokens: &mut impl Iterator<Item = Token>,
    count: usize,
) -> Result<Vec<f64>, Rejection> {
    let mut values = Vec::with_capacity(count);
    for _ in 0..count {
        match tokens.next() {
            Some(Token::Number(value)) if value.is_finite() && value.abs() < 1e5 => {
                values.push(value)
            }
            _ => return Err(Rejection::BadPath("a command is missing numbers".into())),
        }
    }
    Ok(values)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Token {
    Command(char),
    Number(f64),
}

pub(crate) fn tokenise(path: &str) -> Option<Vec<Token>> {
    let mut tokens = Vec::new();
    let mut number = String::new();
    let flush = |number: &mut String, tokens: &mut Vec<Token>| -> Option<()> {
        if !number.is_empty() {
            tokens.push(Token::Number(number.parse().ok()?));
            number.clear();
        }
        Some(())
    };
    for c in path.chars() {
        match c {
            '0'..='9' | '.' => number.push(c),
            '-' => {
                flush(&mut number, &mut tokens)?;
                number.push(c);
            }
            ' ' | ',' | '\n' | '\t' | '\r' => flush(&mut number, &mut tokens)?,
            'A'..='Z' | 'a'..='z' => {
                flush(&mut number, &mut tokens)?;
                tokens.push(Token::Command(c));
            }
            _ => return None,
        }
        if tokens.len() > 20_000 {
            return None;
        }
    }
    flush(&mut number, &mut tokens)?;
    Some(tokens)
}

/// one straight edge of a flattened outline, and where it sits.
pub(crate) struct Edge {
    pub(crate) from: (f64, f64),
    pub(crate) to: (f64, f64),
    pub(crate) contour: usize,
    /// its place in its contour
    pub(crate) index: usize,
    /// how many edges its contour has
    pub(crate) contour_edges: usize,
}

impl Edge {
    /// neighbours on one contour share an end, which is not a crossing.
    pub(crate) fn is_next_to(&self, other: &Edge) -> bool {
        if self.contour != other.contour {
            return false;
        }
        let gap = self.index.abs_diff(other.index);
        gap <= 1 || gap == self.contour_edges - 1
    }
}

/// more edges than this and a glyph is refused unchecked: real glyphs
/// flatten to a few hundred, and the check compares every pair.
pub(crate) const MAX_CHECKED_EDGES: usize = 4000;

pub(crate) fn edges_of(shape: &Shape) -> Vec<Edge> {
    let mut edges = Vec::new();
    for (contour, polygon) in shape.polygons(0.01).iter().enumerate() {
        for (index, from) in polygon.iter().enumerate() {
            edges.push(Edge {
                from: *from,
                to: polygon[(index + 1) % polygon.len()],
                contour,
                index,
                contour_edges: polygon.len(),
            });
        }
    }
    edges
}

/// true when any two edges of the outline cross (touching at a shared
/// end is fine): such glyphs fill unpredictably.
pub(crate) fn has_crossings(shape: &Shape) -> bool {
    let edges = edges_of(shape);
    if edges.len() > MAX_CHECKED_EDGES {
        return true;
    }
    for (at, first) in edges.iter().enumerate() {
        for second in &edges[at + 1..] {
            if !first.is_next_to(second)
                && segments_cross(first.from, first.to, second.from, second.to)
            {
                return true;
            }
        }
    }
    false
}

pub(crate) fn segments_cross(a: (f64, f64), b: (f64, f64), c: (f64, f64), d: (f64, f64)) -> bool {
    let cross = |o: (f64, f64), p: (f64, f64), q: (f64, f64)| {
        (p.0 - o.0) * (q.1 - o.1) - (p.1 - o.1) * (q.0 - o.0)
    };
    let (d1, d2) = (cross(c, d, a), cross(c, d, b));
    let (d3, d4) = (cross(a, b, c), cross(a, b, d));
    // strictly on opposite sides both ways, with a little tolerance so
    // edges that only touch don't count
    let tolerance = 1e-9;
    ((d1 > tolerance && d2 < -tolerance) || (d1 < -tolerance && d2 > tolerance))
        && ((d3 > tolerance && d4 < -tolerance) || (d3 < -tolerance && d4 > tolerance))
}
