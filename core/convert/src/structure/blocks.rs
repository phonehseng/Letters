//! turning a page's fragments, tables and pictures into blocks:
//! paragraphs, headings, lists, footnotes and captioned figures, in
//! reading order.

use super::fragments::Fragment;
use super::markers::{list_marker, Marker};
use super::model::{runs_text, Block, BlockContent, Figure, ListItem, TextRun};
use super::reading_order::{reading_order, Item};
use super::tables::FoundTable;
use crate::content::RuleLine;
use crate::geometry::{union_all, Rect};

/// a picture on the page, as found by letters-edit.
#[derive(Clone, Debug)]
pub(crate) struct PagePicture {
    pub rect: Rect,
    pub image_id: String,
    pub pixel_width: u32,
    pub pixel_height: u32,
}

/// what a page holds, ready to be turned into blocks.
pub(crate) struct PageParts<'a> {
    pub page: usize,
    pub height: f32,
    pub width: f32,
    /// body text only: headers, footers and table text already taken out
    pub fragments: Vec<&'a Fragment>,
    pub tables: Vec<FoundTable>,
    pub pictures: Vec<PagePicture>,
    pub rules: &'a [RuleLine],
    pub body_size: f32,
    pub dehyphenate: bool,
}

/// lines that read as one unit, before deciding what kind of block it is.
struct LineGroup<'a> {
    lines: Vec<&'a Fragment>,
    /// for a list item: where its text starts after the marker
    marker: Option<Marker>,
    text_start: f32,
}

impl LineGroup<'_> {
    fn rect(&self) -> Rect {
        union_all(self.lines.iter().map(|l| &l.rect)).unwrap_or_default()
    }

    fn size(&self) -> f32 {
        self.lines.iter().map(|l| l.size).fold(0.0, f32::max)
    }

    fn char_count(&self) -> usize {
        self.lines.iter().map(|l| l.char_count()).sum()
    }
}

/// one entry of the page in reading order.
enum Entry<'a> {
    Text(LineGroup<'a>),
    Table(FoundTable),
    Picture(PagePicture),
}

/// the page's blocks in reading order, and how many columns it has.
/// headings come back with level 0; the document gives them levels
/// once every page is read (see `assign_heading_levels`).
pub(crate) fn page_blocks(parts: PageParts) -> (Vec<Block>, usize) {
    let mut items: Vec<Item> = parts
        .fragments
        .iter()
        .map(|f| Item {
            rect: f.rect,
            is_text: !f.rotated,
        })
        .collect();
    items.extend(parts.tables.iter().map(|t| Item {
        rect: t.rect,
        is_text: false,
    }));
    items.extend(parts.pictures.iter().map(|p| Item {
        rect: p.rect,
        is_text: false,
    }));
    let order = reading_order(&items);
    let pitch = typical_pitch(&parts.fragments, &order.placed);

    let fragment_count = parts.fragments.len();
    let table_count = parts.tables.len();
    let mut tables: Vec<Option<FoundTable>> = parts.tables.into_iter().map(Some).collect();
    let mut pictures: Vec<Option<PagePicture>> = parts.pictures.into_iter().map(Some).collect();
    let mut entries: Vec<Entry> = Vec::new();
    let mut group: Option<(LineGroup, (usize, usize))> = None;
    for placed in &order.placed {
        if placed.item < fragment_count {
            let line = parts.fragments[placed.item];
            let continues = group.as_ref().is_some_and(|(g, region)| {
                *region == placed.region && continues_group(g, line, pitch)
            });
            if continues {
                group.as_mut().expect("checked").0.lines.push(line);
            } else {
                if let Some((done, _)) = group.take() {
                    entries.push(Entry::Text(done));
                }
                group = Some((start_group(line), placed.region));
            }
            continue;
        }
        if let Some((done, _)) = group.take() {
            entries.push(Entry::Text(done));
        }
        let other = placed.item - fragment_count;
        if other < table_count {
            if let Some(table) = tables[other].take() {
                entries.push(Entry::Table(table));
            }
        } else if let Some(picture) = pictures[other - table_count].take() {
            entries.push(Entry::Picture(picture));
        }
    }
    if let Some((done, _)) = group.take() {
        entries.push(Entry::Text(done));
    }
    let context = Context {
        page: parts.page,
        height: parts.height,
        width: parts.width,
        rules: parts.rules,
        body_size: parts.body_size,
        dehyphenate: parts.dehyphenate,
    };
    let blocks = to_blocks(entries, &context);
    (attach_captions(blocks, parts.body_size), order.columns)
}

