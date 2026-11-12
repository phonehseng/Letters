//! what the checks report: one finding per problem, with a sentence, where
//! it is, which clause of the standard it comes from, and whether Letters
//! can fix it.

use serde::{Deserialize, Serialize};

/// whether and how a finding can be fixed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Fix {
    /// `convert_to_pdfa` fixes it by itself
    Automatic,
    /// fixable when a matching font file is supplied
    NeedsFont,
    /// fixable by turning the area into a picture (pdf/a-1 transparency)
    NeedsRendering,
    /// Letters can't fix it; the document needs changing at its source
    NotPossible,
}

/// one problem found.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    /// a short stable id, such as "fonts.notEmbedded"
    pub rule: String,
    /// the clause of the standard, such as "ISO 19005-2 6.2.11.4.1"
    pub clause: String,
    /// a sentence for the person
    pub message: String,
    /// 0-based page, when it is on one
    pub page: Option<u32>,
    /// the object, such as "12 0 R", for people who want to look
    pub object: Option<String>,
    pub fix: Fix,
}

/// collects findings without repeating the same one.
#[derive(Default)]
pub(crate) struct Findings {
    pub list: Vec<Finding>,
}

impl Findings {
    pub fn add(&mut self, rule: &str, clause: &str, message: String, fix: Fix) -> &mut Finding {
        self.list.push(Finding {
            rule: rule.to_owned(),
            clause: clause.to_owned(),
            message,
            page: None,
            object: None,
            fix,
        });
        self.list.last_mut().expect("just pushed")
    }

    /// drops repeats: the same rule, page, object and message.
    pub fn deduplicated(mut self) -> Vec<Finding> {
        let mut seen: Vec<(String, Option<u32>, Option<String>, String)> = Vec::new();
        self.list.retain(|finding| {
            let key = (
                finding.rule.clone(),
                finding.page,
                finding.object.clone(),
                finding.message.clone(),
            );
            let new = !seen.contains(&key);
            if new {
                seen.push(key);
            }
            new
        });
        self.list
    }
}

impl Finding {
    pub fn on_page(&mut self, page: Option<u32>) -> &mut Self {
        self.page = page;
        self
    }

    pub fn at(&mut self, object: Option<lopdf::ObjectId>) -> &mut Self {
        self.object = object.map(crate::objects::object_label);
        self
    }
}
