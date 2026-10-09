mod auth;
mod closure;
mod files;
mod machines;
mod plans;
mod support;

/// Fails if a file or directory beside this one is not reached by a `mod`, or if a stray
/// `tests/*.rs` would build as a second test binary: either would silently never run (or run alone).
#[test]
fn every_test_file_is_declared() {
    let here = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let main = std::fs::read_to_string(here.join("it/main.rs")).expect("main.rs");
    let names = |dir: &std::path::Path| -> Vec<String> {
        let mut found: Vec<String> = std::fs::read_dir(dir)
            .expect("test dir")
            .map(|e| e.expect("entry").path())
            .filter_map(|p| match p.extension().and_then(|x| x.to_str()) {
                Some("rs") | None => p.file_stem()?.to_str().map(str::to_owned),
                Some(_) => None,
            })
            .collect();
        found.sort();
        found
    };
    for name in names(&here.join("it")).into_iter().filter(|n| n != "main") {
        assert!(
            main.contains(&format!("mod {name};")),
            "tests/it/{name} is not declared in tests/it/main.rs with `mod {name};`"
        );
    }
    for name in names(&here) {
        assert!(
            name == "it" || name == "golden",
            "tests/{name} would be its own test binary; put it under tests/it"
        );
    }
}
