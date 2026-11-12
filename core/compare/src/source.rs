//! what the comparison needs to know about a document, as a trait.
//!
//! the comparison never opens files itself. it asks a `PageSource` for page
//! sizes, the characters on a page and a picture of a page. that keeps it
//! independent of any one pdf library: `LopdfSource` reads files with lopdf,
//! and the shell can wrap its own engine the same way.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::geometry::{PageSize, Rect};
use crate::CompareError;

/// a colour as 8-bit red, green and blue.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Colour {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Colour {
    pub const BLACK: Colour = Colour { r: 0, g: 0, b: 0 };

    pub fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// builds a colour from pdf's 0 to 1 components.
    pub fn from_unit(r: f32, g: f32, b: f32) -> Self {
        let channel = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
        Self::new(channel(r), channel(g), channel(b))
    }

    /// the largest difference in any one channel.
    pub fn distance(&self, other: &Colour) -> u8 {
        let r = self.r.abs_diff(other.r);
        let g = self.g.abs_diff(other.g);
        let b = self.b.abs_diff(other.b);
        r.max(g).max(b)
    }
}

/// one character drawn on a page.
#[derive(Clone, Debug, PartialEq)]
pub struct Glyph {
    pub ch: char,
    /// where the character sits, in points on the visible page (top left, y down)
    pub rect: Rect,
    /// the font's name as the pdf gives it, without any subset prefix
    pub font_name: Arc<str>,
    /// the size the character is really drawn at, in points
    pub font_size: f32,
    pub colour: Colour,
    pub bold: bool,
    pub italic: bool,
}

/// a picture of a page: 8-bit red, green, blue and alpha, row by row from the top.
#[derive(Clone, Debug, PartialEq)]
pub struct RgbaImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl RgbaImage {
    /// a white picture of the given size.
    pub fn white(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            pixels: vec![255; width as usize * height as usize * 4],
        }
    }

    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let start = (y as usize * self.width as usize + x as usize) * 4;
        [
            self.pixels[start],
            self.pixels[start + 1],
            self.pixels[start + 2],
            self.pixels[start + 3],
        ]
    }

    pub fn set_pixel(&mut self, x: u32, y: u32, value: [u8; 4]) {
        let start = (y as usize * self.width as usize + x as usize) * 4;
        self.pixels[start..start + 4].copy_from_slice(&value);
    }

    /// the pixel blended onto white, so transparent areas read as paper.
    pub fn rgb_on_white(&self, x: u32, y: u32) -> [u8; 3] {
        let [r, g, b, a] = self.pixel(x, y);
        let blend = |channel: u8| {
            let alpha = a as u32;
            ((channel as u32 * alpha + 255 * (255 - alpha)) / 255) as u8
        };
        [blend(r), blend(g), blend(b)]
    }

    /// the same picture turned a quarter turn counter clockwise, `turns` times.
    ///
    /// used to bring a picture of a turned page back upright.
    pub fn turned_counter_clockwise(&self, turns: u32) -> RgbaImage {
        let mut image = self.clone();
        for _ in 0..turns % 4 {
            image = image.one_turn_counter_clockwise();
        }
        image
    }

    fn one_turn_counter_clockwise(&self) -> RgbaImage {
        let mut turned = RgbaImage::white(self.height, self.width);
        for y in 0..self.height {
            for x in 0..self.width {
                // the right edge becomes the top edge
                let new_x = y;
                let new_y = self.width - 1 - x;
                turned.set_pixel(new_x, new_y, self.pixel(x, y));
            }
        }
        turned
    }
}

/// a document the comparison can read. pages are counted from 0.
///
/// implementations should be cheap to call repeatedly: the comparison asks
/// for each page's characters twice (once to match pages, once to compare
/// them) rather than holding every page in memory at once.
pub trait PageSource {
    fn page_count(&self) -> u32;

    /// the page's size as the person sees it, after cropping and turning.
    fn page_size(&self, page: u32) -> Result<PageSize, CompareError>;

    /// every character on the page, in the order the file draws them.
    fn glyphs(&self, page: u32) -> Result<Vec<Glyph>, CompareError>;

    /// a picture of the page at `scale` pixels per point.
    fn render(&self, page: u32, scale: f32) -> Result<RgbaImage, CompareError>;

    /// whether `render` can work at all. pictures are not compared when
    /// either document can't be drawn.
    fn can_render(&self) -> bool {
        true
    }

    /// a number that is the same for two pages only when they draw exactly
    /// the same thing. when both pages of a pair have one and they match,
    /// the pictures are not drawn and compared, which saves a lot of time on
    /// long documents. `None` means "unknown, draw and compare".
    fn drawing_fingerprint(&self, _page: u32) -> Option<u64> {
        None
    }
}
