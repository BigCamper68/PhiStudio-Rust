use super::*;
fn release() -> Release {
    Release {
        version: semver::Version::parse(service::installed_version()).unwrap(),
        url: format!(
            "{}/tag/v{}",
            service::RELEASES,
            service::installed_version()
        ),
        name: "Installed release".into(),
        body: "- Release notes from GitHub.".into(),
    }
}
#[test]
fn only_presented_ready_content_is_acknowledged() {
    let mut notes = Notes {
        content: Content::Ready(release()),
        open: true,
        ..Default::default()
    };
    assert_eq!(notes.close(), None);
    notes.open = true;
    notes.presented();
    assert_eq!(notes.close().as_deref(), Some(service::installed_build()));
    notes.content = Content::Failed(CheckError::Connection);
    notes.open = true;
    notes.presented();
    assert_eq!(notes.close(), None);
}
#[test]
fn cache_is_version_scoped_and_can_be_used_offline() {
    let root = tempfile::tempdir().unwrap();
    let current = release();
    write_cache(root.path(), &current).unwrap();
    assert_eq!(
        load(Some(root.path()), false, || panic!("cache should be used")),
        Ok(current.clone())
    );
    assert_eq!(
        load(Some(root.path()), true, || Err(CheckError::Connection)),
        Ok(current)
    );
    let mut other = release();
    other.version = semver::Version::new(0, 0, 1);
    write_cache(root.path(), &other).unwrap();
    assert_eq!(
        load(Some(root.path()), false, || Err(CheckError::Connection)),
        Err(CheckError::Connection)
    );
}
#[test]
fn corrupt_cache_is_replaced_by_a_valid_installed_release() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("release-notes.json"), b"broken").unwrap();
    let current = release();
    assert_eq!(
        load(Some(root.path()), false, || Ok(current.clone())),
        Ok(current.clone())
    );
    assert_eq!(read_cache(root.path()), Some(current));
}
#[test]
fn a_seen_version_and_unmanaged_chart_mode_do_not_start_automatic_fetches() {
    let mut seen = Notes::default();
    seen.tick(Some(service::installed_build()), true);
    assert!(!seen.requested && !seen.open && seen.pending.is_none());
    let mut direct = Notes::default();
    direct.tick(None, false);
    assert!(!direct.requested && direct.pending.is_none());
}

#[test]
fn first_launch_loads_a_cached_body_and_keeps_the_request_until_presented() {
    let root = tempfile::tempdir().unwrap();
    write_cache(root.path(), &release()).unwrap();
    let mut notes = Notes {
        root: Some(root.path().to_owned()),
        ..Default::default()
    };
    notes.tick(None, true);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    while matches!(notes.content, Content::Loading) {
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(5));
        notes.tick(None, true);
    }
    assert!(matches!(notes.content, Content::Ready(_)));
    assert!(notes.requested);
    notes.open = true;
    notes.requested = false;
    notes.presented();
    let seen = notes.close().unwrap();
    notes.tick(Some(&seen), true);
    assert!(!notes.requested && !notes.open);
}