struct Context<'a> {
    page: usize,
    height: f32,
    width: f32,
    rules: &'a [RuleLine],
    body_size: f32,
    dehyphenate: bool,
}

fn start_group(line: &Fragment) -> LineGroup<'_> {
    let marker = list_marker(&line.text());
    let text_start = match &marker {
        Some(marker) => text_start_after_marker(line, marker),
        None => line.rect.x,
    };
    LineGroup {
        lines: vec![line],
        marker,
        text_start,
    }
}

/// where the words of a list item start, after its marker and the space.
fn text_start_after_marker(line: &Fragment, marker: &Marker) -> f32 {
    let marker_chars = marker.text.chars().count();
    line.glyphs
        .iter()
        .filter(|g| !g.ch.is_whitespace())
        .nth(marker_chars)
        .map(|g| g.rect.x)
        .unwrap_or(line.rect.x)
}

/// the usual distance between baselines of lines in one paragraph,
/// from the lines read one after another in the same column.
fn typical_pitch(fragments: &[&Fragment], placed: &[super::reading_order::Placed]) -> Option<f32> {
    let mut counts: Vec<(i32, usize)> = Vec::new();
    for pair in placed.windows(2) {
        let (Some(a), Some(b)) = (fragments.get(pair[0].item), fragments.get(pair[1].item)) else {
            continue;
        };
        if pair[0].region != pair[1].region || (a.size - b.size).abs() > a.size * 0.1 {
            continue;
        }
        let pitch = b.baseline - a.baseline;
        if pitch > a.size * 0.8 && pitch < a.size * 3.0 {
            let key = (pitch * 2.0).round() as i32;
            match counts.iter_mut().find(|(k, _)| *k == key) {
                Some((_, count)) => *count += 1,
                None => counts.push((key, 1)),
            }
        }
    }
    // the smallest of the most common pitches: paragraph gaps are larger
    counts
        .iter()
        .max_by(|a, b| a.1.cmp(&b.1).then(b.0.cmp(&a.0)))
        .map(|(key, _)| *key as f32 / 2.0)
}

/// whether `line` carries on the paragraph (or list item) in `group`.
fn continues_group(group: &LineGroup, line: &Fragment, pitch: Option<f32>) -> bool {
    let previous = group.lines.last().expect("a group has a line");
    let size = previous.size.max(line.size);
    if previous.rotated || line.rotated {
        return false;
    }
    if (previous.size - line.size).abs() > size * 0.12 || previous.is_bold() != line.is_bold() {
        return false;
    }
    let step = line.baseline - previous.baseline;
    if step.abs() < size * 0.3 {
        // the same baseline: a line cut at a wide gap, read on
        return line.rect.x > previous.rect.right();
    }
    let expected = pitch.filter(|p| *p < size * 2.2).unwrap_or(size * 1.25);
    if step < 0.0 || step > expected * 1.3 {
        return false;
    }
    if list_marker(&line.text()).is_some_and(|m| m.strong || group.marker.is_some()) {
        return false;
    }
    let em = size;
    if group.marker.is_some() {
        // a list item's later lines line up with its words or its marker
        return (line.rect.x - group.text_start).abs() < em * 0.8
            || (line.rect.x - group.lines[0].rect.x).abs() < em * 0.3;
    }
    let centred = (line.rect.center_x() - previous.rect.center_x()).abs() < 2.0
        && (line.rect.x - previous.rect.x).abs() > 2.0;
    if centred {
        return true;
    }
    if line.rect.x - previous.rect.x > em * 0.8 {
        // indented: the first line of a new paragraph
        return false;
    }
    if line.rect.x < previous.rect.x - em * 0.8 && group.lines.len() > 1 {
        return false;
    }
    // a line that stopped well short of the others ended its paragraph
    let widest = group
        .lines
        .iter()
        .map(|l| l.rect.right())
        .fold(0.0, f32::max);
    let left = group
        .lines
        .iter()
        .map(|l| l.rect.x)
        .fold(f32::INFINITY, f32::min);
    let short = previous.rect.right() < widest - (em * 3.0).max((widest - left) * 0.15);
    // (a line ending in a hyphen goes on, however short it is)
    let hyphenated = previous.text().ends_with('-');
    !(group.lines.len() > 1 && short && !hyphenated)
}

