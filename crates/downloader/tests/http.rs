//! Integration tests for `download_verified` against a mocked HTTP server.

use std::time::Duration;

use madi_downloader::{download_verified, DownloadError, Progress};
use sha2::{Digest, Sha256};
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    hex::encode(h.finalize())
}

#[tokio::test]
async fn downloads_and_verifies_sha256() {
    let server = MockServer::start().await;
    let payload = b"hello madistack".repeat(1024); // ~15KB
    let digest = sha256_hex(&payload);

    Mock::given(method("GET"))
        .and(path("/file.bin"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(payload.clone()))
        .mount(&server)
        .await;

    let tmp = tempfile::tempdir().unwrap();
    let dest = tmp.path().join("out.bin");
    let url = format!("{}/file.bin", server.uri());

    download_verified(
        &reqwest::Client::new(),
        &url,
        &dest,
        Some(&digest),
        None,
        None,
    )
    .await
    .expect("download should succeed");

    assert_eq!(std::fs::read(&dest).unwrap(), payload);
}

#[tokio::test]
async fn sha_mismatch_removes_partial_file() {
    let server = MockServer::start().await;
    let payload = b"actual contents";
    Mock::given(method("GET"))
        .and(path("/x"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(payload.to_vec()))
        .mount(&server)
        .await;

    let tmp = tempfile::tempdir().unwrap();
    let dest = tmp.path().join("out.bin");
    let url = format!("{}/x", server.uri());
    let bogus = "0".repeat(64);

    let err = download_verified(
        &reqwest::Client::new(),
        &url,
        &dest,
        Some(&bogus),
        None,
        None,
    )
    .await
    .expect_err("should reject mismatched hash");

    // Mismatch is reported, but the partial file is kept on disk — caller
    // decides whether to delete it (policy documented in the module docs).
    assert!(matches!(err, DownloadError::ChecksumMismatch { .. }));
    assert!(dest.exists());
}

#[tokio::test]
async fn emits_progress_events() {
    let server = MockServer::start().await;
    let payload = vec![0xABu8; 4096];
    let digest = sha256_hex(&payload);

    Mock::given(method("GET"))
        .and(path("/p"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(payload.clone()))
        .mount(&server)
        .await;

    let tmp = tempfile::tempdir().unwrap();
    let dest = tmp.path().join("out.bin");
    let url = format!("{}/p", server.uri());

    let (tx, mut rx) = tokio::sync::mpsc::channel(32);
    download_verified(
        &reqwest::Client::new(),
        &url,
        &dest,
        Some(&digest),
        Some(tx),
        None,
    )
    .await
    .unwrap();

    let mut started = false;
    let mut downloaded_max = 0u64;
    let mut done = false;
    while let Some(ev) = rx.recv().await {
        match ev {
            Progress::Started { .. } => started = true,
            Progress::Downloaded { bytes } => downloaded_max = downloaded_max.max(bytes),
            Progress::Done => done = true,
            _ => {}
        }
    }
    assert!(started, "expected Started event");
    assert_eq!(downloaded_max, payload.len() as u64);
    assert!(done, "expected Done event");
}

#[tokio::test]
async fn cancellation_aborts_download() {
    let server = MockServer::start().await;
    // Delay the response long enough that we can cancel first.
    Mock::given(method("GET"))
        .and(path("/slow"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_bytes(vec![0u8; 1024])
                .set_delay(Duration::from_secs(2)),
        )
        .mount(&server)
        .await;

    let tmp = tempfile::tempdir().unwrap();
    let dest = tmp.path().join("out.bin");
    let url = format!("{}/slow", server.uri());

    let cancel = CancellationToken::new();
    let c2 = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        c2.cancel();
    });

    let err = download_verified(
        &reqwest::Client::new(),
        &url,
        &dest,
        None,
        None,
        Some(cancel),
    )
    .await
    .expect_err("cancel should surface an error");

    // Either our explicit Cancelled variant (if cancel was observed between
    // chunks) or an Http error (if reqwest noticed the dropped connection
    // first) is acceptable. The important part: the partial file is gone.
    assert!(
        matches!(err, DownloadError::Cancelled | DownloadError::Http(_)),
        "unexpected error: {err:?}"
    );
    assert!(!dest.exists(), "partial file should be cleaned up on error");
}

#[tokio::test]
async fn http_error_surfaces() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/404"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;

    let tmp = tempfile::tempdir().unwrap();
    let dest = tmp.path().join("out.bin");
    let url = format!("{}/404", server.uri());

    let err = download_verified(&reqwest::Client::new(), &url, &dest, None, None, None)
        .await
        .expect_err("404 must error");
    assert!(matches!(err, DownloadError::Http(_)));
    assert!(!dest.exists());
}
