//! Offline smoke test for the phpMyAdmin extraction shape.
//!
//! The upstream zip ships every entry under a single versioned top folder
//! (`phpMyAdmin-5.2.1-all-languages/...`). Our extractor is supposed to strip
//! that prefix so files land directly under `bin/phpmyadmin/`, which is what
//! the nginx alias in `templates/nginx.conf.tera` points at.
//!
//! Running the real download in CI is not viable (~15 MB, network flake), so
//! we build a minimally-faithful zip in-memory and assert the post-extract
//! layout matches what the nginx alias and the DB-bootstrap step expect.

use std::io::{Cursor, Write};

use madi_downloader::extract_zip;
use zip::{write::SimpleFileOptions, CompressionMethod, ZipWriter};

const TOP: &str = "phpMyAdmin-5.2.1-all-languages";

fn make_phpmyadmin_zip() -> Vec<u8> {
    let mut buf = Vec::new();
    {
        let cursor = Cursor::new(&mut buf);
        let mut zw = ZipWriter::new(cursor);
        let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);

        let dirs = [
            "",
            "sql/",
            "libraries/",
            "themes/pmahomme/",
            "js/",
            "locale/pt_BR/LC_MESSAGES/",
        ];
        for d in dirs {
            zw.add_directory(format!("{TOP}/{d}"), opts).unwrap();
        }

        let files: &[(&str, &[u8])] = &[
            ("index.php", b"<?php // pma entry"),
            ("config.sample.inc.php", b"<?php // sample"),
            ("README", b"phpMyAdmin"),
            ("sql/create_tables.sql", b"-- create tables"),
            ("libraries/common.inc.php", b"<?php // common"),
            ("themes/pmahomme/info.json", b"{}"),
        ];
        for (path, content) in files {
            zw.start_file(format!("{TOP}/{path}"), opts).unwrap();
            zw.write_all(content).unwrap();
        }
        zw.finish().unwrap();
    }
    buf
}

#[tokio::test]
async fn extracts_phpmyadmin_into_flat_layout() {
    let tmp = tempfile::tempdir().unwrap();
    let zip_path = tmp.path().join("phpmyadmin.zip");
    std::fs::write(&zip_path, make_phpmyadmin_zip()).unwrap();

    let target = tmp.path().join("bin").join("phpmyadmin");
    extract_zip(&zip_path, &target).await.unwrap();

    // Top folder must be stripped — these are the paths the nginx alias and
    // the phpmyadmin DB bootstrap rely on.
    assert!(target.join("index.php").is_file(), "index.php missing");
    assert!(
        target.join("sql/create_tables.sql").is_file(),
        "sql/create_tables.sql missing — DB bootstrap would fail"
    );
    assert!(
        target.join("libraries/common.inc.php").is_file(),
        "libraries/ tree missing"
    );
    assert!(
        target.join("themes/pmahomme/info.json").is_file(),
        "themes/ tree missing"
    );
    assert!(
        !target.join(TOP).exists(),
        "versioned top folder should have been stripped"
    );
}
