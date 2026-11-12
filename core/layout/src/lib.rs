//! lays out office documents into pages on its own, measured against word
//! and libreoffice.
//!
//! the pipeline, one module per stage:
//!
//! 1. `fonts`: finding faces, their metrics, substitutions
//! 2. `style`: resolved paragraph and run formatting in points
//! 3. `text`: shaped clusters and break opportunities
//! 4. `paragraph`: lines (greedy first fit, tabs, alignment, line spacing)
//! 5. `lists`, `tables`, `floats`, `footnotes`, `columns`: the block kinds
//! 6. `pages`: sections, headers and footers, keeps, page building
//! 7. `result`: the layout result and the model to page map
//! 8. `pdf`: the result as a pdf
//!
//! `flavor` holds the rules where word and libreoffice differ.

pub mod columns;
pub mod engine;
pub mod fields;
pub mod flavor;
pub mod floats;
pub mod fonts;
pub mod footnotes;
pub mod grid;
pub mod incremental;
pub mod lists;
pub mod pages;
pub mod paragraph;
pub mod pdf;
pub mod positioned;
pub mod provider;
pub mod result;
pub mod stack;
pub mod style;
pub mod tables;
pub mod text;
pub mod units;
pub mod unsupported;

pub use engine::{layout, LayoutOptions};
pub use flavor::{Flavor, Rules};
pub use incremental::{LayoutEngine, PageChanges, RelayoutStats};
pub use positioned::{
    layout_exact, layout_mixed, ExactFont, ExactGlyph, ExactPage, ExactRun, MixedPart,
};
pub use result::{LayoutResult, ModelPosition, Story};
