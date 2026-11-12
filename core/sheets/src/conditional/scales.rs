//! colour scales and data bars: reading their points and colours from
//! the rule's kept xml, and turning a cell's number into a colour or a
//! bar length the way excel does.
//!
//! a data bar from excel 2010 or later has a second half in the sheet's
//! `extLst` (x14 elements, linked by an id): it says whether the bar is a
//! gradient, its negative colour and where the axis sits. without it the
//! bar is an excel 2007 one: a gradient from 10% to 90% of the cell.

use crate::model::Sheet;
use crate::xlsx::style_parts::color_from_attributes;
use crate::xml::{Event, Fragment, Reader};

use super::statistics::Numbers;

/// one end (or the middle) of a scale or bar, as excel's `<cfvo>` gives it
#[derive(Clone, Debug, PartialEq)]
pub enum Point {
    Min,
    Max,
    Number(f64),
    Percent(f64),
    Percentile(f64),
    /// a formula (or a number written as one), worked out at the range's
    /// top-left cell
    Formula(String),
    /// excel 2010 bars: the smallest value, but never above 0
    AutoMin,
    /// excel 2010 bars: the largest value, but never below 0
    AutoMax,
}

impl Point {
    /// reads a `<cfvo>`; `val` is an attribute, or an `<xm:f>` child in x14
    fn read(xml: &str) -> Option<Point> {
        let cfvo = Fragment::parse(xml)?;
        let value = cfvo.attribute("val").map(str::to_string).or_else(|| {
            cfvo.children
                .iter()
                .find(|(name, _)| name == "f")
                .and_then(|(_, f)| element_text(f))
        });
        let number = || value.as_deref().and_then(|v| v.trim().parse::<f64>().ok());
        Some(match cfvo.attribute("type")? {
            "min" => Point::Min,
            "max" => Point::Max,
            "autoMin" => Point::AutoMin,
            "autoMax" => Point::AutoMax,
            "num" => number()
                .map(Point::Number)
                .unwrap_or(Point::Formula(value?)),
            "percent" => Point::Percent(number()?),
            "percentile" => Point::Percentile(number()?),
            "formula" => Point::Formula(value?),
            _ => return None,
        })
    }

    /// the point as a number, given the range's numbers (none when the
    /// range has no numbers: then nothing is drawn)
    pub fn resolve(
        &self,
        numbers: &mut Numbers,
        formula: &dyn Fn(&str) -> Option<f64>,
    ) -> Option<f64> {
        let (low, high) = (numbers.least()?, numbers.most()?);
        Some(match self {
            Point::Min => low,
            Point::Max => high,
            Point::AutoMin => low.min(0.0),
            Point::AutoMax => high.max(0.0),
            Point::Number(n) => *n,
            Point::Percent(p) => low + (high - low) * p / 100.0,
            Point::Percentile(p) => numbers.percentile(p / 100.0)?,
            Point::Formula(text) => formula(text)?,
        })
    }
}

/// the text inside a small element such as `<xm:f>5</xm:f>`
fn element_text(xml: &str) -> Option<String> {
    let mut reader = Reader::new(xml);
    let tag = reader.root().ok()?;
    if tag.self_closing {
        return Some(String::new());
    }
    reader.element_text(&tag).ok()
}

/// the colour a `<color>` names, as "#RRGGBB"
fn css_colour(xml: &str, theme: &[String]) -> Option<String> {
    let element = Fragment::parse(xml)?;
    color_from_attributes(&element.attributes).to_css(theme)
}

/// the children of the element inside a rule's details, by local name
fn details_element(details: &str, name: &str) -> Option<Fragment> {
    let mut reader = Reader::new(details);
    while let Ok(Some(event)) = reader.next() {
        if let Event::Start(tag) = event {
            if tag.local() == name {
                let raw = reader.skip_element(&tag).ok()?;
                return Fragment::parse(raw);
            }
        }
    }
    None
}

/// a colour scale: two or three points, each with its colour
#[derive(Clone, Debug, PartialEq)]
pub struct ColorScale {
    pub points: Vec<(Point, Rgb)>,
}

impl ColorScale {
    pub fn read(details: &str, theme: &[String]) -> Option<ColorScale> {
        let scale = details_element(details, "colorScale")?;
        let points: Vec<Point> = children_named(&scale, "cfvo")
            .filter_map(Point::read)
            .collect();
        let colours: Vec<Rgb> = children_named(&scale, "color")
            .filter_map(|xml| css_colour(xml, theme))
            .filter_map(|css| Rgb::parse(&css))
            .collect();
        if points.len() < 2 || points.len() != colours.len() {
            return None;
        }
        Some(ColorScale {
            points: points.into_iter().zip(colours).collect(),
        })
    }
}

