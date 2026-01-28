use std::path::PathBuf;

use super::Runtime;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImportStamp {
    pub len: u64,
    pub modified_nanos: Option<u128>,
}

pub trait ModuleLoader {
    fn resolve_key(&self, rt: &Runtime, raw: &str) -> Result<String, String>;
    fn load_text_and_stamp(&self, rt: &Runtime, key: &str)
    -> Result<(String, ImportStamp), String>;
}
pub struct StdModuleLoader;

impl ModuleLoader for StdModuleLoader {
    fn resolve_key(&self, rt: &Runtime, raw: &str) -> Result<String, String> {
        if raw.starts_with("std/") {
            if let Some(stdlib_path) = rt.stdlib_path() {
                let p = PathBuf::from(stdlib_path).join(raw);
                let p_with_ext = if p.extension().is_none() {
                    p.with_extension("xu")
                } else {
                    p
                };
                if let Ok(path) = rt.canonicalize_import_checked(&p_with_ext.to_string_lossy()) {
                    return Ok(path);
                }
            }
        }

        let raw_path = PathBuf::from(raw);
        if raw_path.is_absolute() {
            return rt.canonicalize_import_checked(raw);
        }

        let mut candidates: Vec<PathBuf> = Vec::new();
        if let Some(base) = current_import_base_dir(rt) {
            candidates.push(base.join(&raw_path));
        }
        candidates.push(raw_path.clone());

        let mut last_err: Option<String> = None;
        for c in &candidates {
            match rt.canonicalize_import_checked(&c.to_string_lossy()) {
                Ok(p) => return Ok(p),
                Err(e) => last_err = Some(e),
            }
        }
        let tried = candidates
            .iter()
            .map(|p| p.to_string_lossy())
            .collect::<Vec<_>>()
            .join(", ");
        let msg = last_err.unwrap_or_else(|| "File not found".into());
        Err(format!("Import failed: {msg} (tried: {tried})"))
    }

    fn load_text_and_stamp(
        &self,
        rt: &Runtime,
        key: &str,
    ) -> Result<(String, ImportStamp), String> {
        let stat = rt.fs_stat(key)?;
        let text = rt.fs_read_to_string_import(key)?;
        Ok((
            text,
            ImportStamp {
                len: stat.len,
                modified_nanos: stat.modified_nanos,
            },
        ))
    }
}

fn current_import_base_dir(rt: &Runtime) -> Option<std::path::PathBuf> {
    let base = rt
        .import_stack
        .last()
        .cloned()
        .or_else(|| rt.entry_path.clone())?;
    std::path::PathBuf::from(base)
        .parent()
        .map(|p| p.to_path_buf())
}
