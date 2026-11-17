use super::Store;

#[test]
fn closing_store_releases_ownership_even_with_a_duplicated_os_handle() {
    let directory =
        std::env::temp_dir().join(format!("cueglint-store-lock-{}", uuid::Uuid::new_v4()));
    let path = directory.join("session.sqlite3");
    let store = Store::open(&path).unwrap();
    let inherited = store._lock.as_ref().unwrap().try_clone().unwrap();
    assert!(
        Store::open(&path).is_err(),
        "live ownership stays exclusive"
    );
    drop(store);
    let reopened = Store::open(&path).expect("closed Store releases its own lock");
    assert!(inherited.metadata().is_ok(), "the duplicate stays open");
    assert!(Store::open(&path).is_err(), "new ownership stays exclusive");
    drop(reopened);
    drop(inherited);
    std::fs::remove_dir_all(directory).unwrap();
}
