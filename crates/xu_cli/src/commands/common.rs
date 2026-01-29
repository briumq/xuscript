use xu_syntax::SourceFile;

pub fn parse_input_path(path: &str) -> Result<SourceFile, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("Read failed: {e}"))?;
    Ok(SourceFile::new(path.to_string(), text))
}

pub fn print_json(obj: serde_json::Value) {
    println!("{}", obj);
}
