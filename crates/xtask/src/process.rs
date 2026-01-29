use std::process::{Command, Output};

pub fn run_args(cmd: &str, args: &[&str]) -> Result<Output, String> {
    eprintln!(
        "$ {} {}",
        cmd,
        args.iter()
            .map(|s| shell_escape(s))
            .collect::<Vec<_>>()
            .join(" ")
    );
    Command::new(cmd)
        .args(args)
        .output()
        .map_err(|e| format!("Failed to run {cmd}: {e}"))
}

pub fn run_owned(cmd: &str, args: &[String]) -> Result<Output, String> {
    eprintln!(
        "$ {} {}",
        cmd,
        args.iter()
            .map(|s| shell_escape(s))
            .collect::<Vec<_>>()
            .join(" ")
    );
    Command::new(cmd)
        .args(args)
        .output()
        .map_err(|e| format!("Failed to run {cmd}: {e}"))
}

pub fn format_output(o: &Output) -> String {
    let mut s = String::new();
    if !o.stdout.is_empty() {
        s.push_str("stdout:\n");
        s.push_str(&String::from_utf8_lossy(&o.stdout));
        if !s.ends_with('\n') {
            s.push('\n');
        }
    }
    if !o.stderr.is_empty() {
        s.push_str("stderr:\n");
        s.push_str(&String::from_utf8_lossy(&o.stderr));
        if !s.ends_with('\n') {
            s.push('\n');
        }
    }
    if s.is_empty() {
        s.push_str("(no output)\n");
    }
    s
}

fn shell_escape(s: &str) -> String {
    if s.chars().all(|c| c.is_ascii_alphanumeric() || "-_./:".contains(c)) {
        return s.to_string();
    }
    format!("{:?}", s)
}
