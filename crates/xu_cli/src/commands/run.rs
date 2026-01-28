use std::io::Write;

use xu_driver::Driver;
use xu_runtime::Runtime;
use xu_syntax::RenderOptions;

use crate::args::CliArgs;
use crate::commands::emit_diagnostics;

pub(crate) fn run(args: &CliArgs, driver: &Driver, render_opts: RenderOptions) {
    if args.positional.is_empty() {
        eprintln!("Missing <file>");
        std::process::exit(2);
    }
    let path = args.positional[0].as_str();
    let compiled = match driver.compile_file_bytecode(path, args.strict) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    };
    if !args.no_diags {
        emit_diagnostics(
            &compiled.source,
            &compiled.diagnostics,
            render_opts,
            args.json_out,
        );
    }

    let mut rt = Runtime::new();
    rt.set_strict_vars(args.strict);
    rt.set_frontend(Box::new(Driver::new()));
    rt.set_entry_path(path).expect("set entry path");
    set_stdlib_path(&mut rt);
    rt.set_args(args.positional.clone());

    let result = rt.exec_executable(&compiled.executable);
    let output = match &result {
        Ok(res) => res.output.clone(),
        Err(_) => rt.take_output(),
    };

    let mut stdout = std::io::stdout().lock();
    let _ = write!(stdout, "{}", output);

    if let Err(e) = result {
        eprintln!("RuntimeError: {e}");
        std::process::exit(1);
    }
}

fn set_stdlib_path(rt: &mut Runtime) {
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(project_root) = exe_path
            .parent()
            .and_then(|p| p.parent())
            .and_then(|p| p.parent())
        {
            let stdlib = project_root.join("stdlib");
            if stdlib.exists() {
                rt.set_stdlib_path(stdlib.to_string_lossy().to_string());
            }
        }
    }
    if std::path::Path::new("stdlib").exists() {
        rt.set_stdlib_path(
            std::env::current_dir()
                .unwrap()
                .join("stdlib")
                .to_string_lossy()
                .to_string(),
        );
    }
}
