//! the locked layout mode must stay byte for byte what round one wrote.
//!
//! `tests/corpus/locked-hashes.json` holds a 64-bit fnv-1a hash of every
//! corpus docx as round one wrote it without word (the built-in tables,
//! no checking export, so the bytes depend on nothing outside the crate).
//! set LETTERS_EXACT_RECORD=1 to write the file again (only ever from a
//! round-one build).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use letters_exact::{convert, ConvertOptions, OutputMode};

const DOCUMENTS: [&str; 13] = [
    "syllabus",
    "research_paper",
    "lab_report",
    "slides",
    "form",
    "journal",
    "cid_subset",
    "type3_font",
    "mixed_styles",
    "tj_kerning",
    "cff_font",
    "type1_font",
    "restricted_font",
];

fn corpus() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("corpus")
}

fn fnv1a(bytes: &[u8]) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

/// the docx bytes of one corpus document in locked layout mode, without word.
fn locked_docx(name: &str) -> Vec<u8> {
    let out = std::env::temp_dir().join(format!(
        "letters-exact-locked-{}-{name}.docx",
        std::process::id()
    ));
    let options = ConvertOptions {
        use_word: false,
        mode: OutputMode::LockedLayout,
        ..ConvertOptions::default()
    };
    convert(&corpus().join(format!("{name}.pdf")), &out, &options).unwrap();
    let bytes = std::fs::read(&out).unwrap();
    let _ = std::fs::remove_file(&out);
    bytes
}

#[test]
fn locked_layout_is_byte_for_byte_round_one() {
    let path = corpus().join("locked-hashes.json");
    let hashes: BTreeMap<String, String> = DOCUMENTS
        .iter()
        .map(|name| (name.to_string(), fnv1a(&locked_docx(name))))
        .collect();
    if std::env::var("LETTERS_EXACT_RECORD").is_ok() {
        std::fs::write(&path, serde_json::to_string_pretty(&hashes).unwrap()).unwrap();
        return;
    }
    let recorded: BTreeMap<String, String> =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let differing: Vec<&String> = hashes
        .iter()
        .filter(|(name, hash)| recorded.get(*name) != Some(hash))
        .map(|(name, _)| name)
        .collect();
    assert!(
        differing.is_empty(),
        "locked layout output changed for {differing:?}"
    );
}
