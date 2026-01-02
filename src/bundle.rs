use axum::{extract::{Path, State}, http::{header, StatusCode}, response::{IntoResponse, Response}};
use bytes::Bytes;
use zip::{ZipWriter, write::{ExtendedFileOptions, FileOptions}};
use std::io::Write;
use crate::config::AppState;
pub async fn get_bundle(State(st): State<AppState>, Path(request_id): Path<String>) -> Response {
  let mut buf: Vec<u8> = vec![];
  {
    let mut zip = ZipWriter::new(std::io::Cursor::new(&mut buf));
    let opts: FileOptions<'_, ExtendedFileOptions> = FileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let _ = zip.start_file("policy_snapshot.json", opts.clone());
    let _ = zip.write_all(serde_json::to_string_pretty(&*st.policy).unwrap_or_default().as_bytes());
    let audit = st.ledger.export_all();
    let _ = zip.start_file("audit_slice.jsonl", opts);
    let mut slice = String::new();
    for line in audit.lines() { if line.contains(&request_id) { slice.push_str(line); slice.push('\n'); } }
    let _ = zip.write_all(slice.as_bytes());
    let _ = zip.finish();
  }
  let bytes = Bytes::from(buf);
  (StatusCode::OK, [(header::CONTENT_TYPE, "application/zip")], bytes).into_response()
}