/// joins the lines of a paragraph with spaces, rejoining words split
/// with a hyphen at a line end when `dehyphenate` is on ("conver-" +
/// "sion" is "conversion"; "well-" + "Known" keeps its hyphen).
pub(crate) fn join_lines(lines: Vec<Vec<TextRun>>, dehyphenate: bool) -> Vec<TextRun> {
    let mut joined: Vec<TextRun> = Vec::new();
    for line in lines.into_iter().filter(|l| !l.is_empty()) {
        if let Some(last) = joined.last_mut() {
            let next_starts_lower = line[0].text.chars().next().is_some_and(char::is_lowercase);
            let ends_split = {
                let mut chars = last.text.chars().rev();
                chars.next() == Some('-') && chars.next().is_some_and(char::is_alphabetic)
            };
            if dehyphenate && ends_split && next_starts_lower {
                last.text.pop();
            } else if ends_split {
                // a hyphen that stays is part of the word: no space after it
            } else if !last.text.ends_with(char::is_whitespace) {
                last.text.push(' ');
            }
        }
        for run in line {
            match joined.last_mut() {
                Some(last) if super::fragments::same_style(last, &run) => {
                    last.text.push_str(&run.text)
                }
                _ => joined.push(run),
            }
        }
    }
    joined
}

fn group_runs(group: &LineGroup, dehyphenate: bool) -> Vec<TextRun> {
    join_lines(group.lines.iter().map(|l| l.runs()).collect(), dehyphenate)
}

/// the kind of each group, then lists gathered from their items.
fn to_blocks(entries: Vec<Entry>, context: &Context) -> Vec<Block> {
    let mut blocks: Vec<Block> = Vec::new();
    let mut list: Vec<(ListItem, f32, bool)> = Vec::new();
    for entry in entries {
        match entry {
            Entry::Text(group) if group.marker.is_some() => {
                let marker = group.marker.clone().expect("checked");
                let rect = group.rect();
                // a numbered list straight after a bulleted one (or the
                // other way round) at the same indent is a new list
                let x = group.lines[0].rect.x;
                let new_kind = list.first().is_some_and(|(first, first_x, _)| {
                    (first_x - x).abs() < 4.0 && first.ordered != marker.ordered
                });
                if new_kind {
                    flush_list(&mut list, &mut blocks, context);
                }
                let runs = without_marker(group_runs(&group, context.dehyphenate), &marker.text);
                list.push((
                    ListItem {
                        level: 0,
                        ordered: marker.ordered,
                        marker: marker.text.clone(),
                        runs,
                        rect,
                    },
                    group.lines[0].rect.x,
                    marker.strong,
                ));
            }
            entry => {
                flush_list(&mut list, &mut blocks, context);
                blocks.push(match entry {
                    Entry::Text(group) => text_block(&group, context),
                    Entry::Table(found) => Block {
                        page: context.page,
                        rect: found.rect,
                        confidence: found.confidence,
                        content: BlockContent::Table(found.table),
                    },
                    Entry::Picture(picture) => Block {
                        page: context.page,
                        rect: picture.rect,
                        confidence: 0.95,
                        content: BlockContent::Figure(Figure {
                            image_id: picture.image_id,
                            pixel_width: picture.pixel_width,
                            pixel_height: picture.pixel_height,
                            caption: None,
                            caption_confidence: 0.0,
                        }),
                    },
                });
            }
        }
    }
    flush_list(&mut list, &mut blocks, context);
    blocks
}

fn without_marker(mut runs: Vec<TextRun>, marker: &str) -> Vec<TextRun> {
    if let Some(first) = runs.first_mut() {
        if let Some(rest) = first.text.trim_start().strip_prefix(marker) {
            first.text = rest.trim_start().to_string();
        }
    }
    runs.retain(|run| !run.text.is_empty());
    runs
}

