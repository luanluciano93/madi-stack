//! Synchronous zip extraction helpers. Called from `spawn_blocking`.

use std::{fs, io, path::Path};

use zip::ZipArchive;

use crate::{find_common_top_prefix, safe_join, DownloadResult};

pub fn extract_zip_sync(zip_path: &Path, target_dir: &Path) -> DownloadResult<()> {
    let file = fs::File::open(zip_path)?;
    let mut archive = ZipArchive::new(file)?;

    let prefix = find_common_top_prefix(&mut archive);
    fs::create_dir_all(target_dir)?;

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        extract_one(&mut entry, prefix.as_deref(), target_dir)?;
    }

    Ok(())
}

fn extract_one(
    entry: &mut zip::read::ZipFile<'_>,
    common_prefix: Option<&str>,
    target_dir: &Path,
) -> DownloadResult<()> {
    let raw_name = entry.name().replace('\\', "/");
    let rel = match common_prefix {
        Some(p) => raw_name.strip_prefix(p).unwrap_or(&raw_name),
        None => &raw_name,
    };
    if rel.is_empty() {
        return Ok(());
    }

    let out_path = safe_join(target_dir, rel)?;

    if entry.is_dir() {
        fs::create_dir_all(&out_path)?;
        return Ok(());
    }

    if let Some(parent) = out_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut out = fs::File::create(&out_path)?;
    io::copy(entry, &mut out)?;

    // Preserve Unix permissions if the zip carried them (useful for the
    // `.exe` bit on WSL/wine — a no-op on native Windows, which ignores
    // permissions, but harmless to set).
    #[cfg(unix)]
    if let Some(mode) = entry.unix_mode() {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&out_path, fs::Permissions::from_mode(mode))?;
    }

    // On Windows, the drop of the std::fs::File flushes via CloseHandle.
    drop(out);
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Write};

    use zip::{write::SimpleFileOptions, CompressionMethod, ZipWriter};

    use super::*;
    use crate::DownloadError;

    /// Build an in-memory zip with the given entries (path, content pairs).
    /// Directory entries use an empty `contents` and a trailing `/` in path.
    fn make_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut buf = Vec::new();
        {
            let cursor = Cursor::new(&mut buf);
            let mut zw = ZipWriter::new(cursor);
            let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
            for (path, content) in entries {
                if path.ends_with('/') {
                    zw.add_directory(*path, opts).unwrap();
                } else {
                    zw.start_file(*path, opts).unwrap();
                    zw.write_all(content).unwrap();
                }
            }
            zw.finish().unwrap();
        }
        buf
    }

    /// Write `bytes` into a fresh tempdir managed by the `tempfile` crate
    /// — PID + atomic counter + random bytes, safe against parallel
    /// `cargo test` runs on platforms with coarse system clocks (Windows'
    /// ~16ms tick resolution used to cause collisions with a homegrown
    /// nanosecond-based id). The `TempDir` handle is returned so the
    /// directory lives until the test finishes.
    fn write_tmp_zip(bytes: &[u8]) -> TempZip {
        let dir = tempfile::tempdir().unwrap();
        let zip = dir.path().join("test.zip");
        std::fs::write(&zip, bytes).unwrap();
        TempZip { dir, zip }
    }

    struct TempZip {
        dir: tempfile::TempDir,
        zip: std::path::PathBuf,
    }

    impl TempZip {
        fn dir(&self) -> &std::path::Path {
            self.dir.path()
        }
    }

    #[test]
    fn strips_common_top_folder() {
        let bytes = make_zip(&[
            ("nginx-1.29.8/", b""),
            ("nginx-1.29.8/nginx.exe", b"FAKE-EXE"),
            ("nginx-1.29.8/conf/", b""),
            ("nginx-1.29.8/conf/mime.types", b"types"),
        ]);
        let tmp = write_tmp_zip(&bytes);
        let out = tmp.dir().join("extracted");

        extract_zip_sync(&tmp.zip, &out).unwrap();

        assert!(out.join("nginx.exe").exists());
        assert!(out.join("conf/mime.types").exists());
        assert!(!out.join("nginx-1.29.8").exists());
    }

    #[test]
    fn keeps_layout_when_no_common_prefix() {
        let bytes = make_zip(&[
            ("php.exe", b"EXE"),
            ("php.ini", b"INI"),
            ("ext/opcache.dll", b"DLL"),
        ]);
        let tmp = write_tmp_zip(&bytes);
        let out = tmp.dir().join("extracted");

        extract_zip_sync(&tmp.zip, &out).unwrap();

        assert!(out.join("php.exe").exists());
        assert!(out.join("php.ini").exists());
        assert!(out.join("ext/opcache.dll").exists());
    }

    #[test]
    fn rejects_zip_slip() {
        let bytes = make_zip(&[("../../evil.exe", b"bad")]);
        let tmp = write_tmp_zip(&bytes);
        let out = tmp.dir().join("extracted");
        let err = extract_zip_sync(&tmp.zip, &out).unwrap_err();
        assert!(matches!(err, DownloadError::UnsafePath(_)));
    }
}
