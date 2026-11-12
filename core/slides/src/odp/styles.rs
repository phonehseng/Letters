//! opendocument styles: every `style:style` of content.xml and
//! styles.xml with its parent, resolved into one map of properties per
//! kind (graphic, paragraph, text), plus list styles and the named
//! gradients and pictures fills refer to.

use std::collections::HashMap;

use crate::xml::Element;

/// one kind of properties, attribute name to value.
pub type Properties = Vec<(String, String)>;

#[derive(Clone, Debug, Default)]
pub struct Style {
    pub parent: Option<String>,
    pub graphic: Properties,
    pub paragraph: Properties,
    pub text: Properties,
    /// `style:drawing-page-properties` (backgrounds, transitions)
    pub page: Properties,
}

/// one level of a list style.
#[derive(Clone, Debug, Default)]
pub struct ListLevel {
    /// the bullet character, or `None` for numbering
    pub bullet: Option<String>,
    /// "1", "a", "A", "i", "I" for numbering
    pub number_format: Option<String>,
    pub number_suffix: Option<String>,
    pub bullet_font: Option<String>,
    /// share of the text size, from `fo:font-size="45%"`
    pub bullet_size: Option<i32>,
}

#[derive(Clone, Debug, Default)]
pub struct StyleSheet {
    /// by (family, name)
    pub styles: HashMap<(String, String), Style>,
    pub defaults: HashMap<String, Style>,
    pub lists: HashMap<String, Vec<ListLevel>>,
    /// `draw:gradient` by name: (start colour, end colour, angle in degrees, style)
    pub gradients: HashMap<String, (String, String, f64, String)>,
    /// `draw:fill-image` by name: the picture's path
    pub images: HashMap<String, String>,
}

fn attributes(element: Option<&Element>) -> Properties {
    element
        .map(|e| {
            e.attributes
                .iter()
                .filter(|(k, _)| !k.starts_with("xmlns"))
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}

impl StyleSheet {
    /// adds every style found under `root` (a whole content.xml or styles.xml).
    pub fn collect(&mut self, root: &Element) {
        root.walk(&mut |element| match element.name.as_str() {
            "style:style" => self.add_style(element),
            "style:default-style" => {
                let family = element.attr("style:family").unwrap_or("").to_string();
                self.defaults.insert(family, style_of(element));
            }
            "text:list-style" => {
                if let Some(name) = element.attr("style:name") {
                    self.lists.insert(name.to_string(), list_levels(element));
                }
            }
            "draw:gradient" => self.add_gradient(element),
            "draw:fill-image" => {
                if let (Some(name), Some(href)) =
                    (element.attr("draw:name"), element.attr("xlink:href"))
                {
                    self.images.insert(name.to_string(), href.to_string());
                }
            }
            _ => {}
        });
    }

    fn add_style(&mut self, element: &Element) {
        let (Some(name), Some(family)) = (element.attr("style:name"), element.attr("style:family"))
        else {
            return;
        };
        self.styles
            .insert((family.to_string(), name.to_string()), style_of(element));
    }

    fn add_gradient(&mut self, element: &Element) {
        let Some(name) = element.attr("draw:name") else {
            return;
        };
        let angle = element.attr("draw:angle").map(parse_angle).unwrap_or(0.0);
        self.gradients.insert(
            name.to_string(),
            (
                element
                    .attr("draw:start-color")
                    .unwrap_or("#000000")
                    .to_string(),
                element
                    .attr("draw:end-color")
                    .unwrap_or("#ffffff")
                    .to_string(),
                angle,
                element.attr("draw:style").unwrap_or("linear").to_string(),
            ),
        );
    }

    /// the merged properties of a style and its parents (and the family's
    /// default style), nearest last so it wins.
    pub fn resolve(&self, family: &str, name: Option<&str>) -> Style {
        let mut chain = Vec::new();
        let mut current = name.map(str::to_string);
        // a style naming itself as an ancestor must not loop
        while let Some(name) = current.take() {
            if chain.len() > 32 {
                break;
            }
            match self.styles.get(&(family.to_string(), name)) {
                Some(style) => {
                    chain.push(style);
                    current = style.parent.clone();
                }
                None => break,
            }
        }
        let mut merged = self.defaults.get(family).cloned().unwrap_or_default();
        for style in chain.into_iter().rev() {
            merge(&mut merged.graphic, &style.graphic);
            merge(&mut merged.paragraph, &style.paragraph);
            merge(&mut merged.text, &style.text);
            merge(&mut merged.page, &style.page);
        }
        merged
    }
}

fn merge(into: &mut Properties, from: &Properties) {
    for (key, value) in from {
        match into.iter_mut().find(|(k, _)| k == key) {
            Some(entry) => entry.1 = value.clone(),
            None => into.push((key.clone(), value.clone())),
        }
    }
}

fn style_of(element: &Element) -> Style {
    // table column, row and cell properties are kept with the graphic
    // ones: no style has both, and readers look them up by name
    let mut graphic = attributes(element.child("style:graphic-properties"));
    for name in [
        "style:table-column-properties",
        "style:table-row-properties",
        "style:table-cell-properties",
    ] {
        graphic.extend(attributes(element.child(name)));
    }
    Style {
        parent: element.attr("style:parent-style-name").map(str::to_string),
        graphic,
        paragraph: attributes(element.child("style:paragraph-properties")),
        text: attributes(element.child("style:text-properties")),
        page: attributes(element.child("style:drawing-page-properties")),
    }
}

fn list_levels(element: &Element) -> Vec<ListLevel> {
    let mut levels = vec![ListLevel::default(); 10];
    for level in element.elements() {
        let index: usize = level
            .attr("text:level")
            .and_then(|l| l.parse().ok())
            .unwrap_or(1);
        let Some(slot) = levels.get_mut(index.saturating_sub(1)) else {
            continue;
        };
        let text = level.child("style:text-properties");
        slot.bullet_font = text
            .and_then(|t| t.attr("fo:font-family"))
            .map(|f| f.trim_matches('\'').to_string());
        slot.bullet_size = text
            .and_then(|t| t.attr("fo:font-size"))
            .and_then(super::units::percent);
        match level.name.as_str() {
            "text:list-level-style-bullet" => {
                slot.bullet = level.attr("text:bullet-char").map(str::to_string);
            }
            "text:list-level-style-number" => {
                slot.number_format = level.attr("style:num-format").map(str::to_string);
                slot.number_suffix = level.attr("style:num-suffix").map(str::to_string);
            }
            _ => {}
        }
    }
    levels
}

/// "45deg", "450" (tenths of a degree in odf 1.1), "1rad" in degrees.
fn parse_angle(text: &str) -> f64 {
    if let Some(degrees) = text.strip_suffix("deg") {
        return degrees.parse().unwrap_or(0.0);
    }
    if let Some(radians) = text.strip_suffix("rad") {
        return radians.parse::<f64>().unwrap_or(0.0).to_degrees();
    }
    text.parse::<f64>().unwrap_or(0.0) / 10.0
}

/// a property's value from a property list.
pub fn get<'a>(properties: &'a Properties, name: &str) -> Option<&'a str> {
    properties
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.as_str())
}
