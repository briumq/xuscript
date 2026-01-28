pub(crate) struct CliArgs {
    pub cmd: String,
    pub strict: bool,
    pub timing: bool,
    pub verbose: bool,
    pub no_diags: bool,
    pub json_out: bool,
    pub color: bool,
    pub positional: Vec<String>,
}

pub(crate) fn usage() -> &'static str {
    "Usage: xu <tokens|check|ast|run> [strict] [timing] [verbose] [no-diags] [json] [color] <args>"
}

pub(crate) fn parse_args() -> Result<CliArgs, String> {
    let mut argv: Vec<String> = std::env::args().skip(1).collect();
    let cmd = argv.first().cloned().ok_or_else(|| usage().to_string())?;
    argv.remove(0);

    let mut strict = false;
    let mut timing = false;
    let mut verbose = false;
    let mut no_diags = false;
    let mut json_out = false;
    let mut color = false;
    let mut positional: Vec<String> = Vec::new();

    let mut i = 0;
    while i < argv.len() {
        let a = &argv[i];
        if a.starts_with("--") {
            return Err(format!("Unknown option: {a}"));
        }
        if a == "strict" {
            strict = true;
        } else if a == "timing" {
            timing = true;
        } else if a == "verbose" {
            verbose = true;
        } else if a == "no-diags" {
            no_diags = true;
        } else if a == "json" {
            json_out = true;
        } else if a == "color" {
            color = true;
        } else {
            positional.push(a.clone());
        }
        i += 1;
    }

    Ok(CliArgs {
        cmd,
        strict,
        timing,
        verbose,
        no_diags,
        json_out,
        color,
        positional,
    })
}
