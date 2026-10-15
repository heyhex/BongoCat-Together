use super::*;
use serde_json::{Value, json};

const WINDOWS: UpdateTargetTriple = UpdateTargetTriple::X86_64PcWindowsMsvc;

fn release(version: &str) -> Value {
    let tag = format!("together-v{version}");
    let name = format!("BongoCat-Together_{version}_x64.exe");
    json!({
        "tag_name": tag, "draft": false, "prerelease": true,
        "html_url": format!("{}/releases/tag/{tag}", repository_url()),
        "body": "Together changes",
        "assets": [{"name": name,
            "browser_download_url": format!("{}/releases/download/{tag}/{name}", repository_url())}]
    })
}

fn select(values: Vec<Value>, current: &str) -> Result<UpdateOutcome, UpdateError> {
    let bytes = serde_json::to_vec(&values).unwrap();
    select_release(&read_page(bytes.as_slice())?, current, WINDOWS)
}

#[test]
fn prereleases_use_semver_instead_of_publish_order_or_lexical_order() {
    let result = select(
        vec![release("0.2.0"), release("0.10.0"), release("0.3.0")],
        "0.1.0",
    )
    .unwrap();
    assert!(
        matches!(result, UpdateOutcome::Available { release } if release.version == "0.10.0" && release.notes.as_deref() == Some("Together changes"))
    );
    assert!(matches!(
        select(vec![release("0.2.0-beta.2")], "0.2.0-beta.1").unwrap(),
        UpdateOutcome::Available { .. }
    ));
    assert!(matches!(
        select(vec![release("0.2.0-beta.2")], "0.2.0").unwrap(),
        UpdateOutcome::UpToDate
    ));
}

#[test]
fn same_older_and_build_metadata_versions_do_not_offer_a_downgrade() {
    for current in ["0.1.0", "0.2.0", "0.1.0+local"] {
        assert!(matches!(
            select(vec![release("0.1.0")], current).unwrap(),
            UpdateOutcome::UpToDate
        ));
    }
}

#[test]
fn drafts_upstream_and_dated_tags_cannot_offer_an_update() {
    let mut values = vec![release("0.1.0")];
    for tag in [
        "v99.0.0",
        "together-test-2026-10-15",
        "together-v01.0.0",
        "together-vjunk",
    ] {
        let mut value = release("99.0.0");
        value["tag_name"] = tag.into();
        values.push(value);
    }
    let mut draft = release("100.0.0");
    draft["draft"] = true.into();
    values.push(draft);
    assert!(matches!(
        select(values, "0.1.0").unwrap(),
        UpdateOutcome::UpToDate
    ));
}

#[test]
fn release_and_asset_urls_must_belong_to_the_exact_together_release() {
    for url in [
        "http://github.com/heyhex/BongoCat-Together/releases/tag/together-v0.2.0",
        "https://github.com/ayangweb/BongoCat/releases/tag/v0.2.0",
        "https://evil.invalid/releases/tag/together-v0.2.0",
    ] {
        let mut value = release("0.2.0");
        value["html_url"] = url.into();
        assert_eq!(
            select(vec![value], "0.1.0").unwrap_err().code(),
            UpdateErrorCode::ReleaseManifestInvalid
        );
    }
    for url in [
        "https://github.com/ayangweb/BongoCat/releases/download/v0.2.0/a.exe",
        "https://evil.invalid/a.exe",
        "http://github.com/a.exe",
    ] {
        let mut value = release("0.2.0");
        value["assets"][0]["browser_download_url"] = url.into();
        assert_eq!(
            select(vec![value], "0.1.0").unwrap_err().code(),
            UpdateErrorCode::NoMatchingAsset
        );
    }
    assert!(TogetherReleaseChecker::release_page_url("../../other").is_none());
}

#[test]
fn newest_release_missing_its_installer_is_an_error_even_if_older_one_has_it() {
    let mut newest = release("0.3.0");
    newest["assets"] = json!([]);
    assert_eq!(
        select(vec![release("0.2.0"), newest], "0.1.0")
            .unwrap_err()
            .code(),
        UpdateErrorCode::NoMatchingAsset
    );
    let mut wrong_arch = release("0.2.0");
    wrong_arch["assets"][0]["name"] = "BongoCat-Together_0.2.0_arm64.exe".into();
    assert_eq!(
        select(vec![wrong_arch], "0.1.0").unwrap_err().code(),
        UpdateErrorCode::NoMatchingAsset
    );
}

#[test]
fn invalid_empty_and_oversized_responses_do_not_report_up_to_date() {
    assert_eq!(
        read_page(b"not json".as_slice()).err().unwrap().code(),
        UpdateErrorCode::ReleaseManifestInvalid
    );
    assert_eq!(
        select(vec![], "0.1.0").unwrap_err().code(),
        UpdateErrorCode::ReleaseFetchFailed
    );
    assert_eq!(
        select(vec![release("0.2.0")], "not a version")
            .unwrap_err()
            .code(),
        UpdateErrorCode::NotConfigured
    );
    let oversized = vec![b' '; MAX_BODY_BYTES as usize + 1];
    assert_eq!(
        read_page(oversized.as_slice()).err().unwrap().code(),
        UpdateErrorCode::ReleaseManifestInvalid
    );
    let many = serde_json::to_vec(&vec![release("0.1.0"); PAGE_SIZE + 1]).unwrap();
    assert_eq!(
        read_page(many.as_slice()).err().unwrap().code(),
        UpdateErrorCode::ReleaseManifestInvalid
    );
}

#[test]
fn broken_transport_and_unsupported_target_are_failures() {
    struct Broken;
    impl Read for Broken {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("transport failed"))
        }
    }
    assert_eq!(
        read_page(Broken).err().unwrap().code(),
        UpdateErrorCode::ReleaseFetchFailed
    );
    let values = read_page(
        serde_json::to_vec(&vec![release("0.2.0")])
            .unwrap()
            .as_slice(),
    )
    .unwrap();
    assert_eq!(
        select_release(&values, "0.1.0", UpdateTargetTriple::Aarch64AppleDarwin)
            .unwrap_err()
            .code(),
        UpdateErrorCode::NoMatchingAsset
    );
}

#[test]
#[ignore = "anonymous live GitHub smoke; requires network and the Together Windows release"]
fn anonymous_live_together_release_check() {
    let diagnostics = UpdateDiagnosticsTracker::default();
    let checker = TogetherReleaseChecker::new("0.0.0", diagnostics.clone());
    let result = checker.check().expect("anonymous Together release fetch");
    assert!(
        matches!(result, UpdateOutcome::Available { release } if Version::parse(&release.version).is_ok())
    );
    let current = TogetherReleaseChecker::new(env!("CARGO_PKG_VERSION"), diagnostics.clone());
    assert!(matches!(current.check().unwrap(), UpdateOutcome::UpToDate));
    assert_eq!(diagnostics.snapshot().checks_succeeded, 2);
    assert_eq!(diagnostics.snapshot().downloads_started, 0);
    assert_eq!(diagnostics.snapshot().installs_started, 0);
}
