//! working out the label each list paragraph shows ("3.", "b)", "iv.",
//! a bullet), following word's counting rules.

use std::collections::{HashMap, HashSet};

use crate::model::{Block, Document, ListLevel, NumberFormat};

/// the label of every top-level block, `None` for blocks not in a list.
/// lists inside tables count along in document order too.
pub fn list_labels(document: &Document) -> Vec<Option<String>> {
    let mut counter = ListCounter::new(document);
    document
        .blocks
        .iter()
        .map(|block| counter.label_for_block(block))
        .collect()
}

/// counts list items as they come, one document at a time.
pub struct ListCounter<'a> {
    document: &'a Document,
    /// definition id -> current number at each level
    counts: HashMap<String, [Option<u32>; 9]>,
    /// (instance, level) start overrides already applied
    started: HashSet<(String, u8)>,
}

impl<'a> ListCounter<'a> {
    pub fn new(document: &'a Document) -> ListCounter<'a> {
        ListCounter {
            document,
            counts: HashMap::new(),
            started: HashSet::new(),
        }
    }

    pub fn label_for_block(&mut self, block: &Block) -> Option<String> {
        let paragraph = block.paragraph()?;
        let list = crate::styles::resolved_paragraph_props(self.document, paragraph).list?;
        self.next_label(&list.list_id, list.level)
    }

    /// advances the counter for a paragraph in `list_id` at `level` and
    /// returns its label.
    pub fn next_label(&mut self, list_id: &str, level: u8) -> Option<String> {
        if list_id == "0" {
            return None;
        }
        let numbering = &self.document.numbering;
        let instance = numbering.instance(list_id)?;
        let definition = numbering.definition(&instance.definition_id)?;
        let level = level.min(8);
        let counts = self
            .counts
            .entry(definition.id.clone())
            .or_insert([None; 9]);
        let override_start = instance
            .overrides
            .iter()
            .find(|item| item.level == level)
            .and_then(|item| item.start);
        let level_start = |l: u8| -> u32 {
            definition
                .levels
                .iter()
                .find(|item| item.level == l)
                .map(|item| item.start)
                .unwrap_or(1)
        };
        let index = usize::from(level);
        let fresh = self.started.insert((instance.id.clone(), level));
        counts[index] = match (override_start, fresh, counts[index]) {
            (Some(start), true, _) => Some(start),
            (_, _, Some(current)) => Some(current + 1),
            (_, _, None) => Some(level_start(level)),
        };
        // a deeper level starts again whenever a shallower one moves on
        for deeper in counts.iter_mut().skip(index + 1) {
            *deeper = None;
        }
        // levels above that never got a number show their start
        for (upper, count) in counts.iter_mut().enumerate().take(index) {
            if count.is_none() {
                *count = Some(level_start(upper as u8));
            }
        }
        let numbers = *counts;
        let level_def = definition.levels.iter().find(|item| item.level == level)?;
        Some(render_label(level_def, &definition.levels, &numbers))
    }
}

/// fills the level's pattern ("%1.%2.") with the current numbers.
fn render_label(level: &ListLevel, levels: &[ListLevel], numbers: &[Option<u32>; 9]) -> String {
    if level.format == NumberFormat::Bullet {
        return level.text.clone();
    }
    let mut label = String::new();
    let mut chars = level.text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '%' {
            if let Some(digit) = chars.peek().and_then(|d| d.to_digit(10)) {
                chars.next();
                let index = (digit as usize).saturating_sub(1).min(8);
                let format = levels
                    .iter()
                    .find(|item| usize::from(item.level) == index)
                    .map(|item| &item.format)
                    .unwrap_or(&NumberFormat::Decimal);
                label.push_str(&format_number(numbers[index].unwrap_or(1), format));
                continue;
            }
        }
        label.push(c);
    }
    label
}

/// a number in a list format: 3 is "3", "03", "c", "C", "iii" or "III".
pub fn format_number(number: u32, format: &NumberFormat) -> String {
    match format {
        NumberFormat::Decimal | NumberFormat::Other(_) => number.to_string(),
        NumberFormat::DecimalZero => format!("{number:02}"),
        NumberFormat::LowerLetter => letters(number),
        NumberFormat::UpperLetter => letters(number).to_uppercase(),
        NumberFormat::LowerRoman => roman(number),
        NumberFormat::UpperRoman => roman(number).to_uppercase(),
        NumberFormat::Bullet | NumberFormat::None => String::new(),
    }
}

/// word's letter numbering: a..z, then aa..zz, then aaa...
fn letters(number: u32) -> String {
    if number == 0 {
        return String::new();
    }
    let index = (number - 1) % 26;
    let repeat = (number - 1) / 26 + 1;
    let letter = char::from(b'a' + index as u8);
    std::iter::repeat_n(letter, repeat as usize).collect()
}

fn roman(mut number: u32) -> String {
    const TABLE: [(u32, &str); 13] = [
        (1000, "m"),
        (900, "cm"),
        (500, "d"),
        (400, "cd"),
        (100, "c"),
        (90, "xc"),
        (50, "l"),
        (40, "xl"),
        (10, "x"),
        (9, "ix"),
        (5, "v"),
        (4, "iv"),
        (1, "i"),
    ];
    if number == 0 {
        return "0".into();
    }
    let mut out = String::new();
    for (value, text) in TABLE {
        while number >= value {
            out.push_str(text);
            number -= value;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn number_formats() {
        assert_eq!(format_number(4, &NumberFormat::LowerRoman), "iv");
        assert_eq!(format_number(1994, &NumberFormat::UpperRoman), "MCMXCIV");
        assert_eq!(format_number(27, &NumberFormat::LowerLetter), "aa");
        assert_eq!(format_number(3, &NumberFormat::UpperLetter), "C");
        assert_eq!(format_number(7, &NumberFormat::DecimalZero), "07");
    }
}
