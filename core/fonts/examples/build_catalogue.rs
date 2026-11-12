//! builds the online library's catalogue from the google/fonts repository
//! at one pinned commit. run it through tools/build-font-catalogue.ps1
//! (or .sh); the result is core/fonts/data/catalogue.z, shipped with the
//! app so searching the library works offline.
//!
//!   cargo run -p letters-fonts --example build_catalogue -- <commit> <out file> [cache folder]
//!
//! it asks github once for the commit's file list (names, sizes and git
//! blob hashes) and then reads every family's METADATA.pb from
//! raw.githubusercontent.com. font files are never downloaded here: the
//! app checks a downloaded file against the size and hash recorded now.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// one file of the commit, from github's tree listing.
#[derive(Clone)]
struct Blob {
    sha: String,
    size: u64,
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(60)))
        .user_agent("letters-font-catalogue-builder")
        .build()
        .into()
}

fn get_text(agent: &ureq::Agent, url: &str) -> Result<String, String> {
    let mut response = agent
        .get(url)
        .call()
        .map_err(|error| format!("{url}: {error}"))?;
    let mut text = String::new();
    response
        .body_mut()
        .with_config()
        .limit(64 * 1024 * 1024)
        .reader()
        .read_to_string(&mut text)
        .map_err(|error| format!("{url}: {error}"))?;
    Ok(text)
}

fn main() {
    let arguments: Vec<String> = std::env::args().collect();
    if arguments.len() < 3 {
        eprintln!("usage: build_catalogue <commit> <out file> [cache folder]");
        std::process::exit(2);
    }
    let commit = &arguments[1];
    let out = PathBuf::from(&arguments[2]);
    let cache = arguments
        .get(3)
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("letters-font-catalogue"));
    std::fs::create_dir_all(&cache).expect("the cache folder can be made");
    let agent = agent();

    let tree = load_tree(&agent, commit, &cache);
    let blobs = blobs_of(&tree);
    let mut family_dirs: Vec<String> = blobs
        .keys()
        .filter_map(|path| path.strip_suffix("/METADATA.pb"))
        .filter(|dir| {
            dir.starts_with("ofl/") || dir.starts_with("apache/") || dir.starts_with("ufl/")
        })
        .map(str::to_string)
        .collect();
    family_dirs.sort();
    eprintln!("{} families at {commit}", family_dirs.len());

    let metadata = fetch_all_metadata(&agent, commit, &family_dirs, &cache);
    let mut lines = vec![format!("letters-font-catalogue\t1\t{commit}")];
    let mut families = 0;
    for dir in &family_dirs {
        let Some(text) = metadata.get(dir) else {
            continue;
        };
        if let Some(family_lines) = family_lines(dir, text, &blobs) {
            lines.extend(family_lines);
            families += 1;
        }
    }
    let text = lines.join("\n");
    let packed = miniz_oxide::deflate::compress_to_vec_zlib(text.as_bytes(), 9);
    std::fs::write(&out, &packed).expect("the catalogue can be written");
    eprintln!(
        "wrote {families} families, {} kB of text, {} kB packed, to {}",
        text.len() / 1024,
        packed.len() / 1024,
        out.display()
    );
}

fn load_tree(agent: &ureq::Agent, commit: &str, cache: &Path) -> serde_json::Value {
    let saved = cache.join(format!("tree-{commit}.json"));
    let text = std::fs::read_to_string(&saved).unwrap_or_else(|_| {
        let url =
            format!("https://api.github.com/repos/google/fonts/git/trees/{commit}?recursive=1");
        let text = get_text(agent, &url).expect("github lists the commit's files");
        std::fs::write(&saved, &text).ok();
        text
    });
    let tree: serde_json::Value = serde_json::from_str(&text).expect("the file list is json");
    assert_eq!(tree["truncated"], false, "github cut the file list short");
    tree
}

