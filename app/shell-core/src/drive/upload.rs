//! uploading one file to google drive: one "multipart" request holding a
//! small description of the file (its name, the folder, and whether drive
//! should turn it into a google doc) and the file's bytes.

use serde::Deserialize;
use serde_json::json;

use crate::drive::oauth::{encode, Endpoints};

pub const GOOGLE_DOC: &str = "application/vnd.google-apps.document";
pub const FOLDER: &str = "application/vnd.google-apps.folder";

/// what is uploaded and how drive should keep it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UploadRequest {
    pub name: String,
    /// the file's own type ("application/pdf", the docx type...)
    pub content_type: String,
    /// turn a .docx into a native google doc on the way in
    pub as_google_doc: bool,
    pub folder_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Uploaded {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub web_view_link: Option<String>,
}

/// the boundary between the two parts; random enough that it never occurs in a file.
pub fn boundary() -> String {
    format!(
        "letters-{}",
        crate::drive::pkce::new_state()
            .unwrap_or_else(|_| "boundary-7f3a9c".into())
            .replace(['-', '_'], "x")
    )
}

/// the request body: the description as json, then the bytes.
pub fn multipart_body(request: &UploadRequest, bytes: &[u8], boundary: &str) -> Vec<u8> {
    let mut metadata = json!({ "name": request.name });
    if request.as_google_doc {
        metadata["mimeType"] = json!(GOOGLE_DOC);
    }
    if let Some(folder) = &request.folder_id {
        metadata["parents"] = json!([folder]);
    }
    let mut body = Vec::with_capacity(bytes.len() + 512);
    body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Type: application/json; charset=UTF-8\r\n\r\n{metadata}\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Type: {}\r\n\r\n",
            request.content_type
        )
        .as_bytes(),
    );
    body.extend_from_slice(bytes);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    body
}

pub fn upload_url(endpoints: &Endpoints) -> String {
    format!(
        "{}?uploadType=multipart&fields=id,name,webViewLink",
        endpoints.upload
    )
}

pub fn upload(
    endpoints: &Endpoints,
    access_token: &str,
    request: &UploadRequest,
    bytes: &[u8],
) -> Result<Uploaded, String> {
    let boundary = boundary();
    let body = multipart_body(request, bytes, &boundary);
    let mut response = ureq::post(&upload_url(endpoints))
        .header("Authorization", &format!("Bearer {access_token}"))
        .header(
            "Content-Type",
            &format!("multipart/related; boundary={boundary}"),
        )
        .send(&body[..])
        .map_err(|error| format!("The upload to Google Drive failed ({error})."))?;
    let text = response
        .body_mut()
        .read_to_string()
        .map_err(|error| error.to_string())?;
    serde_json::from_str(&text)
        .map_err(|error| format!("Google Drive’s answer couldn’t be read ({error})."))
}

/// the folder Letters made with this name before, or a new one. with the
/// drive.file scope Letters only ever sees folders it made itself.
pub fn find_or_create_folder(
    endpoints: &Endpoints,
    access_token: &str,
    name: &str,
) -> Result<String, String> {
    let query = format!(
        "name = '{}' and mimeType = '{FOLDER}' and trashed = false",
        name.replace('\'', "\\'")
    );
    let url = format!(
        "{}?q={}&fields=files(id,name)",
        endpoints.files,
        encode(&query)
    );
    let mut response = ureq::get(&url)
        .header("Authorization", &format!("Bearer {access_token}"))
        .call()
        .map_err(|error| format!("Google Drive couldn’t be searched ({error})."))?;
    let text = response
        .body_mut()
        .read_to_string()
        .map_err(|error| error.to_string())?;
    #[derive(Deserialize)]
    struct Listing {
        files: Vec<Uploaded>,
    }
    if let Some(found) = serde_json::from_str::<Listing>(&text)
        .ok()
        .and_then(|listing| listing.files.into_iter().next())
    {
        return Ok(found.id);
    }
    let metadata = json!({ "name": name, "mimeType": FOLDER }).to_string();
    let mut created = ureq::post(&format!("{}?fields=id,name", endpoints.files))
        .header("Authorization", &format!("Bearer {access_token}"))
        .header("Content-Type", "application/json")
        .send(metadata.as_bytes())
        .map_err(|error| format!("The folder couldn’t be made in Google Drive ({error})."))?;
    let text = created
        .body_mut()
        .read_to_string()
        .map_err(|error| error.to_string())?;
    let folder: Uploaded = serde_json::from_str(&text).map_err(|error| error.to_string())?;
    Ok(folder.id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_body_holds_the_description_then_the_bytes() {
        let request = UploadRequest {
            name: "Essay.docx".into(),
            content_type: "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
                .into(),
            as_google_doc: true,
            folder_id: Some("folder1".into()),
        };
        let body = String::from_utf8(multipart_body(&request, b"PK-bytes", "b0")).unwrap();
        assert!(body.starts_with("--b0\r\nContent-Type: application/json; charset=UTF-8\r\n\r\n{"));
        assert!(body.contains(r#""mimeType":"application/vnd.google-apps.document""#));
        assert!(body.contains(r#""parents":["folder1"]"#));
        assert!(body.contains("\r\n--b0\r\nContent-Type: application/vnd.openxmlformats-officedocument.wordprocessingml.document\r\n\r\nPK-bytes\r\n--b0--\r\n"));
    }

    #[test]
    fn a_plain_upload_keeps_its_own_type() {
        let request = UploadRequest {
            name: "Essay.pdf".into(),
            content_type: "application/pdf".into(),
            as_google_doc: false,
            folder_id: None,
        };
        let body = String::from_utf8(multipart_body(&request, b"%PDF", "b")).unwrap();
        assert!(!body.contains("mimeType"));
        assert!(!body.contains("parents"));
        assert!(upload_url(&Endpoints::google())
            .ends_with("uploadType=multipart&fields=id,name,webViewLink"));
    }
}
