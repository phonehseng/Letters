//! inheritance resolved: where a placeholder sits when the slide does not
//! say, and the effective paragraph and run formatting of any text, so
//! the interface can show effective values and the renderer can draw.
//!
//! the chain for placeholder text, lowest first (ecma-376 part 1,
//! 19.3.1.51 and 21.1.2.4.13): the master's text style for its kind
//! (title, body or other), the master placeholder's list style, the
//! layout placeholder's list style, the shape's own list style, then the
//! paragraph. text that is not in a placeholder starts from the deck's
//! default text style instead.

use crate::model::{
    BodyProps, Layout, Master, Object, ParagraphProps, Placeholder, PlaceholderKind, Presentation,
    RunProps, Shape, Slide, TextBody, TextParagraph, TextStyle, Theme, Transform,
};

/// where an object lives: a slide (with its layout and master), a layout
/// (with its master) or a master.
#[derive(Clone, Copy)]
pub struct Chain<'a> {
    pub layout: Option<&'a Layout>,
    pub master: Option<&'a Master>,
    pub theme: Option<&'a Theme>,
    pub presentation: &'a Presentation,
}

impl<'a> Chain<'a> {
    pub fn of_slide(presentation: &'a Presentation, slide: &Slide) -> Chain<'a> {
        let (layout, master) = presentation.chain_of(slide);
        Chain {
            layout,
            master,
            theme: presentation.theme_of(master),
            presentation,
        }
    }

    pub fn of_layout(presentation: &'a Presentation, layout: &'a Layout) -> Chain<'a> {
        let master = presentation.master(&layout.master_id);
        Chain {
            layout: None,
            master,
            theme: presentation.theme_of(master),
            presentation,
        }
    }

    pub fn of_master(presentation: &'a Presentation, master: &'a Master) -> Chain<'a> {
        Chain {
            layout: None,
            master: None,
            theme: presentation.theme_of(Some(master)),
            presentation,
        }
    }
}

/// the kind of master placeholder a placeholder kind inherits from:
/// titles from the title, text and content of any kind from the body.
fn master_kind(kind: PlaceholderKind) -> PlaceholderKind {
    match kind {
        PlaceholderKind::Title | PlaceholderKind::CenteredTitle => PlaceholderKind::Title,
        PlaceholderKind::Date
        | PlaceholderKind::Footer
        | PlaceholderKind::Header
        | PlaceholderKind::SlideNumber
        | PlaceholderKind::SlideImage => kind,
        _ => PlaceholderKind::Body,
    }
}

/// the same-kind test layouts use: a centred title matches a title, a
/// subtitle or content placeholder matches a body.
fn kinds_match(own: PlaceholderKind, other: PlaceholderKind) -> bool {
    own == other || master_kind(own) == master_kind(other)
}

fn shape_placeholder(object: &Object) -> Option<(&Placeholder, Option<&Shape>)> {
    match object {
        Object::Shape(shape) => shape.placeholder.as_ref().map(|p| (p, Some(shape))),
        _ => object.placeholder().map(|p| (p, None)),
    }
}

/// the layout's placeholder a slide placeholder inherits from: the same
/// index when it has one (powerpoint matches by `idx` first), otherwise
/// the first of the same kind.
pub fn find_in_layout<'a>(layout: &'a Layout, placeholder: &Placeholder) -> Option<&'a Object> {
    let by_index = placeholder.index.and_then(|index| {
        layout
            .objects
            .iter()
            .find(|o| o.placeholder().is_some_and(|p| p.index == Some(index)))
    });
    by_index.or_else(|| {
        layout
            .objects
            .iter()
            .find(|o| o.placeholder().is_some_and(|p| p.kind == placeholder.kind))
            .or_else(|| {
                layout.objects.iter().find(|o| {
                    o.placeholder()
                        .is_some_and(|p| kinds_match(p.kind, placeholder.kind))
                })
            })
    })
}

/// the master's placeholder of the kind a placeholder inherits from.
pub fn find_in_master<'a>(master: &'a Master, placeholder: &Placeholder) -> Option<&'a Object> {
    let wanted = master_kind(placeholder.kind);
    master.objects.iter().find(|o| {
        o.placeholder()
            .is_some_and(|p| master_kind(p.kind) == wanted)
    })
}

/// the layout and master placeholders behind an object, nearest first.
pub fn ancestors<'a>(chain: &Chain<'a>, object: &Object) -> Vec<&'a Object> {
    let Some((placeholder, _)) = shape_placeholder(object) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    if let Some(parent) = chain.layout.and_then(|l| find_in_layout(l, placeholder)) {
        found.push(parent);
    }
    if let Some(parent) = chain.master.and_then(|m| find_in_master(m, placeholder)) {
        found.push(parent);
    }
    found
}

