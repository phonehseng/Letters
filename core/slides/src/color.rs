//! works out the rgb a drawingml colour stands for: the theme slot through
//! the colour map, then each transform in order (ecma-376 part 1,
//! 20.1.2.3). luminance, saturation and hue transforms work in hsl;
//! tint and shade work on linear light, which is what powerpoint does
//! (checked against powerpoint's own pdf colours in tests/colours.rs).

use crate::model::{ColorKind, ColorMap, ColorRef, ColorScheme};

/// a resolved colour: red, green, blue and alpha from 0 to 1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rgba {
    pub red: f64,
    pub green: f64,
    pub blue: f64,
    pub alpha: f64,
}

impl Rgba {
    pub const BLACK: Rgba = Rgba::opaque(0.0, 0.0, 0.0);
    pub const WHITE: Rgba = Rgba::opaque(1.0, 1.0, 1.0);

    pub const fn opaque(red: f64, green: f64, blue: f64) -> Rgba {
        Rgba {
            red,
            green,
            blue,
            alpha: 1.0,
        }
    }

    pub fn from_hex(hex: &str) -> Option<Rgba> {
        let value = u32::from_str_radix(hex.get(..6)?, 16).ok()?;
        let channel = |shift: u32| f64::from((value >> shift) & 0xFF) / 255.0;
        Some(Rgba::opaque(channel(16), channel(8), channel(0)))
    }

    /// six hex digits, rounded the way 8-bit colours are.
    pub fn to_hex(&self) -> String {
        let byte = |value: f64| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
        format!(
            "{:02X}{:02X}{:02X}",
            byte(self.red),
            byte(self.green),
            byte(self.blue)
        )
    }
}

/// what a colour is resolved against.
pub struct ColorContext<'a> {
    pub scheme: &'a ColorScheme,
    pub map: &'a ColorMap,
    /// the colour a style reference hands in for `phClr`
    pub placeholder: Option<Rgba>,
}

/// slots a colour map names ("tx1"); anything else is a scheme slot.
const MAPPED_NAMES: [&str; 4] = ["tx1", "tx2", "bg1", "bg2"];

pub fn resolve(color: &ColorRef, context: &ColorContext<'_>) -> Rgba {
    let base = base_color(&color.kind, context, 0);
    color.mods.iter().fold(base, |current, modifier| {
        apply(current, &modifier.name, modifier.value)
    })
}

/// the colour before transforms. `depth` stops a scheme that refers to
/// itself from looping.
fn base_color(kind: &ColorKind, context: &ColorContext<'_>, depth: u8) -> Rgba {
    match kind {
        ColorKind::Rgb { hex } => Rgba::from_hex(hex).unwrap_or(Rgba::BLACK),
        ColorKind::Scheme { slot } => scheme_color(slot, context, depth),
        ColorKind::System { name, last } => last
            .as_deref()
            .and_then(Rgba::from_hex)
            .unwrap_or_else(|| system_color(name)),
        ColorKind::Preset { name } => preset_color(name),
        ColorKind::Hsl { hue, sat, lum } => from_hsl(
            f64::from(*hue) / 60000.0,
            f64::from(*sat) / 100000.0,
            f64::from(*lum) / 100000.0,
        ),
        ColorKind::ScRgb { r, g, b } => Rgba::opaque(
            linear_to_srgb(f64::from(*r) / 100000.0),
            linear_to_srgb(f64::from(*g) / 100000.0),
            linear_to_srgb(f64::from(*b) / 100000.0),
        ),
    }
}

fn scheme_color(slot: &str, context: &ColorContext<'_>, depth: u8) -> Rgba {
    if slot == "phClr" {
        return context.placeholder.unwrap_or(Rgba::BLACK);
    }
    let theme_slot = if MAPPED_NAMES.contains(&slot) {
        context.map.map(slot).unwrap_or(match slot {
            "tx1" => "dk1",
            "tx2" => "dk2",
            "bg1" => "lt1",
            _ => "lt2",
        })
    } else {
        slot
    };
    match context.scheme.slot(theme_slot) {
        Some(color) if depth < 4 => {
            let base = base_color(&color.kind, context, depth + 1);
            color
                .mods
                .iter()
                .fold(base, |c, m| apply(c, &m.name, m.value))
        }
        _ => Rgba::BLACK,
    }
}

/// the colours windows reports for the system names themes use most.
fn system_color(name: &str) -> Rgba {
    match name {
        "window" | "highlightText" | "btnHighlight" => Rgba::WHITE,
        "highlight" => Rgba::from_hex("0078D7").unwrap_or(Rgba::BLACK),
        "btnFace" | "menu" => Rgba::from_hex("F0F0F0").unwrap_or(Rgba::WHITE),
        _ => Rgba::BLACK,
    }
}

