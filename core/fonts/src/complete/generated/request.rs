//! the generation request, its disclosure and the yes to it: what would
//! be sent and where, and the permit only an acknowledged plan can make.

use super::*;

/// one request for glyphs. made only by `plan_generation`.
#[derive(Clone, Debug, PartialEq)]
pub struct GlyphRequest {
    pub(crate) examples: Vec<RequestGlyph>,
    pub(crate) metrics: RequestMetrics,
    pub(crate) wanted: Vec<char>,
}

impl GlyphRequest {
    /// the request as the json a provider receives.
    pub fn body(&self) -> String {
        let wanted: Vec<String> = self.wanted.iter().map(|c| c.to_string()).collect();
        serde_json::json!({
            "format": "letters-glyph-request-1",
            "grid": GRID,
            "yUp": true,
            "metrics": self.metrics,
            "examples": self.examples,
            "wanted": wanted,
            "instructions": INSTRUCTIONS,
        })
        .to_string()
    }

    pub fn wanted(&self) -> &[char] {
        &self.wanted
    }

    pub fn example_count(&self) -> usize {
        self.examples.len()
    }

    /// the same request asking for fewer characters (never more).
    pub(crate) fn narrowed(&self, chars: &[char]) -> GlyphRequest {
        GlyphRequest {
            wanted: self
                .wanted
                .iter()
                .copied()
                .filter(|c| chars.contains(c))
                .collect(),
            ..self.clone()
        }
    }
}

/// what the provider is told to do. plain words; the example glyphs are
/// data in the json, not instructions.
pub const INSTRUCTIONS: &str = "Draw each character in \"wanted\" in the same typeface as the \"examples\": the same stem weight, heights, contrast, slant, serifs and proportions. Coordinates are on a grid of 1000 units per em, y up, baseline at 0, origin at the left of the advance. Reply with JSON only: {\"glyphs\":[{\"char\":\"x\",\"advance\":500,\"path\":\"M x y L x y Q cx cy x y C c1x c1y c2x c2y x y Z\"}]}, using only absolute M, L, Q, C and Z commands, with every contour closed by Z and no contour crossing itself or another.";

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RequestGlyph {
    #[serde(rename = "char")]
    pub(crate) character: String,
    pub(crate) advance: i64,
    pub(crate) path: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RequestMetrics {
    pub(crate) x_height: i64,
    pub(crate) cap_height: i64,
    pub(crate) ascender: i64,
    pub(crate) descender: i64,
    pub(crate) stem: i64,
    pub(crate) cap_stem: i64,
    pub(crate) slant_degrees: f64,
    pub(crate) contrast: f64,
    pub(crate) serif: bool,
}

pub(crate) static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// what a planned generation would send, and where.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GlyphDisclosure {
    pub id: u64,
    pub leaves_device: bool,
    pub destination: String,
    pub provider_name: String,
    /// how many glyphs of the document's font go as examples
    pub example_glyphs: usize,
    /// the characters asked for
    pub wanted: Vec<char>,
    pub sentence: String,
}

impl GlyphDisclosure {
    pub fn needs_confirmation(&self) -> bool {
        self.leaves_device && !self.wanted.is_empty()
    }

    /// the person's yes to this disclosure, and only this one.
    pub fn acknowledge(&self) -> GlyphAcknowledgement {
        GlyphAcknowledgement { id: self.id }
    }

    pub(crate) fn describe(&self) -> String {
        if !self.leaves_device {
            return "This stays on this computer.".to_string();
        }
        if self.wanted.is_empty() {
            return "Nothing will be sent.".to_string();
        }
        format!(
            "This will send the outlines of {} letters of this document’s font and the characters wanted ({}) to {}. No text of the document is sent.",
            self.example_glyphs,
            char_list(&self.wanted),
            self.destination
        )
    }
}

/// the person's confirmation of one disclosure; made only by
/// `GlyphDisclosure::acknowledge`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GlyphAcknowledgement {
    pub(crate) id: u64,
}

/// proof that the person agreed; generators check it before sending.
#[derive(Debug)]
pub struct GlyphPermit {
    pub(crate) destination: String,
    pub(crate) leaves_device: bool,
}