/// turns the list items gathered so far into one list block. a lone item
/// with a weak marker ("-", "*") is more likely a sentence, so it becomes
/// a paragraph again.
fn flush_list(list: &mut Vec<(ListItem, f32, bool)>, blocks: &mut Vec<Block>, context: &Context) {
    if list.is_empty() {
        return;
    }
    let items = std::mem::take(list);
    if items.len() == 1 && !items[0].2 {
        let (item, _, _) = items.into_iter().next().expect("one item");
        let mut runs = item.runs;
        if let Some(first) = runs.first_mut() {
            first.text = format!("{} {}", item.marker, first.text);
        }
        blocks.push(Block {
            page: context.page,
            rect: item.rect,
            confidence: 0.6,
            content: BlockContent::Paragraph { runs },
        });
        return;
    }
    let mut indents: Vec<f32> = Vec::new();
    for (_, x, _) in &items {
        if !indents.iter().any(|i| (i - x).abs() < 4.0) {
            indents.push(*x);
        }
    }
    indents.sort_by(f32::total_cmp);
    let strong = items.iter().all(|(_, _, strong)| *strong);
    let confidence = match (strong, items.len()) {
        (true, 1) => 0.6,
        (true, _) => 0.9,
        (false, _) => 0.7,
    };
    let rect = union_all(items.iter().map(|(item, _, _)| &item.rect)).unwrap_or_default();
    let items = items
        .into_iter()
        .map(|(mut item, x, _)| {
            item.level = indents
                .iter()
                .position(|i| (i - x).abs() < 4.0)
                .unwrap_or(0) as u8;
            item
        })
        .collect();
    blocks.push(Block {
        page: context.page,
        rect,
        confidence,
        content: BlockContent::List { items },
    });
}

/// a heading, a footnote or a paragraph.
fn text_block(group: &LineGroup, context: &Context) -> Block {
    let rect = group.rect();
    let runs = group_runs(group, context.dehyphenate);
    let text = runs_text(&runs);
    let size = group.size();
    let ratio = if context.body_size > 0.0 {
        size / context.body_size
    } else {
        1.0
    };
    let short = group.lines.len() <= 3 && group.char_count() <= 200;
    let all_bold = group.lines.iter().all(|l| l.is_bold());

    if let Some((marker, confidence)) = footnote_marker(group, &text, context, ratio) {
        let runs = without_marker(runs, &marker);
        return Block {
            page: context.page,
            rect,
            confidence,
            content: BlockContent::Footnote { marker, runs },
        };
    }
    let heading_confidence = if short && ratio >= 1.5 {
        Some(0.9)
    } else if short && ratio >= 1.15 {
        Some(0.75)
    } else if all_bold
        && group.lines.len() <= 2
        && group.char_count() <= 120
        && ratio >= 0.95
        && !text.trim_end().ends_with(['.', ',', ';'])
    {
        Some(0.55)
    } else {
        None
    };
    match heading_confidence {
        Some(confidence) => Block {
            page: context.page,
            rect,
            confidence,
            content: BlockContent::Heading { level: 0, runs },
        },
        None => Block {
            page: context.page,
            rect,
            confidence: if group.lines.len() > 1 { 0.85 } else { 0.7 },
            content: BlockContent::Paragraph { runs },
        },
    }
}

/// a note at the foot of the page: small text low on the page starting
/// with a number or a note sign. a short rule just above it (the usual
/// separator) makes it more certain.
fn footnote_marker(
    group: &LineGroup,
    text: &str,
    context: &Context,
    ratio: f32,
) -> Option<(String, f32)> {
    let rect = group.rect();
    if rect.y < context.height * 0.6 || ratio > 0.88 {
        return None;
    }
    let first = group.lines[0].runs();
    let leading_superscript = first
        .first()
        .filter(|run| run.superscript)
        .map(|run| run.text.trim().to_string());
    let marker = leading_superscript.or_else(|| {
        let token: String = text.chars().take_while(|c| !c.is_whitespace()).collect();
        let plain = token.trim_end_matches(['.', ')']);
        let is_marker =
            (!plain.is_empty() && plain.len() <= 3 && plain.chars().all(|c| c.is_ascii_digit()))
                || (!plain.is_empty() && plain.chars().all(|c| matches!(c, '*' | '†' | '‡' | '§')));
        is_marker.then_some(token)
    })?;
    let separator = context.rules.iter().any(|rule| {
        rule.horizontal
            && rule.end - rule.start < context.width * 0.5
            && rule.position < rect.y
            && rect.y - rule.position < 24.0
    });
    Some((marker, if separator { 0.85 } else { 0.6 }))
}

/// words that start a caption
const CAPTION_WORDS: &[&str] = &[
    "figure",
    "fig.",
    "fig",
    "image",
    "photo",
    "chart",
    "diagram",
    "illustration",
    "map",
    "plate",
];

