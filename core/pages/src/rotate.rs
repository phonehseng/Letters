//! turning pages.

use lopdf::{Document, Object};
use serde::{Deserialize, Serialize};

use crate::changes::Changes;
use crate::tree::{check_selection, page_ids, rotation};
use crate::PagesError;

/// settings for `rotate`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RotateParams {
    pub pages: Vec<u32>,
    /// quarter turns clockwise; negative turns anticlockwise
    pub quarter_turns: i32,
}

/// turns pages by quarter turns, on top of however they are turned now
/// (including a turn they inherit from the page tree). only the page's
/// /Rotate setting changes; nothing on the page is redrawn.
pub fn rotate(doc: &mut Document, params: &RotateParams) -> Result<(), PagesError> {
    let pages = page_ids(doc)?;
    let chosen = check_selection(&params.pages, pages.len())?;
    let turn = i64::from(params.quarter_turns).rem_euclid(4) * 90;
    let mut changes = Changes::new(doc);
    for position in chosen {
        let page = pages[position];
        let degrees = (i64::from(rotation(doc, page)) + turn) % 360;
        changes.set(page, "Rotate", Object::Integer(degrees));
    }
    changes.apply(doc);
    Ok(())
}
