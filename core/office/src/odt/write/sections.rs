//! section breaks in the body. a section that starts on a new page gets a
//! master page of its own; a continuous section on the same page size
//! becomes a `text:section` with its columns, which is how opendocument
//! changes the number of columns in the middle of a page.

use crate::model::{Section, SectionStart};

use super::OdtWriter;

/// which section's master page is in effect, and whether a column
/// section is open.
pub(super) struct SectionState {
    master_section: usize,
    open: bool,
}

impl SectionState {
    pub(super) fn new() -> SectionState {
        SectionState {
            master_section: 0,
            open: false,
        }
    }
}

fn same_columns(a: &Section, b: &Section) -> bool {
    a.column_layout() == b.column_layout() && a.columns.separator == b.columns.separator
}

impl OdtWriter<'_> {
    /// what the break before section `section` does: the master page to
    /// start a new page with, or `None` when the text carries on on the
    /// same page (inside a `text:section` when its columns differ).
    pub(super) fn break_section(
        &mut self,
        section: usize,
        state: &mut SectionState,
        out: &mut String,
    ) -> Option<String> {
        self.close_column_section(state, out);
        let doc = self.doc;
        let next = doc.sections.get(section)?;
        let master = doc.sections.get(state.master_section)?;
        let continuous = next.start == SectionStart::Continuous && next.page == master.page;
        if !continuous {
            state.master_section = section;
            return Some(self.master_name(section));
        }
        if !same_columns(next, master) {
            let body = format!(
                "<style:section-properties text:dont-balance-text-columns=\"false\">{}</style:section-properties>",
                super::super::table_props::columns_xml(next)
            );
            let style = self.automatic_style("LSect", "section", "", &body);
            out.push_str(&format!(
                "<text:section text:style-name=\"{style}\" text:name=\"Section{section}\">"
            ));
            state.open = true;
        }
        None
    }

    pub(super) fn close_column_section(&mut self, state: &mut SectionState, out: &mut String) {
        if state.open {
            out.push_str("</text:section>");
            state.open = false;
        }
    }
}