/// finds the caption of each picture and table: a short paragraph just
/// below or above it. it is taken out of the text and kept with the
/// picture or table.
fn attach_captions(mut blocks: Vec<Block>, body_size: f32) -> Vec<Block> {
    let mut index = 0;
    while index < blocks.len() {
        let is_target = matches!(
            blocks[index].content,
            BlockContent::Figure(_) | BlockContent::Table(_)
        );
        if !is_target {
            index += 1;
            continue;
        }
        let neighbours = [index + 1, index.wrapping_sub(1)];
        let mut taken = None;
        for neighbour in neighbours {
            let Some(candidate) = blocks.get(neighbour) else {
                continue;
            };
            if let Some(confidence) = caption_confidence(&blocks[index], candidate, body_size) {
                taken = Some((neighbour, confidence));
                break;
            }
        }
        if let Some((neighbour, confidence)) = taken {
            let caption = blocks.remove(neighbour);
            let runs = match caption.content {
                BlockContent::Paragraph { runs } | BlockContent::Heading { runs, .. } => runs,
                _ => unreachable!("only paragraphs are captions"),
            };
            if neighbour < index {
                index -= 1;
            }
            match &mut blocks[index].content {
                BlockContent::Figure(figure) => {
                    figure.caption = Some(runs);
                    figure.caption_confidence = confidence;
                }
                BlockContent::Table(table) => table.caption = Some(runs),
                _ => {}
            }
        }
        index += 1;
    }
    blocks
}

fn caption_confidence(target: &Block, candidate: &Block, body_size: f32) -> Option<f32> {
    let runs = match &candidate.content {
        BlockContent::Paragraph { runs } | BlockContent::Heading { runs, .. } => runs,
        _ => return None,
    };
    if candidate.page != target.page || candidate.rect.horizontal_overlap(&target.rect) <= 0.0 {
        return None;
    }
    let size = runs.iter().map(|r| r.size).fold(0.0, f32::max);
    let below = candidate.rect.y - target.rect.bottom();
    let above = target.rect.y - candidate.rect.bottom();
    let near = |gap: f32| gap > -2.0 && gap < size.max(body_size) * 2.5;
    if !(near(below) || near(above)) {
        return None;
    }
    let text = runs_text(runs).to_lowercase();
    let first_word = text.split_whitespace().next().unwrap_or("");
    let is_table = matches!(target.content, BlockContent::Table(_));
    if is_table {
        return (first_word == "table").then_some(0.85);
    }
    if CAPTION_WORDS.contains(&first_word) {
        return Some(0.85);
    }
    let small = size < body_size * 0.95 || runs.iter().all(|r| r.italic);
    (small && text.len() <= 300 && near(below)).then_some(0.55)
}

