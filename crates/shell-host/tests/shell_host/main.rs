//! All integration tests, one binary. No test here sets process-global state, so none needs its
//! own process. `gpu/tests.rs` shares one device through a `OnceLock`, so it stays a unit test of
//! the lib.

#[cfg(feature = "headless")]
mod headless;

// The examples' unit tests. They live beside the code they test; the examples themselves do not
// need to link the crate a second time to run them.
#[cfg(feature = "debug-inject")]
#[path = "../../examples/shared/dock.rs"]
#[allow(dead_code)]
mod dock;
#[path = "../../examples/demo_lock/guard.rs"]
#[allow(dead_code)]
mod guard;

/// Fails when a test file is added but not included above, or left as its own binary beside
/// this one: every `tests/shell_host/*.rs` must be a `mod` here, and `tests/*.rs` must not exist.
#[test]
fn every_test_file_is_included() {
    use std::fs;
    use std::path::PathBuf;

    let tests = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests");
    let rs_files = |dir: PathBuf| -> Vec<String> {
        fs::read_dir(dir)
            .expect("tests dir is readable")
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "rs"))
            .filter_map(|p| p.file_stem().map(|s| s.to_string_lossy().into_owned()))
            .collect()
    };
    assert_eq!(
        rs_files(tests.clone()),
        Vec::<String>::new(),
        "a tests/*.rs file would be its own binary; move it into tests/shell_host"
    );
    let main = fs::read_to_string(tests.join("shell_host/main.rs")).expect("main.rs");
    for name in rs_files(tests.join("shell_host")) {
        assert!(
            name == "main" || main.contains(&format!("mod {name};")),
            "tests/shell_host/{name}.rs is not included in main.rs"
        );
    }
}
