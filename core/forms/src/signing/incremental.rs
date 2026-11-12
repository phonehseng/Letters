//! saving changes as an incremental update.
//!
//! a signature covers the exact bytes of the file as it was signed. to
//! add something later (another signature, say) without breaking it, the
//! old bytes are kept exactly as they are and the changed objects are
//! written after them, with a new cross-reference section that points
//! back to the old one (/Prev). readers take the newest copy of each object.

use std::collections::BTreeMap;

use lopdf::{Document, Object, ObjectId, StringFormat};
use sha2::{Digest, Sha256};

use crate::error::{FormError, FormResult};

/// an object whose bytes are written exactly as given, so their place in
/// the file can be found afterwards (the signature dictionary).
pub struct HandWritten {
    pub id: ObjectId,
    pub body: Vec<u8>,
}

/// the original file plus the new objects, and where each object starts.
pub struct Update {
    pub bytes: Vec<u8>,
    pub offsets: BTreeMap<ObjectId, usize>,
}

/// appends every object that differs between `before` and `after` to the
/// original bytes, followed by a cross-reference section and trailer.
pub fn write_update(
    original: &[u8],
    before: &Document,
    after: &Document,
    hand_written: &[HandWritten],
) -> FormResult<Update> {
    let previous_xref = find_startxref(original)
        .ok_or_else(|| FormError::BadPdf("the file has no startxref".into()))?;
    let uses_xref_stream = !original
        .get(previous_xref..)
        .map(|rest| rest.starts_with(b"xref"))
        .unwrap_or(false);

    let mut changed: BTreeMap<ObjectId, Option<&Object>> = BTreeMap::new();
    for (id, object) in &after.objects {
        if before.objects.get(id) != Some(object) {
            changed.insert(*id, Some(object));
        }
    }
    for written in hand_written {
        changed.insert(written.id, None);
    }

    let mut bytes = original.to_vec();
    if !bytes.ends_with(b"\n") {
        bytes.push(b'\n');
    }
    let mut offsets = BTreeMap::new();
    for (id, object) in &changed {
        offsets.insert(*id, bytes.len());
        bytes.extend_from_slice(format!("{} {} obj\n", id.0, id.1).as_bytes());
        match object {
            Some(object) => write_object(&mut bytes, object)?,
            None => {
                let written = hand_written
                    .iter()
                    .find(|written| written.id == *id)
                    .unwrap();
                bytes.extend_from_slice(&written.body);
            }
        }
        bytes.extend_from_slice(b"\nendobj\n");
    }

    let highest = changed.keys().map(|id| id.0).max().unwrap_or(0);
    let previous_size = before
        .trailer
        .get(b"Size")
        .ok()
        .and_then(|size| size.as_i64().ok())
        .unwrap_or(0) as u32;
    let size = previous_size.max(highest + 1).max(after.max_id + 1);
    let trailer = trailer_entries(after, size, previous_xref);

    let xref_start = bytes.len();
    if uses_xref_stream {
        write_xref_stream(&mut bytes, &mut offsets, size, &trailer)?;
    } else {
        write_xref_table(&mut bytes, &offsets, &trailer);
    }
    bytes.extend_from_slice(format!("startxref\n{xref_start}\n%%EOF\n").as_bytes());
    Ok(Update { bytes, offsets })
}

fn write_object(bytes: &mut Vec<u8>, object: &Object) -> FormResult<()> {
    let mut object = object.clone();
    // the length must match the bytes actually written
    if let Object::Stream(stream) = &mut object {
        let length = stream.content.len() as i64;
        stream.dict.set("Length", Object::Integer(length));
    }
    lopdf::Writer::write_object(bytes, &object)
        .map_err(|error| FormError::SigningFailed(error.to_string()))
}

/// where the last cross-reference section starts, from the file's end.
pub fn find_startxref(bytes: &[u8]) -> Option<usize> {
    let tail_start = bytes.len().saturating_sub(2048);
    let tail = &bytes[tail_start..];
    let position = tail.windows(9).rposition(|window| window == b"startxref")?;
    let after: String = tail[position + 9..]
        .iter()
        .map(|&byte| byte as char)
        .skip_while(|c| c.is_whitespace())
        .take_while(char::is_ascii_digit)
        .collect();
    after.parse().ok()
}