/// gives headings levels across the whole document: the largest heading
/// style is level 1, the next 2, and so on (bold before plain at one
/// size), at most 6 levels like html.
pub(crate) fn assign_heading_levels(blocks: &mut [Block]) {
    let key = |runs: &[TextRun]| {
        let size = runs.iter().map(|r| r.size).fold(0.0, f32::max);
        let bold = runs.iter().all(|r| r.bold || r.text.trim().is_empty());
        ((size * 2.0).round() as i32, bold)
    };
    let mut styles: Vec<(i32, bool)> = Vec::new();
    for block in blocks.iter() {
        if let BlockContent::Heading { runs, .. } = &block.content {
            let style = key(runs);
            if !styles.contains(&style) {
                styles.push(style);
            }
        }
    }
    styles.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));
    for block in blocks.iter_mut() {
        if let BlockContent::Heading { level, runs } = &mut block.content {
            let style = key(runs);
            *level = (styles.iter().position(|s| *s == style).unwrap_or(0) + 1).min(6) as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::fragments::tests::fragment;
    use super::*;

    fn parts<'a>(fragments: Vec<&'a Fragment>) -> PageParts<'a> {
        PageParts {
            page: 0,
            height: 792.0,
            width: 612.0,
            fragments,
            tables: Vec::new(),
            pictures: Vec::new(),
            rules: &[],
            body_size: 10.0,
            dehyphenate: true,
        }
    }

    fn kinds(blocks: &[Block]) -> Vec<String> {
        blocks
            .iter()
            .map(|b| match &b.content {
                BlockContent::Heading { .. } => format!("h: {}", b.text()),
                BlockContent::Paragraph { .. } => format!("p: {}", b.text()),
                BlockContent::List { .. } => format!("list: {}", b.text()),
                BlockContent::Footnote { marker, .. } => format!("note {marker}: {}", b.text()),
                BlockContent::Table(_) => "table".into(),
                BlockContent::Figure(_) => "figure".into(),
            })
            .collect()
    }

    #[test]
    fn lines_become_paragraphs_and_headings() {
        let lines = [
            fragment("Introduction", 72.0, 80.0, 16.0, true),
            fragment(
                "This first paragraph runs over two lines and its",
                72.0,
                110.0,
                10.0,
                false,
            ),
            fragment("words are conver-", 72.0, 122.0, 10.0, false),
            fragment("sion ready.", 72.0, 134.0, 10.0, false),
            fragment(
                "A second paragraph is set apart by a gap above it and",
                72.0,
                160.0,
                10.0,
                false,
            ),
            fragment("ends here.", 72.0, 172.0, 10.0, false),
        ];
        let (blocks, columns) = page_blocks(parts(lines.iter().collect()));
        assert_eq!(columns, 1);
        assert_eq!(
            kinds(&blocks),
            [
                "h: Introduction",
                "p: This first paragraph runs over two lines and its words are conversion ready.",
                "p: A second paragraph is set apart by a gap above it and ends here.",
            ]
        );
        assert!(blocks[0].confidence >= 0.9);
    }

    #[test]
    fn markers_make_nested_lists() {
        let lines = [
            fragment("Shopping:", 72.0, 80.0, 10.0, false),
            fragment("• Fruit", 72.0, 94.0, 10.0, false),
            fragment("◦ Apples", 90.0, 106.0, 10.0, false),
            fragment("◦ Pears", 90.0, 118.0, 10.0, false),
            fragment(
                "• Bread and some other things that take a",
                72.0,
                130.0,
                10.0,
                false,
            ),
            fragment("second line", 82.0, 142.0, 10.0, false),
            fragment("After the list.", 72.0, 170.0, 10.0, false),
        ];
        let (blocks, _) = page_blocks(parts(lines.iter().collect()));
        assert_eq!(blocks.len(), 3, "{:?}", kinds(&blocks));
        let BlockContent::List { items } = &blocks[1].content else {
            panic!("not a list: {:?}", kinds(&blocks));
        };
        let summary: Vec<(u8, &str, String)> = items
            .iter()
            .map(|i| (i.level, i.marker.as_str(), runs_text(&i.runs)))
            .collect();
        assert_eq!(
            summary,
            [
                (0, "•", "Fruit".to_string()),
                (1, "◦", "Apples".to_string()),
                (1, "◦", "Pears".to_string()),
                (
                    0,
                    "•",
                    "Bread and some other things that take a second line".to_string()
                ),
            ]
        );
        assert!(!items[0].ordered);
    }

    #[test]
    fn small_numbered_notes_low_on_the_page_are_footnotes() {
        let lines = [
            fragment("Body text that has a note.", 72.0, 100.0, 10.0, false),
            fragment("1 The note itself, in small type.", 72.0, 740.0, 7.0, false),
        ];
        let (blocks, _) = page_blocks(parts(lines.iter().collect()));
        assert_eq!(kinds(&blocks)[1], "note 1: The note itself, in small type.");
    }

    #[test]
    fn heading_levels_follow_size() {
        let lines = [
            fragment("Big", 72.0, 80.0, 20.0, true),
            fragment("Body", 72.0, 120.0, 10.0, false),
            fragment("Smaller", 72.0, 160.0, 14.0, true),
            fragment("More body", 72.0, 200.0, 10.0, false),
        ];
        let (mut blocks, _) = page_blocks(parts(lines.iter().collect()));
        assign_heading_levels(&mut blocks);
        let levels: Vec<u8> = blocks
            .iter()
            .filter_map(|b| match b.content {
                BlockContent::Heading { level, .. } => Some(level),
                _ => None,
            })
            .collect();
        assert_eq!(levels, [1, 2]);
    }

    #[test]
    fn hyphens_join_only_lower_case_words() {
        let line = |text: &str| fragment(text, 0.0, 0.0, 10.0, false).runs();
        assert_eq!(
            runs_text(&join_lines(vec![line("conver-"), line("sion")], true)),
            "conversion"
        );
        assert_eq!(
            runs_text(&join_lines(vec![line("conver-"), line("sion")], false)),
            "conver-sion"
        );
        assert_eq!(
            runs_text(&join_lines(vec![line("Jean-"), line("Paul")], true)),
            "Jean-Paul"
        );
        assert_eq!(
            runs_text(&join_lines(vec![line("one"), line("two")], true)),
            "one two"
        );
    }
}