impl GlyphPermit {
    /// refuses a permit made for another destination, or one for "stays
    /// on this computer" used with a generator that sends things away.
    pub fn check(&self, info: &GeneratorInfo) -> Result<(), GenerationError> {
        if self.destination == info.host && (self.leaves_device || !info.leaves_device) {
            Ok(())
        } else {
            Err(GenerationError::ConsentRequired)
        }
    }
}

/// the example glyphs: letters first (they show the style best), then
/// digits, at most `MAX_EXAMPLES`, preferring a spread of shapes.
pub(crate) fn choose_examples(set: &GlyphSet) -> Vec<char> {
    const PREFERRED: &str = "nohHOaegsdpbAERSt1027ilvxk";
    let mut chosen: Vec<char> = PREFERRED
        .chars()
        .filter(|c| set.get(*c).is_some_and(|g| !g.shape.is_empty()))
        .collect();
    for (c, glyph) in &set.glyphs {
        if chosen.len() >= MAX_EXAMPLES {
            break;
        }
        if !chosen.contains(c) && !glyph.shape.is_empty() && c.is_alphanumeric() {
            chosen.push(*c);
        }
    }
    chosen.truncate(MAX_EXAMPLES);
    chosen
}

/// works out a generation request and its disclosure. nothing is sent.
pub fn plan_generation(
    set: &GlyphSet,
    style: &StyleFit,
    wanted: &[char],
    info: &GeneratorInfo,
) -> GenerationPlan {
    let units = |value: f64| (value * GRID).round() as i64;
    let examples: Vec<RequestGlyph> = choose_examples(set)
        .into_iter()
        .map(|c| {
            let glyph = &set.glyphs[&c];
            RequestGlyph {
                character: c.to_string(),
                advance: units(glyph.advance),
                path: glyph.shape.to_path_data(GRID),
            }
        })
        .collect();
    let mut wanted: Vec<char> = wanted
        .iter()
        .copied()
        .filter(|c| !c.is_whitespace())
        .collect();
    wanted.sort_unstable();
    wanted.dedup();
    wanted.truncate(MAX_WANTED);
    let request = GlyphRequest {
        metrics: RequestMetrics {
            x_height: units(style.x_height),
            cap_height: units(style.cap_height),
            ascender: units(style.ascender_height),
            descender: units(style.descender_depth),
            stem: units(style.stem),
            cap_stem: units(style.cap_stem),
            slant_degrees: (style.slant_degrees * 10.0).round() / 10.0,
            contrast: (style.contrast * 100.0).round() / 100.0,
            serif: style.serif,
        },
        examples,
        wanted,
    };
    let mut disclosure = GlyphDisclosure {
        id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
        leaves_device: info.leaves_device,
        destination: info.host.clone(),
        provider_name: info.name.clone(),
        example_glyphs: request.examples.len(),
        wanted: request.wanted.clone(),
        sentence: String::new(),
    };
    disclosure.sentence = disclosure.describe();
    GenerationPlan {
        disclosure,
        request,
    }
}

/// sends the planned request for `chars` (some of those disclosed) and
/// returns the reply text. checks the yes and the provider first.
pub fn run_generation(generation: &Generation, chars: &[char]) -> Result<String, GenerationError> {
    let disclosure = &generation.plan.disclosure;
    let info = generation.generator.describe();
    if info.host != disclosure.destination || info.leaves_device != disclosure.leaves_device {
        return Err(GenerationError::ProviderChanged);
    }
    match generation.acknowledgement {
        Some(ack) if ack.id != disclosure.id => return Err(GenerationError::ConsentMismatch),
        None if disclosure.needs_confirmation() => return Err(GenerationError::ConsentRequired),
        _ => {}
    }
    let permit = GlyphPermit {
        destination: info.host.clone(),
        leaves_device: info.leaves_device,
    };
    let request = generation.plan.request.narrowed(chars);
    if request.wanted.is_empty() {
        return Ok(String::from("{\"glyphs\":[]}"));
    }
    generation.generator.generate(&request, &permit)
}