/// the preset colour names powerpoint writes; others fall back to black.
fn preset_color(name: &str) -> Rgba {
    let hex = match name {
        "white" => "FFFFFF",
        "red" => "FF0000",
        "green" => "008000",
        "blue" => "0000FF",
        "yellow" => "FFFF00",
        "gray" | "grey" => "808080",
        "ltGray" | "lightGray" => "D3D3D3",
        "dkGray" | "darkGray" => "A9A9A9",
        "orange" => "FFA500",
        "cyan" | "aqua" => "00FFFF",
        "magenta" | "fuchsia" => "FF00FF",
        "navy" => "000080",
        "purple" => "800080",
        "silver" => "C0C0C0",
        _ => "000000",
    };
    Rgba::from_hex(hex).unwrap_or(Rgba::BLACK)
}

/// one transform; `value` is in thousandths of a percent (or 60000ths of
/// a degree for hue).
fn apply(color: Rgba, name: &str, value: i32) -> Rgba {
    let fraction = f64::from(value) / 100000.0;
    match name {
        "alpha" => Rgba {
            alpha: fraction,
            ..color
        },
        "alphaMod" => Rgba {
            alpha: color.alpha * fraction,
            ..color
        },
        "alphaOff" => Rgba {
            alpha: (color.alpha + fraction).clamp(0.0, 1.0),
            ..color
        },
        "lumMod" | "lumOff" | "satMod" | "satOff" | "sat" | "lum" | "hue" | "hueMod" | "hueOff" => {
            apply_hsl(color, name, value)
        }
        "tint" => map_linear(color, |c| c * fraction + (1.0 - fraction)),
        "shade" => map_linear(color, |c| c * fraction),
        "gamma" => map_channels(color, linear_to_srgb),
        "invGamma" => map_channels(color, srgb_to_linear),
        "comp" => complement(color),
        "inv" => map_channels(color, |c| 1.0 - c),
        "gray" => {
            let grey = 0.3 * color.red + 0.59 * color.green + 0.11 * color.blue;
            Rgba {
                red: grey,
                green: grey,
                blue: grey,
                alpha: color.alpha,
            }
        }
        "red" | "green" | "blue" | "redMod" | "greenMod" | "blueMod" | "redOff" | "greenOff"
        | "blueOff" => apply_channel(color, name, fraction),
        _ => color,
    }
}

fn apply_hsl(color: Rgba, name: &str, value: i32) -> Rgba {
    let (mut hue, mut saturation, mut lightness) = to_hsl(color);
    let fraction = f64::from(value) / 100000.0;
    let degrees = f64::from(value) / 60000.0;
    match name {
        "lumMod" => lightness *= fraction,
        "lumOff" => lightness += fraction,
        "lum" => lightness = fraction,
        "satMod" => saturation *= fraction,
        "satOff" => saturation += fraction,
        "sat" => saturation = fraction,
        "hue" => hue = degrees,
        "hueMod" => hue *= fraction,
        _ => hue += degrees,
    }
    let mut result = from_hsl(
        hue.rem_euclid(360.0),
        saturation.clamp(0.0, 1.0),
        lightness.clamp(0.0, 1.0),
    );
    result.alpha = color.alpha;
    result
}

fn apply_channel(color: Rgba, name: &str, fraction: f64) -> Rgba {
    // channel transforms work on linear light, like tint and shade
    let mut linear = [
        srgb_to_linear(color.red),
        srgb_to_linear(color.green),
        srgb_to_linear(color.blue),
    ];
    let index = match &name[..1] {
        "r" => 0,
        "g" => 1,
        _ => 2,
    };
    if name.ends_with("Mod") {
        linear[index] *= fraction;
    } else if name.ends_with("Off") {
        linear[index] += fraction;
    } else {
        linear[index] = fraction;
    }
    Rgba {
        red: linear_to_srgb(linear[0].clamp(0.0, 1.0)),
        green: linear_to_srgb(linear[1].clamp(0.0, 1.0)),
        blue: linear_to_srgb(linear[2].clamp(0.0, 1.0)),
        alpha: color.alpha,
    }
}

fn complement(color: Rgba) -> Rgba {
    let (hue, saturation, lightness) = to_hsl(color);
    let mut result = from_hsl((hue + 180.0).rem_euclid(360.0), saturation, lightness);
    result.alpha = color.alpha;
    result
}

fn map_channels(color: Rgba, change: impl Fn(f64) -> f64) -> Rgba {
    Rgba {
        red: change(color.red).clamp(0.0, 1.0),
        green: change(color.green).clamp(0.0, 1.0),
        blue: change(color.blue).clamp(0.0, 1.0),
        alpha: color.alpha,
    }
}

fn map_linear(color: Rgba, change: impl Fn(f64) -> f64) -> Rgba {
    map_channels(color, |c| {
        linear_to_srgb(change(srgb_to_linear(c)).clamp(0.0, 1.0))
    })
}