fn blobs_of(tree: &serde_json::Value) -> HashMap<String, Blob> {
    tree["tree"]
        .as_array()
        .expect("a tree")
        .iter()
        .filter(|entry| entry["type"] == "blob")
        .filter_map(|entry| {
            Some((
                entry["path"].as_str()?.to_string(),
                Blob {
                    sha: entry["sha"].as_str()?.to_string(),
                    size: entry["size"].as_u64()?,
                },
            ))
        })
        .collect()
}

/// every METADATA.pb, eight at a time, kept in the cache folder so a
/// second run needs no network.
fn fetch_all_metadata(
    agent: &ureq::Agent,
    commit: &str,
    dirs: &[String],
    cache: &Path,
) -> HashMap<String, String> {
    let queue = Arc::new(Mutex::new(dirs.to_vec()));
    let found = Arc::new(Mutex::new(HashMap::new()));
    let folder = cache.join(format!("metadata-{commit}"));
    std::fs::create_dir_all(&folder).ok();
    let workers: Vec<_> = (0..8)
        .map(|_| {
            let (queue, found, folder, agent, commit) = (
                queue.clone(),
                found.clone(),
                folder.clone(),
                agent.clone(),
                commit.to_string(),
            );
            std::thread::spawn(move || loop {
                let Some(dir) = queue.lock().unwrap().pop() else {
                    break;
                };
                let saved = folder.join(dir.replace('/', "__"));
                let text = std::fs::read_to_string(&saved).ok().or_else(|| {
                    let url = format!(
                        "https://raw.githubusercontent.com/google/fonts/{commit}/{dir}/METADATA.pb"
                    );
                    let text = get_text(&agent, &url)
                        .map_err(|error| eprintln!("{error}"))
                        .ok()?;
                    std::fs::write(&saved, &text).ok();
                    Some(text)
                });
                if let Some(text) = text {
                    found.lock().unwrap().insert(dir, text);
                }
            })
        })
        .collect();
    for worker in workers {
        worker.join().ok();
    }
    Arc::try_unwrap(found).unwrap().into_inner().unwrap()
}

/// one parsed `key: value` or `key { ... }` of a text protobuf.
#[derive(Debug)]
enum Field {
    Value(String, String),
    Message(String, Vec<Field>),
}

/// a small reader for the text protobuf METADATA.pb uses: names, quoted
/// strings with escapes, numbers and nested messages.
fn parse_proto(text: &str) -> Vec<Field> {
    let tokens = tokenize(text);
    let mut at = 0;
    parse_fields(&tokens, &mut at)
}

fn tokenize(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
        } else if c == '#' {
            while chars.next().is_some_and(|c| c != '\n') {}
        } else if c == '{' || c == '}' || c == ':' {
            tokens.push(c.to_string());
            chars.next();
        } else if c == '"' || c == '\'' {
            chars.next();
            let mut value = String::from("\"");
            let mut bytes = Vec::new();
            while let Some(next) = chars.next() {
                match next {
                    '\\' => match chars.next() {
                        Some('n') => bytes.push(b'\n'),
                        Some('t') => bytes.push(b'\t'),
                        Some(digit @ '0'..='7') => {
                            // octal escapes carry utf-8 bytes
                            let mut number = digit.to_digit(8).unwrap();
                            for _ in 0..2 {
                                if let Some(more) = chars.peek().and_then(|d| d.to_digit(8)) {
                                    number = number * 8 + more;
                                    chars.next();
                                }
                            }
                            bytes.push(number as u8);
                        }
                        Some(other) => {
                            let mut buffer = [0; 4];
                            bytes.extend_from_slice(other.encode_utf8(&mut buffer).as_bytes());
                        }
                        None => break,
                    },
                    end if end == c => break,
                    other => {
                        let mut buffer = [0; 4];
                        bytes.extend_from_slice(other.encode_utf8(&mut buffer).as_bytes());
                    }
                }
            }
            value.push_str(&String::from_utf8_lossy(&bytes));
            tokens.push(value);
        } else {
            let mut word = String::new();
            while let Some(&next) = chars.peek() {
                if next.is_whitespace() || "{}:\"'".contains(next) {
                    break;
                }
                word.push(next);
                chars.next();
            }
            tokens.push(word);
        }
    }
    tokens
}

