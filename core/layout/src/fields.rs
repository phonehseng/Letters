//! what fields in the text show when the page is drawn.
//!
//! page numbers and page counts come from the layout itself (see
//! `engine::place_body_with`); a DATE field shows the day the document is
//! laid out, as word and libreoffice both show it whenever a document is
//! opened. its picture switch (`\@ "d MMMM yyyy"`) says how; without one
//! the result keeps word's us english short date (M/d/yyyy).
//!
//! the date is the coordinated universal time day: the standard library
//! does not know the computer's time zone, so around midnight the shown
//! day can be one off the local one.

use std::time::{SystemTime, UNIX_EPOCH};

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

const DAYS: [&str; 7] = [
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
];

/// a calendar day.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Day {
    pub year: i64,
    pub month: u32,
    pub day: u32,
    /// days since 1 january 1970 (a thursday)
    pub since_epoch: i64,
}

impl Day {
    /// today, in coordinated universal time.
    pub fn today() -> Day {
        let seconds = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        Day::from_epoch_days(seconds.div_euclid(86_400))
    }

    /// the civil date of a day count (the proleptic gregorian calendar,
    /// howard hinnant's days-to-civil method).
    pub fn from_epoch_days(days: i64) -> Day {
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let day_of_era = z - era * 146_097;
        let year_of_era =
            (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
        let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
        let shifted_month = (5 * day_of_year + 2) / 153;
        let day = (day_of_year - (153 * shifted_month + 2) / 5 + 1) as u32;
        let month = if shifted_month < 10 {
            shifted_month + 3
        } else {
            shifted_month - 9
        } as u32;
        let year = year_of_era + era * 400 + i64::from(month <= 2);
        Day {
            year,
            month,
            day,
            since_epoch: days,
        }
    }

    fn weekday(&self) -> &'static str {
        DAYS[self.since_epoch.rem_euclid(7) as usize]
    }
}

/// the picture of a field instruction: the text after `\@`, unquoted.
fn picture_of(instruction: &str) -> Option<String> {
    let at = instruction.find("\\@")?;
    let rest = instruction[at + 2..].trim_start();
    if let Some(quoted) = rest.strip_prefix('"') {
        return quoted.split('"').next().map(str::to_string);
    }
    rest.split_whitespace().next().map(str::to_string)
}

/// a date shown by a field's instruction (`DATE \@ "MMMM d, yyyy"`).
pub fn date_text(instruction: &str, day: Day) -> String {
    let picture = picture_of(instruction).unwrap_or_else(|| "M/d/yyyy".to_string());
    let mut out = String::new();
    let chars: Vec<char> = picture.chars().collect();
    let mut index = 0;
    while index < chars.len() {
        let ch = chars[index];
        let mut run = 1;
        while index + run < chars.len() && chars[index + run] == ch {
            run += 1;
        }
        match (ch, run) {
            ('y', 2) => out.push_str(&format!("{:02}", day.year.rem_euclid(100))),
            ('y', _) => out.push_str(&day.year.to_string()),
            ('M', 1) => out.push_str(&day.month.to_string()),
            ('M', 2) => out.push_str(&format!("{:02}", day.month)),
            ('M', 3) => out.push_str(&MONTHS[day.month as usize - 1][..3]),
            ('M', _) => out.push_str(MONTHS[day.month as usize - 1]),
            ('d', 1) => out.push_str(&day.day.to_string()),
            ('d', 2) => out.push_str(&format!("{:02}", day.day)),
            ('d', 3) => out.push_str(&day.weekday()[..3]),
            ('d', _) => out.push_str(day.weekday()),
            ('\'', _) => {
                // quoted literal text
                let end = chars[index + 1..]
                    .iter()
                    .position(|c| *c == '\'')
                    .map_or(chars.len(), |p| index + 1 + p);
                out.extend(&chars[index + 1..end]);
                index = end + 1;
                continue;
            }
            _ => out.extend(std::iter::repeat_n(ch, run)),
        }
        index += run;
    }
    out
}

/// a page number or page count field.
pub fn is_page_field(inline: &letters_office::model::Inline) -> bool {
    use letters_office::model::{FieldKind, Inline};
    matches!(
        inline,
        Inline::Field(field) if matches!(field.kind, FieldKind::PageNumber | FieldKind::PageCount)
    )
}

/// a page number or page count field in body text makes a block depend
/// on the page it lands on (and on the page count).
pub fn has_body_page_fields(document: &letters_office::model::Document) -> bool {
    use letters_office::model::Block;
    fn any(blocks: &[Block]) -> bool {
        blocks.iter().any(|block| match block {
            Block::Paragraph(_) | Block::Heading(_) => block
                .paragraph()
                .is_some_and(|p| p.content.iter().any(is_page_field)),
            Block::Table(table) => table
                .rows
                .iter()
                .any(|row| row.cells.iter().any(|cell| any(&cell.blocks))),
            Block::TextBox(text_box) => any(&text_box.blocks),
            _ => false,
        })
    }
    any(&document.blocks)
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn day_counts_become_dates() {
        assert_eq!(
            Day::from_epoch_days(0),
            Day {
                year: 1970,
                month: 1,
                day: 1,
                since_epoch: 0
            }
        );
        // 12 november 2026 is day 20,769 and a thursday
        let day = Day::from_epoch_days(20_769);
        assert_eq!((day.year, day.month, day.day), (2026, 11, 12));
        assert_eq!(day.weekday(), "Thursday");
        // a leap day
        let leap = Day::from_epoch_days(19_782);
        assert_eq!((leap.year, leap.month, leap.day), (2024, 2, 29));
    }

    #[test]
    fn pictures_shape_the_date() {
        let day = Day::from_epoch_days(20_769);
        assert_eq!(date_text("DATE", day), "11/12/2026");
        assert_eq!(
            date_text("DATE \\@ \"MMMM d, yyyy\"", day),
            "November 12, 2026"
        );
        assert_eq!(
            date_text("DATE \\@ \"d MMMM yyyy\"", day),
            "12 November 2026"
        );
        assert_eq!(date_text("DATE \\@ \"yyyy-MM-dd\"", day), "2026-11-12");
        assert_eq!(
            date_text("DATE \\@ \"dddd, MMM d\"", day),
            "Thursday, Nov 12"
        );
        assert_eq!(date_text("DATE \\@ \"d 'of' MMMM\"", day), "12 of November");
    }
}
