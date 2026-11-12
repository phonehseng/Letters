//! a table's rows flowing down the column: rows that fit are drawn whole,
//! a row that does not is moved to the next page or split between lines
//! when it may be, and header rows repeat at the top of every page the
//! table continues on.

use super::draw::{draw_row, CellCursor, RowLimit};
use super::PreparedTable;
use crate::pages::{EmittedBlocks, Flow, PreviousParagraph, FIT_TOLERANCE};
impl Flow<'_, '_> {
    /// places a table's rows, moving or splitting rows at page ends and
    /// repeating header rows.
    pub fn place_table(&mut self, table: &PreparedTable) {
        self.place_table_from(table, 0);
    }

    /// places a table's rows from `first_row` on (an incremental relayout
    /// resumes at the row a page started with, header rows repeated).
    pub fn place_table_from(&mut self, table: &PreparedTable, first_row: usize) {
        let headers = table.rows.iter().take_while(|row| row.header).count();
        if first_row == 0 {
            if let Some(previous) = self.previous.take() {
                if !self.column_empty {
                    self.y += previous.space_after;
                }
            }
        } else if first_row >= headers {
            for header in 0..headers {
                self.draw_table_row(table, header, None, &[]);
            }
        }
        let resumable = self.cursor.resumable;
        let mut index = first_row;
        let mut cursors: Vec<CellCursor> = Vec::new();
        while index < table.rows.len() {
            self.cursor.line = index;
            let row = &table.rows[index];
            let fresh = cursors.is_empty();
            let fits = self.y + row.height <= self.bottom() + FIT_TOLERANCE;
            if fresh && fits {
                self.draw_table_row(table, index, None, &[]);
                index += 1;
                continue;
            }
            let room = self.bottom() - self.y;
            let can_split = !row.cant_split && room > 6.0;
            if !self.column_empty && !can_split {
                self.cursor.resumable = resumable;
                self.continue_table_on_next_page(table, headers, index);
                if self.stopped.is_some() {
                    return;
                }
                continue;
            }
            let limit = RowLimit {
                height: room.max(1.0),
                orphans_may_move: !self.column_empty && self.context.flavor.rules().widows_in_cells,
            };
            match self.draw_table_row(table, index, Some(limit), &cursors) {
                Some(rest) => {
                    cursors = rest;
                    // a row split over pages: its second part cannot be resumed
                    self.cursor.resumable = false;
                    self.continue_table_on_next_page(table, headers, index);
                    self.cursor.resumable = resumable;
                    if self.stopped.is_some() {
                        return;
                    }
                }
                None => {
                    cursors.clear();
                    index += 1;
                }
            }
        }
        self.cursor.line = 0;
        self.previous = Some(PreviousParagraph {
            style_id: None,
            space_after: 0.0,
            contextual: false,
        });
    }

    fn continue_table_on_next_page(&mut self, table: &PreparedTable, headers: usize, index: usize) {
        self.next_column();
        if index >= headers {
            for header in 0..headers {
                self.draw_table_row(table, header, None, &[]);
            }
        }
    }

    fn draw_table_row(
        &mut self,
        table: &PreparedTable,
        index: usize,
        limit: Option<RowLimit>,
        cursors: &[CellCursor],
    ) -> Option<Vec<CellCursor>> {
        let (left, _) = self.column_box();
        let mut out = EmittedBlocks::default();
        let (height, rest) = draw_row(
            self.context,
            table,
            index,
            left,
            self.y,
            limit,
            cursors,
            &mut out,
        );
        out.apply_to(&mut self.page().page);
        self.y += height;
        self.column_empty = false;
        rest
    }
}