/// a colour scale with its points worked out for the range
pub struct ResolvedScale {
    stops: Vec<(f64, Rgb)>,
}

impl ResolvedScale {
    pub fn new(
        scale: &ColorScale,
        numbers: &mut Numbers,
        formula: &dyn Fn(&str) -> Option<f64>,
    ) -> Option<ResolvedScale> {
        let stops = scale
            .points
            .iter()
            .map(|(point, colour)| Some((point.resolve(numbers, formula)?, *colour)))
            .collect::<Option<Vec<_>>>()?;
        Some(ResolvedScale { stops })
    }

    /// the colour for a number: the end colours past the ends, and a
    /// straight mix of the two nearest colours in between
    pub fn colour(&self, value: f64) -> Rgb {
        let first = self.stops[0];
        if value <= first.0 {
            return first.1;
        }
        for pair in self.stops.windows(2) {
            let ((low, from), (high, to)) = (pair[0], pair[1]);
            if value <= high {
                if high <= low {
                    return to;
                }
                return from.mix(to, (value - low) / (high - low));
            }
        }
        self.stops[self.stops.len() - 1].1
    }
}

/// a colour as three channels, for mixing
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    pub fn parse(css: &str) -> Option<Rgb> {
        let hex = css.strip_prefix('#')?;
        let channel = |i: usize| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok();
        Some(Rgb(channel(0)?, channel(2)?, channel(4)?))
    }

    pub fn mix(self, other: Rgb, amount: f64) -> Rgb {
        let blend = |a: u8, b: u8| {
            let mixed = f64::from(a) + (f64::from(b) - f64::from(a)) * amount.clamp(0.0, 1.0);
            mixed.round() as u8
        };
        Rgb(
            blend(self.0, other.0),
            blend(self.1, other.1),
            blend(self.2, other.2),
        )
    }

    pub fn css(self) -> String {
        format!("#{:02X}{:02X}{:02X}", self.0, self.1, self.2)
    }
}

fn children_named<'f>(fragment: &'f Fragment, name: &'f str) -> impl Iterator<Item = &'f str> + 'f {
    fragment
        .children
        .iter()
        .filter(move |(child, _)| child == name)
        .map(|(_, xml)| xml.as_str())
}

