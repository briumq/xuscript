use std::fs;
use std::path::PathBuf;

pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .unwrap()
        .to_path_buf()
}

pub fn golden_path_for(subdir: &str, name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
        .join(subdir)
        .join(format!("{name}.txt"))
}

pub fn golden_update_enabled() -> bool {
    let v = std::env::var("XU_UPDATE_GOLDEN").ok();
    if v.as_deref().is_some_and(|v| v == "1" || v == "true") {
        return true;
    }
    std::env::var("HAOSCRIPT_UPDATE_GOLDEN").is_ok_and(|v| v == "1" || v == "true")
}

pub fn assert_or_update(path: PathBuf, actual: &str) {
    let update = golden_update_enabled();
    if update {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&path, actual).unwrap();
        return;
    }
    let expected = fs::read_to_string(&path).unwrap_or_default();
    assert_eq!(
        actual.trim_end(),
        expected.trim_end(),
        "Golden mismatch for {:?}",
        path
    );
}

pub fn find_files(dir: &PathBuf, ext: &str) -> Vec<PathBuf> {
    let mut files = Vec::new();
    if dir.is_dir() {
        for entry in fs::read_dir(dir).expect("read_dir") {
            let entry = entry.expect("entry");
            let path = entry.path();
            if path.is_dir() {
                files.extend(find_files(&path, ext));
            } else if path.extension().and_then(|s| s.to_str()) == Some(ext) {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}
