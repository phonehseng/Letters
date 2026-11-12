//! graphics state while running content: marked-content sequences,
//! ExtGState opacity, colour spaces and fill and stroke colours.

use super::*;

impl Runner<'_> {
    pub(super) fn begin_marked(&mut self, source: usize, op: usize, operands: &[Object]) {
        let tag = match operands.first() {
            Some(Object::Name(tag)) => tag.clone(),
            _ => Vec::new(),
        };
        self.result.marked.push(MarkedRecord {
            source,
            begin_op: op,
            tag,
        });
        self.open_marked.push(self.result.marked.len() - 1);
    }

    /// applies the parts of an ExtGState that matter here: opacity, line
    /// width and dash.
    pub(super) fn graphics_state(&mut self, source: usize, name: &[u8], state: &mut GraphicsState) {
        let Some(dict) = self
            .resource(source, b"ExtGState", name)
            .and_then(|object| resolve(self.doc, object).as_dict().ok())
        else {
            return;
        };
        let number = |key: &[u8]| dict.get(key).ok().and_then(|value| value.as_float().ok());
        if let Some(alpha) = number(b"ca") {
            state.fill_alpha = alpha;
        }
        if let Some(alpha) = number(b"CA") {
            state.stroke_alpha = alpha;
        }
        if let Some(width) = number(b"LW") {
            state.line.width = width;
        }
    }

    /// the colour after a cs or CS operator: the space's initial colour.
    pub(super) fn color_space(&self, source: usize, operation: &Operation) -> FillColor {
        let name = match operation.operands.first() {
            Some(Object::Name(name)) => name.clone(),
            _ => Vec::new(),
        };
        let family = self.space_family(source, &name);
        // a named space is a pattern space when its array starts /Pattern
        let is_pattern = name == b"Pattern" || family.as_deref() == Some(b"Pattern".as_slice());
        let ink_tint = matches!(family.as_deref(), Some(b"Separation" | b"DeviceN"));
        FillColor {
            color: Color::BLACK,
            space_operator: Some(
                self.result.sources[source].content[operation.span.clone()].to_vec(),
            ),
            color_operator: None,
            source: Some(source),
            space: String::from_utf8_lossy(&name).into_owned(),
            components: Vec::new(),
            pattern: None,
            pattern_space: is_pattern,
            ink_tint,
        }
    }

    /// the kind of a named colour space: the first name of its array.
    fn space_family(&self, source: usize, name: &[u8]) -> Option<Vec<u8>> {
        let space = resolve(self.doc, self.resource(source, b"ColorSpace", name)?);
        let first = space.as_array().ok()?.first()?;
        Some(first.as_name().ok()?.to_vec())
    }
}

pub(super) fn set_fill(
    fill: &mut FillColor,
    source: usize,
    operation: &Operation,
    numbers: &[f32],
    space: Option<Vec<u8>>,
) {
    fill.color = match numbers {
        [tint] if fill.ink_tint => Color::from_fractions(1.0 - tint, 1.0 - tint, 1.0 - tint),
        [grey] => Color::from_fractions(*grey, *grey, *grey),
        [r, g, b] => Color::from_fractions(*r, *g, *b),
        [c, m, y, k] => Color::from_fractions(
            1.0 - (c + k).min(1.0),
            1.0 - (m + k).min(1.0),
            1.0 - (y + k).min(1.0),
        ),
        // a pattern or a colour space Letters doesn't read
        _ => Color::BLACK,
    };
    let device = match operation.operator.as_str() {
        "g" | "G" => Some("DeviceGray"),
        "rg" | "RG" => Some("DeviceRGB"),
        "k" | "K" => Some("DeviceCMYK"),
        _ => None,
    };
    match device {
        Some(device) => {
            fill.space_operator = None;
            fill.space = device.into();
            fill.pattern_space = false;
            fill.ink_tint = false;
        }
        None => fill.space_operator = space,
    }
    fill.components = numbers.to_vec();
    fill.pattern = match operation.operands.last() {
        Some(Object::Name(pattern)) => Some(pattern.clone()),
        _ => None,
    };
    fill.color_operator = Some(operation_bytes_placeholder(operation));
    fill.source = Some(source);
}

/// the operator as text, rebuilt from its operands. (the original bytes
/// are equivalent; rebuilding keeps this independent of the source.)
fn operation_bytes_placeholder(operation: &Operation) -> Vec<u8> {
    crate::writer::operation(&operation.operator, &operation.operands)
}
