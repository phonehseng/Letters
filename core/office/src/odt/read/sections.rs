//! the body's sections. a new master page starts a section on a new page;
//! a `text:section` whose style has columns is a section that carries on
//! the same page (word's continuous section break), so its columns change
//! mid-page.

use crate::model::{Block, Section, SectionStart};
use crate::xml::{Element, XmlDocument};

use super::OdtReader;

impl OdtReader<'_> {
    /// the body's children, one after another, starting sections where a
    /// master page or a column section says so.
    pub(super) fn read_body_children(
        &mut self,
        children: &[&Element],
        xml: &XmlDocument,
        blocks: &mut Vec<Block>,
        sections: &mut Vec<Section>,
    ) {
        for (index, child) in children.iter().enumerate() {
            self.start_master_page(child, blocks, sections);
            if child.name != "text:section" {
                self.read_block(child, xml, blocks);
                continue;
            }
            let inner: Vec<&Element> = child.elements().collect();
            match self.column_section(child, sections) {
                Some(section) => {
                    let more_follows = index + 1 < children.len();
                    self.read_column_section(&inner, section, more_follows, xml, blocks, sections);
                }
                // a section without columns only groups its content
                None => self.read_body_children(&inner, xml, blocks, sections),
            }
        }
    }

    /// a new master page on the first block of a run starts a section.
    fn start_master_page(
        &mut self,
        child: &Element,
        blocks: &mut Vec<Block>,
        sections: &mut Vec<Section>,
    ) {
        let Some(master) = self.master_page_of(child) else {
            return;
        };
        let Some(page) = self.masters.get(&master).cloned() else {
            return;
        };
        if blocks.is_empty() {
            sections[0] = page.section;
        } else {
            blocks.push(Block::SectionBreak);
            sections.push(page.section);
        }
    }

    /// the section a `text:section` stands for, when its style gives it
    /// columns other than the page's.
    fn column_section(&self, element: &Element, sections: &[Section]) -> Option<Section> {
        let name = element.attr("text:style-name")?;
        let (style, _) = self.style(name)?;
        let columns = style.table_layout.columns.as_ref()?;
        let current = sections.last().cloned().unwrap_or_default();
        let (count_and_gap, widths) =
            crate::odt::table_props::read_columns(columns, current.page.text_width());
        let mut section = current;
        section.columns = count_and_gap;
        section.column_widths = widths;
        section.start = SectionStart::Continuous;
        Some(section)
    }

    /// a column section between continuous breaks. after it the page's
    /// own columns come back, unless the body ends here.
    fn read_column_section(
        &mut self,
        inner: &[&Element],
        section: Section,
        more_follows: bool,
        xml: &XmlDocument,
        blocks: &mut Vec<Block>,
        sections: &mut Vec<Section>,
    ) {
        let surrounding = sections.last().cloned().unwrap_or_default();
        if blocks.is_empty() {
            // the document starts with it: the first section keeps the
            // page's start
            let start = sections[0].start;
            sections[0] = Section { start, ..section };
        } else {
            blocks.push(Block::SectionBreak);
            sections.push(section);
        }
        self.read_body_children(inner, xml, blocks, sections);
        if more_follows {
            blocks.push(Block::SectionBreak);
            sections.push(Section {
                start: SectionStart::Continuous,
                ..surrounding
            });
        }
    }
}