/// where a data bar's axis sits when the range has negative numbers
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Axis {
    /// excel 2007 bars, and x14 `axisPosition="none"`: every bar grows
    /// from the left, a negative number just gets a shorter one
    None,
    /// in proportion to how far the numbers reach below and above 0
    Automatic,
    Middle,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DataBar {
    pub low: Point,
    pub high: Point,
    pub colour: String,
    pub negative_colour: String,
    /// the shortest and longest bar, in percent of the cell's width
    pub min_length: f64,
    pub max_length: f64,
    pub gradient: bool,
    pub show_value: bool,
    pub axis: Axis,
}

impl DataBar {
    pub fn read(details: &str, sheet: &Sheet, theme: &[String]) -> Option<DataBar> {
        let bar = details_element(details, "dataBar")?;
        let mut points = children_named(&bar, "cfvo").filter_map(Point::read);
        let (low, high) = (points.next()?, points.next()?);
        let colour = children_named(&bar, "color")
            .next()
            .and_then(|xml| css_colour(xml, theme))?;
        let length = |name: &str, default: f64| {
            bar.attribute(name)
                .and_then(|v| v.parse::<f64>().ok())
                .unwrap_or(default)
        };
        let mut read = DataBar {
            low,
            high,
            colour,
            // excel's default colour for negative bars
            negative_colour: "#FF0000".into(),
            min_length: length("minLength", 10.0),
            max_length: length("maxLength", 90.0),
            gradient: true,
            show_value: bar.attribute("showValue") != Some("0"),
            axis: Axis::None,
        };
        if let Some(modern) =
            extension_rule(details, sheet).and_then(|rule| details_element(&rule, "dataBar"))
        {
            read.apply_extension(&modern, theme);
        }
        Some(read)
    }

    /// the x14 half of a bar replaces the 2007 lengths and points
    fn apply_extension(&mut self, bar: &Fragment, theme: &[String]) {
        let number = |name: &str, default: f64| {
            bar.attribute(name)
                .and_then(|v| v.parse::<f64>().ok())
                .unwrap_or(default)
        };
        self.min_length = number("minLength", 0.0);
        self.max_length = number("maxLength", 100.0);
        self.gradient = bar.attribute("gradient") != Some("0");
        self.axis = match bar.attribute("axisPosition") {
            Some("none") => Axis::None,
            Some("middle") => Axis::Middle,
            _ => Axis::Automatic,
        };
        let mut points = children_named(bar, "cfvo").filter_map(Point::read);
        if let (Some(low), Some(high)) = (points.next(), points.next()) {
            self.low = low;
            self.high = high;
        }
        if let Some(colour) = children_named(bar, "fillColor")
            .next()
            .and_then(|xml| css_colour(xml, theme))
        {
            self.colour = colour;
        }
        let same = bar.attribute("negativeBarColorSameAsPositive") == Some("1");
        let negative = children_named(bar, "negativeFillColor")
            .next()
            .and_then(|xml| css_colour(xml, theme));
        if same {
            self.negative_colour = self.colour.clone();
        } else if let Some(negative) = negative {
            self.negative_colour = negative;
        }
    }
}

/// the x14 rule an excel 2010 rule points to through `<x14:id>` in its own
/// `extLst`, found in the sheet's kept `extLst`
fn extension_rule(details: &str, sheet: &Sheet) -> Option<String> {
    let start = details.find("<x14:id>")? + "<x14:id>".len();
    let id = &details[start..start + details[start..].find('<')?];
    for section in sheet.carry.sections.iter().filter(|s| s.name == "extLst") {
        let mut reader = Reader::new(&section.xml);
        while let Ok(Some(event)) = reader.next() {
            let Event::Start(tag) = event else { continue };
            if tag.local() == "cfRule" && tag.attr("id").as_deref() == Some(id) {
                return reader.skip_element(&tag).ok().map(str::to_string);
            }
        }
    }
    None
}

/// how a data bar is drawn in one cell
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DataBarLook {
    /// the bar's length as a share of the cell's width, 0 to 1
    pub fraction: f64,
    /// "#RRGGBB"
    pub color: String,
    /// fades to white at its end, as excel's gradient bars do
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub gradient: bool,
    /// a negative number's bar, drawn leftwards from the axis
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub negative: bool,
    /// where the axis sits as a share of the width; bars then grow from
    /// it, not from the cell's left edge
    #[serde(skip_serializing_if = "Option::is_none")]
    pub axis: Option<f64>,
    /// the rule shows the bar only, without the cell's text
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub hide_value: bool,
}

/// a data bar with its ends worked out for the range
pub struct ResolvedBar {
    bar: DataBar,
    low: f64,
    high: f64,
    /// where the axis sits, when bars grow from one
    axis: Option<f64>,
}

impl ResolvedBar {
    pub fn new(
        bar: DataBar,
        numbers: &mut Numbers,
        formula: &dyn Fn(&str) -> Option<f64>,
    ) -> Option<ResolvedBar> {
        let low = bar.low.resolve(numbers, formula)?;
        let high = bar.high.resolve(numbers, formula)?.max(low);
        let axis = match bar.axis {
            Axis::None => None,
            Axis::Middle => Some(0.5),
            // the axis sits as far in as the negative part of the range
            // reaches; with no negative numbers there is none
            Axis::Automatic if low < 0.0 && high > 0.0 => Some(-low / (high - low)),
            Axis::Automatic if high <= 0.0 && low < 0.0 => Some(1.0),
            Axis::Automatic => None,
        };
        Some(ResolvedBar {
            bar,
            low,
            high,
            axis,
        })
    }

    pub fn look(&self, value: f64) -> DataBarLook {
        let bar = &self.bar;
        let (fraction, negative) = match self.axis {
            Some(axis) => self.growing_from_axis(value, axis),
            None => {
                // a straight line from the shortest bar at the low point to
                // the longest at the high point
                let share = if self.high > self.low {
                    ((value - self.low) / (self.high - self.low)).clamp(0.0, 1.0)
                } else {
                    1.0
                };
                let percent = bar.min_length + share * (bar.max_length - bar.min_length);
                (percent / 100.0, false)
            }
        };
        DataBarLook {
            fraction: (fraction.clamp(0.0, 1.0) * 1000.0).round() / 1000.0,
            color: if negative {
                bar.negative_colour.clone()
            } else {
                bar.colour.clone()
            },
            gradient: bar.gradient,
            negative,
            axis: self.axis,
            hide_value: !bar.show_value,
        }
    }

    /// a bar growing from the axis: right for positive numbers, left for
    /// negative ones, each side scaled to its own end of the range
    fn growing_from_axis(&self, value: f64, axis: f64) -> (f64, bool) {
        let reach = |end: f64| {
            if self.bar.axis == Axis::Middle {
                // both sides share one scale, so equal sizes look equal
                self.low.abs().max(self.high.abs())
            } else {
                end.abs()
            }
        };
        if value < 0.0 {
            let room = axis;
            let scale = reach(self.low);
            let share = if scale > 0.0 {
                (-value / scale).min(1.0)
            } else {
                0.0
            };
            (share * room, true)
        } else {
            let room = 1.0 - axis;
            let scale = reach(self.high);
            let share = if scale > 0.0 {
                (value / scale).min(1.0)
            } else {
                0.0
            };
            (share * room, false)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::style::default_theme_colors;

    fn no_formulas(_: &str) -> Option<f64> {
        None
    }

    #[test]
    fn two_colour_scales_mix_between_their_ends() {
        let theme = default_theme_colors();
        let scale = ColorScale::read(
            r#"<colorScale><cfvo type="min"/><cfvo type="max"/><color rgb="FFF8696B"/><color rgb="FF63BE7B"/></colorScale>"#,
            &theme,
        )
        .unwrap();
        let mut numbers = Numbers::new(vec![0.0, 50.0, 100.0]);
        let resolved = ResolvedScale::new(&scale, &mut numbers, &no_formulas).unwrap();
        assert_eq!(resolved.colour(0.0).css(), "#F8696B");
        assert_eq!(resolved.colour(100.0).css(), "#63BE7B");
        // halfway: each channel halfway, rounded ((248+99)/2 = 173.5)
        assert_eq!(resolved.colour(50.0).css(), "#AE9473");
        assert_eq!(resolved.colour(-5.0).css(), "#F8696B");
    }

    #[test]
    fn three_colour_scales_use_their_midpoint() {
        let theme = default_theme_colors();
        let scale = ColorScale::read(
            r#"<colorScale><cfvo type="num" val="0"/><cfvo type="percentile" val="50"/><cfvo type="percent" val="100"/><color rgb="FFFF0000"/><color rgb="FFFFFFFF"/><color rgb="FF0000FF"/></colorScale>"#,
            &theme,
        )
        .unwrap();
        let mut numbers = Numbers::new(vec![0.0, 10.0, 20.0, 100.0]);
        let resolved = ResolvedScale::new(&scale, &mut numbers, &no_formulas).unwrap();
        // the 50th percentile of 0, 10, 20, 100 is 15
        assert_eq!(resolved.colour(15.0).css(), "#FFFFFF");
        assert_eq!(resolved.colour(7.5).css(), "#FF8080");
        assert_eq!(resolved.colour(100.0).css(), "#0000FF");
    }

    #[test]
    fn excel_2007_bars_run_from_ten_to_ninety_percent() {
        let theme = default_theme_colors();
        let sheet = Sheet::new("S");
        let bar = DataBar::read(
            r#"<dataBar><cfvo type="min"/><cfvo type="max"/><color rgb="FF638EC6"/></dataBar>"#,
            &sheet,
            &theme,
        )
        .unwrap();
        let mut numbers = Numbers::new(vec![10.0, 20.0, 30.0]);
        let resolved = ResolvedBar::new(bar.clone(), &mut numbers, &no_formulas).unwrap();
        assert_eq!(resolved.look(10.0).fraction, 0.1);
        assert_eq!(resolved.look(20.0).fraction, 0.5);
        let longest = resolved.look(30.0);
        assert_eq!(longest.fraction, 0.9);
        assert_eq!(longest.color, "#638EC6");
        assert!(longest.gradient);
        assert_eq!(longest.axis, None);
    }

    #[test]
    fn excel_2010_bars_grow_from_an_axis() {
        let theme = default_theme_colors();
        let mut sheet = Sheet::new("S");
        sheet.carry.sections.push(crate::model::Section {
            name: "extLst".into(),
            xml: r#"<extLst><ext uri="{78C0D931-6437-407d-A8EE-F0AAD7539E65}"><x14:conditionalFormattings><x14:conditionalFormatting><x14:cfRule type="dataBar" id="{AB}"><x14:dataBar minLength="0" maxLength="100" gradient="0"><x14:cfvo type="autoMin"/><x14:cfvo type="autoMax"/><x14:negativeFillColor rgb="FFFF0000"/></x14:dataBar></x14:cfRule></x14:conditionalFormatting></x14:conditionalFormattings></ext></extLst>"#.into(),
            generated: None,
        });
        let bar = DataBar::read(
            r#"<dataBar><cfvo type="min"/><cfvo type="max"/><color rgb="FF638EC6"/></dataBar><extLst><ext uri="{B025F937-C7B1-47D3-B67F-A62EFF666E3E}"><x14:id>{AB}</x14:id></ext></extLst>"#,
            &sheet,
            &theme,
        )
        .unwrap();
        assert!(!bar.gradient);
        let mut numbers = Numbers::new(vec![-25.0, 0.0, 75.0]);
        let resolved = ResolvedBar::new(bar.clone(), &mut numbers, &no_formulas).unwrap();
        let positive = resolved.look(75.0);
        assert_eq!(positive.axis, Some(0.25));
        assert_eq!(positive.fraction, 0.75);
        let negative = resolved.look(-25.0);
        assert!(negative.negative);
        assert_eq!(negative.fraction, 0.25);
        assert_eq!(negative.color, "#FF0000");
    }
}