/// srgb's transfer curve (iec 61966-2-1).
pub fn srgb_to_linear(value: f64) -> f64 {
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

pub fn linear_to_srgb(value: f64) -> f64 {
    if value <= 0.003_130_8 {
        value * 12.92
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    }
}

/// hue in degrees, saturation and lightness from 0 to 1.
pub fn to_hsl(color: Rgba) -> (f64, f64, f64) {
    let max = color.red.max(color.green).max(color.blue);
    let min = color.red.min(color.green).min(color.blue);
    let lightness = (max + min) / 2.0;
    if (max - min).abs() < 1e-12 {
        return (0.0, 0.0, lightness);
    }
    let delta = max - min;
    let saturation = if lightness > 0.5 {
        delta / (2.0 - max - min)
    } else {
        delta / (max + min)
    };
    let hue = if max == color.red {
        ((color.green - color.blue) / delta).rem_euclid(6.0)
    } else if max == color.green {
        (color.blue - color.red) / delta + 2.0
    } else {
        (color.red - color.green) / delta + 4.0
    };
    (hue * 60.0, saturation, lightness)
}

pub fn from_hsl(hue: f64, saturation: f64, lightness: f64) -> Rgba {
    let chroma = (1.0 - (2.0 * lightness - 1.0).abs()) * saturation;
    let segment = hue / 60.0;
    let second = chroma * (1.0 - (segment.rem_euclid(2.0) - 1.0).abs());
    let (red, green, blue) = match segment as u32 {
        0 => (chroma, second, 0.0),
        1 => (second, chroma, 0.0),
        2 => (0.0, chroma, second),
        3 => (0.0, second, chroma),
        4 => (second, 0.0, chroma),
        _ => (chroma, 0.0, second),
    };
    let lift = lightness - chroma / 2.0;
    Rgba::opaque(red + lift, green + lift, blue + lift)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context<'a>(scheme: &'a ColorScheme, map: &'a ColorMap) -> ColorContext<'a> {
        ColorContext {
            scheme,
            map,
            placeholder: None,
        }
    }

    #[test]
    fn scheme_slots_go_through_the_colour_map() {
        let (scheme, map) = (ColorScheme::default(), ColorMap::default());
        let text = resolve(&ColorRef::scheme("tx1"), &context(&scheme, &map));
        assert_eq!(text.to_hex(), "000000");
        let accent = resolve(&ColorRef::scheme("accent1"), &context(&scheme, &map));
        assert_eq!(accent.to_hex(), "4472C4");
    }

    /// powerpoint's own "accent 1, lighter 40 %" (lumMod 60000 lumOff 40000)
    /// of 4472C4 is 8FAADC, and "darker 25 %" (lumMod 75000) is 2F5597:
    /// both shown in its colour picker and written to its pdf
    #[test]
    fn luminance_transforms_match_powerpoint() {
        let (scheme, map) = (ColorScheme::default(), ColorMap::default());
        let lighter = ColorRef::scheme("accent1")
            .with("lumMod", 60000)
            .with("lumOff", 40000);
        assert_eq!(
            resolve(&lighter, &context(&scheme, &map)).to_hex(),
            "8FAADC"
        );
        let darker = ColorRef::scheme("accent1").with("lumMod", 75000);
        assert_eq!(resolve(&darker, &context(&scheme, &map)).to_hex(), "2F5597");
    }

    #[test]
    fn alpha_and_hsl_round_trip() {
        let (scheme, map) = (ColorScheme::default(), ColorMap::default());
        let faded = ColorRef::rgb("FF0000").with("alpha", 50000);
        let color = resolve(&faded, &context(&scheme, &map));
        assert_eq!((color.to_hex(), color.alpha), ("FF0000".to_string(), 0.5));
        for hex in ["4472C4", "ED7D31", "A5A5A5", "70AD47", "000000", "FFFFFF"] {
            let rgb = Rgba::from_hex(hex).unwrap();
            let (h, s, l) = to_hsl(rgb);
            assert_eq!(from_hsl(h, s, l).to_hex(), hex);
        }
    }

    #[test]
    fn shade_and_tint_work_on_linear_light() {
        let (scheme, map) = (ColorScheme::default(), ColorMap::default());
        let white_shade = ColorRef::rgb("FFFFFF").with("shade", 50000);
        // half the light of white is srgb 0.735, not 0.5
        assert_eq!(
            resolve(&white_shade, &context(&scheme, &map)).to_hex(),
            "BCBCBC"
        );
        let black_tint = ColorRef::rgb("000000").with("tint", 50000);
        assert_eq!(
            resolve(&black_tint, &context(&scheme, &map)).to_hex(),
            "BCBCBC"
        );
    }
}