fn parse_fields(tokens: &[String], at: &mut usize) -> Vec<Field> {
    let mut fields = Vec::new();
    while *at < tokens.len() && tokens[*at] != "}" {
        let name = tokens[*at].clone();
        *at += 1;
        if tokens.get(*at).is_some_and(|token| token == ":") {
            *at += 1;
        }
        if tokens.get(*at).is_some_and(|token| token == "{") {
            *at += 1;
            let inner = parse_fields(tokens, at);
            *at += 1; // the closing brace
            fields.push(Field::Message(name, inner));
        } else if let Some(value) = tokens.get(*at) {
            fields.push(Field::Value(
                name,
                value.trim_start_matches('"').to_string(),
            ));
            *at += 1;
        }
    }
    fields
}

fn values<'a>(fields: &'a [Field], key: &str) -> Vec<&'a str> {
    fields
        .iter()
        .filter_map(|field| match field {
            Field::Value(name, value) if name == key => Some(value.as_str()),
            _ => None,
        })
        .collect()
}

fn value<'a>(fields: &'a [Field], key: &str) -> &'a str {
    values(fields, key).first().copied().unwrap_or("")
}

fn messages<'a>(fields: &'a [Field], key: &str) -> Vec<&'a [Field]> {
    fields
        .iter()
        .filter_map(|field| match field {
            Field::Message(name, inner) if name == key => Some(inner.as_slice()),
            _ => None,
        })
        .collect()
}

/// tabs and line breaks would break the line format.
fn clean(text: &str) -> String {
    text.replace(['\t', '\n', '\r'], " ").trim().to_string()
}

/// the catalogue lines of one family, or None when it has no usable files.
fn family_lines(dir: &str, text: &str, blobs: &HashMap<String, Blob>) -> Option<Vec<String>> {
    let fields = parse_proto(text);
    let name = clean(value(&fields, "name"));
    if name.is_empty() {
        return None;
    }
    let categories = values(&fields, "category").join(",");
    let subsets: Vec<&str> = values(&fields, "subsets")
        .into_iter()
        .filter(|s| *s != "menu")
        .collect();
    let axes: Vec<String> = messages(&fields, "axes")
        .into_iter()
        .map(|axis| {
            format!(
                "{}:{}:{}",
                value(axis, "tag"),
                value(axis, "min_value"),
                value(axis, "max_value")
            )
        })
        .collect();
    let mut lines = vec![format!(
        "F\t{dir}\t{name}\t{}\t{}\t{categories}\t{}\t{}\t{}\t{}",
        clean(value(&fields, "designer")),
        value(&fields, "license"),
        value(&fields, "date_added"),
        subsets.join(","),
        axes.join(","),
        value(&fields, "primary_script"),
    )];
    for license in ["OFL.txt", "LICENSE.txt", "UFL.txt"] {
        if let Some(blob) = blobs.get(&format!("{dir}/{license}")) {
            lines.push(format!("L\t{license}\t{}\t{}", blob.sha, blob.size));
            break;
        }
    }
    let mut files = 0;
    for font in messages(&fields, "fonts") {
        let filename = value(font, "filename");
        let Some(blob) = blobs.get(&format!("{dir}/{filename}")) else {
            continue;
        };
        lines.push(format!(
            "S\t{filename}\t{}\t{}\t{}\t{}\t{}\t{}",
            value(font, "style"),
            value(font, "weight"),
            blob.sha,
            blob.size,
            clean(value(font, "post_script_name")),
            clean(value(font, "full_name")),
        ));
        files += 1;
    }
    (files > 0).then_some(lines)
}