/// where an object is drawn: its own transform, or its placeholder's.
pub fn effective_transform(chain: &Chain<'_>, object: &Object) -> Option<Transform> {
    if let Some(own) = object.transform() {
        return Some(own.clone());
    }
    ancestors(chain, object)
        .into_iter()
        .find_map(|parent| parent.transform().cloned())
}

/// which master text style a placeholder (or plain text) starts from.
fn base_style<'a>(chain: &Chain<'a>, placeholder: Option<&Placeholder>) -> &'a TextStyle {
    let Some(placeholder) = placeholder else {
        return &chain.presentation.default_text_style;
    };
    let Some(master) = chain.master else {
        return &chain.presentation.default_text_style;
    };
    match master_kind(placeholder.kind) {
        PlaceholderKind::Title => &master.title_style,
        PlaceholderKind::Body => &master.body_style,
        _ => &master.other_style,
    }
}

fn level_props(style: &TextStyle, level: u8) -> ParagraphProps {
    let level_style = style
        .level(level)
        .map(|l| l.paragraph.clone())
        .unwrap_or_default();
    match &style.default {
        Some(default) => level_style.over(&default.paragraph),
        None => level_style,
    }
}

/// the effective paragraph properties (with run defaults in `run`) of a
/// paragraph of an object's text, before the paragraph's own runs.
/// `body` is the text the paragraph is in: the object's own text, or a
/// table cell's.
pub fn effective_paragraph(
    chain: &Chain<'_>,
    object: &Object,
    body: &TextBody,
    paragraph: &TextParagraph,
) -> ParagraphProps {
    let level = paragraph.level();
    let placeholder = shape_placeholder(object).map(|(p, _)| p);
    let mut effective = level_props(base_style(chain, placeholder), level);
    for parent in ancestors(chain, object).into_iter().rev() {
        if let Some(text) = parent.text() {
            effective = level_props(&text.list_style, level).over(&effective);
        }
    }
    effective = level_props(&body.list_style, level).over(&effective);
    let mut own = paragraph.props.clone();
    own.level = Some(level);
    own.over(&effective)
}

/// a run's effective properties inside a paragraph whose effective
/// properties are `paragraph`.
pub fn effective_run(paragraph: &ParagraphProps, run: &RunProps) -> RunProps {
    match &paragraph.run {
        Some(defaults) => run.over(defaults),
        None => run.clone(),
    }
}

/// the effective body properties: the shape's own over its placeholders'.
pub fn effective_body(chain: &Chain<'_>, object: &Object, body: &TextBody) -> BodyProps {
    let mut effective = BodyProps::default();
    for parent in ancestors(chain, object).into_iter().rev() {
        if let Some(text) = parent.text() {
            effective = text.body.over(&effective);
        }
    }
    body.body.over(&effective)
}

/// a theme font reference ("+mj-lt", "+mn-ea"...) turned into a family.
pub fn resolve_typeface(theme: Option<&Theme>, typeface: &str) -> String {
    let Some(theme) = theme else {
        return typeface.to_string();
    };
    let fonts = &theme.fonts;
    match typeface {
        "+mj-lt" => fonts.major_latin.clone(),
        "+mn-lt" => fonts.minor_latin.clone(),
        "+mj-ea" => fonts.major_east_asian.clone(),
        "+mn-ea" => fonts.minor_east_asian.clone(),
        "+mj-cs" => fonts.major_complex.clone(),
        "+mn-cs" => fonts.minor_complex.clone(),
        other => other.to_string(),
    }
}

impl BodyProps {
    /// `self` with every property `self` leaves open taken from `parent`.
    pub fn over(&self, parent: &BodyProps) -> BodyProps {
        BodyProps {
            inset_left: self.inset_left.or(parent.inset_left),
            inset_top: self.inset_top.or(parent.inset_top),
            inset_right: self.inset_right.or(parent.inset_right),
            inset_bottom: self.inset_bottom.or(parent.inset_bottom),
            anchor: self.anchor.or(parent.anchor),
            anchor_center: self.anchor_center.or(parent.anchor_center),
            wrap: self.wrap.or(parent.wrap),
            autofit: self.autofit.clone().or_else(|| parent.autofit.clone()),
            columns: self.columns.or(parent.columns),
            column_spacing: self.column_spacing.or(parent.column_spacing),
            vertical: self.vertical.clone().or_else(|| parent.vertical.clone()),
            rotation: self.rotation.or(parent.rotation),
            extra: self.extra.clone(),
        }
    }
}