/// the trailer of the new section: the old trailer's root, info and first
/// id, a fresh second id, the new size and a pointer to the old section.
fn trailer_entries(after: &Document, size: u32, previous_xref: usize) -> Vec<(String, Vec<u8>)> {
    let mut entries = vec![("Size".to_owned(), size.to_string().into_bytes())];
    for key in ["Root", "Info"] {
        if let Ok(Object::Reference(id)) = after.trailer.get(key.as_bytes()) {
            entries.push((key.to_owned(), format!("{} {} R", id.0, id.1).into_bytes()));
        }
    }
    let first_id = match after.trailer.get(b"ID") {
        Ok(Object::Array(ids)) => ids
            .first()
            .and_then(|id| id.as_str().ok())
            .map(<[u8]>::to_vec),
        _ => None,
    };
    let fresh: Vec<u8> = Sha256::digest(
        format!("{previous_xref}-{size}-{:?}", std::time::SystemTime::now()).as_bytes(),
    )[..16]
        .to_vec();
    let first_id = first_id.unwrap_or_else(|| fresh.clone());
    let mut id_bytes = Vec::new();
    let id_array = Object::Array(vec![
        Object::String(first_id, StringFormat::Hexadecimal),
        Object::String(fresh, StringFormat::Hexadecimal),
    ]);
    let _ = lopdf::Writer::write_object(&mut id_bytes, &id_array);
    entries.push(("ID".to_owned(), id_bytes));
    entries.push(("Prev".to_owned(), previous_xref.to_string().into_bytes()));
    entries
}

/// consecutive object numbers, grouped as cross-reference subsections.
fn runs(numbers: &[u32]) -> Vec<(u32, u32)> {
    let mut runs: Vec<(u32, u32)> = Vec::new();
    for &number in numbers {
        match runs.last_mut() {
            Some((start, count)) if *start + *count == number => *count += 1,
            _ => runs.push((number, 1)),
        }
    }
    runs
}

fn write_xref_table(
    bytes: &mut Vec<u8>,
    offsets: &BTreeMap<ObjectId, usize>,
    trailer: &[(String, Vec<u8>)],
) {
    let numbers: Vec<u32> = offsets.keys().map(|id| id.0).collect();
    let generation = |number: u32| {
        offsets
            .keys()
            .find(|id| id.0 == number)
            .map(|id| id.1)
            .unwrap_or(0)
    };
    let offset_of = |number: u32| {
        offsets
            .iter()
            .find(|(id, _)| id.0 == number)
            .map(|(_, offset)| *offset)
            .unwrap_or(0)
    };
    bytes.extend_from_slice(b"xref\n");
    for (start, count) in runs(&numbers) {
        bytes.extend_from_slice(format!("{start} {count}\n").as_bytes());
        for number in start..start + count {
            bytes.extend_from_slice(
                format!("{:010} {:05} n \n", offset_of(number), generation(number)).as_bytes(),
            );
        }
    }
    bytes.extend_from_slice(b"trailer\n<<");
    for (key, value) in trailer {
        bytes.extend_from_slice(format!(" /{key} ").as_bytes());
        bytes.extend_from_slice(value);
    }
    bytes.extend_from_slice(b" >>\n");
}

/// files whose last section is a cross-reference stream get one too,
/// since older readers can't mix the two kinds.
fn write_xref_stream(
    bytes: &mut Vec<u8>,
    offsets: &mut BTreeMap<ObjectId, usize>,
    size: u32,
    trailer: &[(String, Vec<u8>)],
) -> FormResult<()> {
    // the stream's own entry points at where it begins
    let own_number = size;
    let start = bytes.len();
    offsets.insert((own_number, 0), start);
    let numbers: Vec<u32> = offsets.keys().map(|id| id.0).collect();
    let mut data = Vec::new();
    for (id, offset) in offsets.iter() {
        let offset = u32::try_from(*offset)
            .map_err(|_| FormError::SigningFailed("the file is too large".into()))?;
        data.push(1u8);
        data.extend_from_slice(&offset.to_be_bytes());
        data.extend_from_slice(&id.1.to_be_bytes());
    }
    let index: Vec<String> = runs(&numbers)
        .iter()
        .map(|(first, count)| format!("{first} {count}"))
        .collect();
    let mut dict = format!(
        "<< /Type /XRef /W [1 4 2] /Index [{}] /Length {}",
        index.join(" "),
        data.len()
    );
    for (key, value) in trailer {
        let value = if key == "Size" {
            (size + 1).to_string()
        } else {
            String::from_utf8_lossy(value).into_owned()
        };
        dict.push_str(&format!(" /{key} {value}"));
    }
    dict.push_str(" >>");
    bytes.extend_from_slice(format!("{own_number} 0 obj\n{dict}\nstream\n").as_bytes());
    bytes.extend_from_slice(&data);
    bytes.extend_from_slice(b"\nendstream\nendobj\n");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_last_startxref() {
        let file = b"%PDF-1.7\n...startxref\n10\n%%EOF\n...startxref\r\n  1234\r\n%%EOF\r\n";
        assert_eq!(find_startxref(file), Some(1234));
    }

    #[test]
    fn groups_object_numbers_into_runs() {
        assert_eq!(runs(&[3, 4, 5, 9, 11, 12]), vec![(3, 3), (9, 1), (11, 2)]);
    }
}
