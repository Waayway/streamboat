//! `streamboat debug-bundle` (D-029): one zip archive with everything a bug
//! report needs and nothing it must not have. Built by construction, not by
//! filtering: this module only ever reads `<data dir>/logs/` and
//! `<data dir>/crashes/`, plus whatever the caller hands it directly
//! (redacted settings, environment text) — it never walks `<data dir>`
//! itself, so the token file (`tokens.bin`), the offline cache
//! (`offline/`), and the control token file cannot end up in the archive
//! even by accident.
//!
//! `streamboat-core` cannot depend on GStreamer or libmpv (D-004: the core
//! stays engine-agnostic and UI-free), so engine/decoder version strings are
//! the caller's job to gather (`gst::version_string()` in `streamboat-desktop`
//! or `streamboatd`) and pass in as plain text — this module only assembles
//! the archive.

use std::io::Write;
use std::path::Path;

use zip::ZipWriter;
use zip::write::SimpleFileOptions;

use super::redact::redact;
use crate::error::{Error, Result};

/// Everything [`create`] needs beyond the two directories it reads itself.
pub struct BundleInput<'a> {
    /// `streamboat paths`-style text: resolved directories, versions,
    /// engine/decoder info — anything free-form and already safe to show.
    pub environment: &'a str,
    /// `Settings` serialized with every secret/credential field replaced by
    /// whether it is set (`redact_settings` in `streamboat-core::config`, or
    /// the caller's own equivalent) — this module does not re-check that,
    /// it trusts the caller already redacted it.
    pub redacted_settings_json: &'a str,
}

fn add_text_file(zip: &mut ZipWriter<std::fs::File>, name: &str, contents: &str) -> Result<()> {
    let opts = SimpleFileOptions::default();
    zip.start_file(name, opts)
        .map_err(|e| Error::Config(format!("debug bundle: {name}: {e}")))?;
    zip.write_all(contents.as_bytes())
        .map_err(|e| Error::Config(format!("debug bundle: {name}: {e}")))?;
    Ok(())
}

/// Adds every regular file directly under `dir` (non-recursive — logs and
/// crash reports are both flat directories) into the archive under
/// `archive_prefix/<filename>`, redacting each file's text content first.
fn add_redacted_dir(
    zip: &mut ZipWriter<std::fs::File>,
    dir: &Path,
    archive_prefix: &str,
) -> Result<usize> {
    let mut count = 0;
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(e) => return Err(e.into()),
    };
    for entry in entries {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let contents = std::fs::read_to_string(entry.path())?;
        add_text_file(zip, &format!("{archive_prefix}/{name}"), &redact(&contents))?;
        count += 1;
    }
    Ok(count)
}

/// Writes the debug bundle to `out_path` (created, or truncated if it
/// already exists). `data_dir` is only ever read at `logs/` and `crashes/`
/// under it — see the module doc comment.
pub fn create(data_dir: &Path, input: &BundleInput<'_>, out_path: &Path) -> Result<()> {
    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let file = std::fs::File::create(out_path)?;
    let mut zip = ZipWriter::new(file);

    add_text_file(&mut zip, "environment.txt", input.environment)?;
    add_text_file(
        &mut zip,
        "settings.redacted.json",
        input.redacted_settings_json,
    )?;
    add_redacted_dir(&mut zip, &data_dir.join("logs"), "logs")?;
    add_redacted_dir(&mut zip, &data_dir.join("crashes"), "crashes")?;

    zip.finish()
        .map_err(|e| Error::Config(format!("debug bundle: {e}")))?;
    Ok(())
}

/// The archive entry names in `path` (tests, and `streamboat debug-bundle`'s
/// own confirmation printout).
pub fn list_entries(path: &Path) -> Result<Vec<String>> {
    let file = std::fs::File::open(path)?;
    let mut zip =
        zip::ZipArchive::new(file).map_err(|e| Error::Config(format!("debug bundle: {e}")))?;
    Ok((0..zip.len())
        .filter_map(|i| zip.by_index(i).ok().map(|f| f.name().to_string()))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, contents: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }

    #[test]
    fn bundle_contains_logs_crashes_settings_and_environment_but_never_tokens_or_offline() {
        let dir = tempfile::tempdir().unwrap();
        let data_dir = dir.path();

        // A realistic data dir: logs and crash reports the bundle must
        // include, plus the token store and offline cache it must not.
        write(
            &data_dir.join("logs/streamboat.log"),
            "info: Authorization: Bearer super-secret-token\n",
        );
        write(
            &data_dir.join("crashes/crash-1.json"),
            r#"{"message":"panic near Bearer another-secret"}"#,
        );
        write(&data_dir.join("tokens.bin"), "not a real token file");
        write(&data_dir.join("control-token"), "not-a-real-token");
        write(&data_dir.join("offline/index.json"), "{}");
        write(&data_dir.join("offline/chunks/deadbeef"), "ciphertext");

        let input = BundleInput {
            environment: "config: /fake/config\ndata: /fake/data\n",
            redacted_settings_json: r#"{"client_id_set":true,"client_secret_set":false}"#,
        };
        let out = dir.path().join("bundle.zip");
        create(data_dir, &input, &out).unwrap();

        let entries = list_entries(&out).unwrap();
        assert!(entries.contains(&"environment.txt".to_string()));
        assert!(entries.contains(&"settings.redacted.json".to_string()));
        assert!(entries.contains(&"logs/streamboat.log".to_string()));
        assert!(entries.contains(&"crashes/crash-1.json".to_string()));

        // Never the token file, the offline index, or offline chunks.
        assert!(!entries.iter().any(|e| e.contains("tokens.bin")));
        assert!(!entries.iter().any(|e| e.contains("control-token")));
        assert!(!entries.iter().any(|e| e.contains("offline")));

        // The log line's token is redacted inside the archive, not just its name.
        let file = std::fs::File::open(&out).unwrap();
        let mut zip = zip::ZipArchive::new(file).unwrap();
        let mut log_text = String::new();
        std::io::Read::read_to_string(
            &mut zip.by_name("logs/streamboat.log").unwrap(),
            &mut log_text,
        )
        .unwrap();
        assert!(!log_text.contains("super-secret-token"));
        assert!(log_text.contains("<redacted>"));
    }

    #[test]
    fn a_missing_logs_or_crashes_dir_is_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let input = BundleInput {
            environment: "nothing here",
            redacted_settings_json: "{}",
        };
        let out = dir.path().join("bundle.zip");
        // `data_dir` itself exists but has neither `logs/` nor `crashes/`.
        create(dir.path(), &input, &out).unwrap();
        let entries = list_entries(&out).unwrap();
        assert!(entries.contains(&"environment.txt".to_string()));
        assert!(!entries.iter().any(|e| e.starts_with("logs/")));
        assert!(!entries.iter().any(|e| e.starts_with("crashes/")));
    }
}
