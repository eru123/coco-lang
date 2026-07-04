//! Coco compiler CLI — lex, parse, fmt, check commands.

use clap::{Parser as ClapParser, Subcommand};
use coco_diagnostics::Diagnostic;
use coco_formatter::Formatter;
use coco_lexer::{Lexer, TokenKind};
use coco_parser::Parser;
use coco_safety as safety;
use coco_span::{FileId, SourceMap};
use coco_syntax::Item;
use num_traits::ToPrimitive;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(ClapParser)]
#[command(
    name = "coco",
    version = "0.1.0",
    about = "Coco language toolchain — lexer, parser, formatter, interpreter",
    long_about = None
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Tokenize a file and print the token stream
    Lex {
        /// Path to the .co file
        file: PathBuf,
    },
    /// Parse a file and print the AST
    Parse {
        /// Path to the .co file
        file: PathBuf,
    },
    /// Format a file and print to stdout (or write in-place with -w)
    Fmt {
        /// Path to the .co file
        file: PathBuf,
        /// Write result to the file in-place
        #[arg(short = 'w', long = "write")]
        write: bool,
        /// Check mode: exit 1 if file would change (dry-run)
        #[arg(long = "check")]
        check: bool,
    },
    /// Parse a file and report diagnostics
    Check {
        /// Path to the .co file
        file: PathBuf,
    },
    /// Type-check a .co file
    Typecheck {
        /// Path to the .co file
        file: PathBuf,
    },
    /// Run safety analysis on a .co file
    Safety {
        /// Path to the .co file
        file: PathBuf,
    },
    /// Run a .co file (or project if no file given)
    Run {
        /// Path to the .co file (defaults to src/main.co)
        file: Option<PathBuf>,
        /// Skip type checking before execution
        #[arg(long = "no-check")]
        no_check: bool,
        /// Enable debug mode (GC stats)
        #[arg(long = "debug")]
        debug: bool,
        /// Use the bytecode VM path for execution
        #[arg(long = "vm")]
        use_vm: bool,
    },
    /// Compile a .co file to a .cb bytecode artifact
    Build {
        /// Path to the .co file (defaults to src/main.co)
        file: Option<PathBuf>,
        /// Build a standalone executable with an embedded VM instead of a .cb file
        #[arg(long = "binary")]
        binary: bool,
        /// Print bytecode disassembly to stdout instead of writing a .cb file
        #[arg(long = "disasm")]
        disasm: bool,
        /// Enable optimizations (constant folding, dead code elimination; -O3
        /// for --binary via cargo --release)
        #[arg(long = "release")]
        release: bool,
    },
    /// Run test files in the tests/ directory
    Test {
        /// Filter tests by name pattern
        #[arg(long = "filter")]
        filter: Option<String>,
    },
    /// Initialize a new Coco project
    Init {
        /// Project name
        name: String,
    },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Lex { file } => cmd_lex(&file),
        Commands::Parse { file } => cmd_parse(&file),
        Commands::Fmt { file, write, check } => cmd_fmt(&file, write, check),
        Commands::Check { file } => cmd_check(&file),
        Commands::Typecheck { file } => cmd_typecheck(&file),
        Commands::Safety { file } => cmd_safety(&file),
        Commands::Run {
            file,
            no_check,
            debug,
            use_vm,
        } => {
            let f = resolve_entry(file.as_deref());
            cmd_run(&f, no_check, debug, use_vm)
        }
        Commands::Build {
            file,
            binary,
            disasm,
            release,
        } => {
            let f = resolve_entry(file.as_deref());
            cmd_build(&f, binary, disasm, release)
        }
        Commands::Test { filter } => cmd_test(filter.as_deref()),
        Commands::Init { name } => cmd_init(&name),
    }
}

fn resolve_file(file: &Path) -> Option<PathBuf> {
    let base = std::env::current_dir().ok()?;
    resolve_file_in(&base, file)
}

fn resolve_file_in(base: &Path, file: &Path) -> Option<PathBuf> {
    let exact = base.join(file);
    if exact.exists() {
        return Some(exact);
    }

    let with_extension = base.join(file.with_extension("co"));
    if with_extension.exists() {
        return Some(with_extension);
    }

    // Bytecode artifacts (.cb) — resolved after .co so source wins on ties.
    let with_cb = base.join(file.with_extension("cb"));
    if with_cb.exists() {
        return Some(with_cb);
    }

    let src_exact = base.join("src").join(file);
    if src_exact.exists() {
        return Some(src_exact);
    }

    let src_with_extension = base.join("src").join(file.with_extension("co"));
    if src_with_extension.exists() {
        return Some(src_with_extension);
    }

    let src_with_cb = base.join("src").join(file.with_extension("cb"));
    if src_with_cb.exists() {
        return Some(src_with_cb);
    }

    None
}

/// Resolve the entry point file. If none given, look for src/main.co, main.co.
fn resolve_entry(file: Option<&Path>) -> PathBuf {
    if let Some(f) = file {
        return f.to_path_buf();
    }
    for candidate in &["src/main.co", "main.co", "src/index.co", "index.co"] {
        let p = Path::new(candidate);
        if p.exists() {
            return p.to_path_buf();
        }
    }
    PathBuf::from("main.co")
}

/// Initialize a new Coco project.
fn cmd_init(name: &str) {
    let dir = Path::new(name);
    if dir.exists() {
        eprintln!("error: directory '{}' already exists", name);
        std::process::exit(1);
    }
    fs::create_dir_all(dir.join("src")).unwrap_or_else(|e| {
        eprintln!("error creating project: {}", e);
        std::process::exit(1);
    });

    // Write coco.toml
    let toml = format!(
        r#"[package]
name = "{}"
version = "0.1.0"
description = "A Coco project"
authors = []
license = "MIT"
edition = "1.0"

[dependencies]

[dev-dependencies]

[build]
# Build target: "bytecode" (.cb artifact) or "binary" (standalone executable
# with an embedded VM). Bytecode is the default; it ships small and runs via
# `coco run prog.cb`.
target = "bytecode"
optimize = false

[safety]
mode = "application"
"#,
        name
    );
    fs::write(dir.join("coco.toml"), &toml).unwrap();

    // Write src/main.co
    let main_co = r#"import { print } from "std/io";

fn main(): int {
    print("Hello from Coco!");
    return 0;
}

main();
"#;
    fs::write(dir.join("src").join("main.co"), main_co).unwrap();

    println!("Created Coco project '{}'", name);
    println!("  cd {}", name);
    println!("  coco run");
}

fn read_source(file: &Path) -> Result<(String, PathBuf), String> {
    let resolved = resolve_file(file).ok_or_else(|| {
        format!(
            "error: cannot find '{}' (tried {}, {}.co, src/{}, src/{}.co)",
            file.display(),
            file.display(),
            file.display(),
            file.display(),
            file.display()
        )
    })?;
    let source = fs::read_to_string(&resolved)
        .map_err(|e| format!("error reading file '{}': {}", resolved.display(), e))?;
    Ok((source, resolved))
}

fn cmd_lex(file: &Path) {
    let (source, _) = match read_source(file) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{}", e);
            return;
        }
    };

    let mut lexer = Lexer::new(&source);
    loop {
        let token = lexer.next_token();
        if token.kind == TokenKind::Eof {
            break;
        }
        println!("{:?} '{}' {:?}", token.kind, token.text, token.span);
    }
}

fn cmd_parse(file: &Path) {
    let (source, resolved) = match read_source(file) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{}", e);
            return;
        }
    };

    let mut parser = Parser::new(&source);
    let program = parser.parse_program();

    // Print diagnostics with ariadne
    let diagnostics = parser.diagnostics();
    if !diagnostics.is_empty() {
        report_parser_diagnostics(&source, &resolved, &diagnostics);
    }

    // Print AST
    println!("Parsed {} items:", program.items.len());
    for (i, item) in program.items.iter().enumerate() {
        println!("  {}: {:?}", i, item_name(item));
    }
    println!("Program span: {:?}", program.span);
}

fn item_name(item: &Item) -> &'static str {
    match item {
        Item::FnDecl(_) => "FnDecl",
        Item::ClassDecl(_) => "ClassDecl",
        Item::InterfaceDecl(_) => "InterfaceDecl",
        Item::TraitDecl(_) => "TraitDecl",
        Item::EnumDecl(_) => "EnumDecl",
        Item::ConstDecl(_) => "ConstDecl",
        Item::LetDecl(_) => "LetDecl",
        Item::TypeAlias(_) => "TypeAlias",
        Item::Import(_) => "Import",
        Item::Export(_) => "Export",
        Item::ExprStmt(_) => "ExprStmt",
        Item::Stmt(_) => "Stmt",
    }
}

fn cmd_fmt(file: &Path, write: bool, check: bool) {
    let (source, resolved) = match read_source(file) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{}", e);
            return;
        }
    };

    let mut parser = Parser::new(&source);
    let program = parser.parse_program();

    let diagnostics = parser.diagnostics();
    if !diagnostics.is_empty() {
        report_parser_diagnostics(&source, &resolved, &diagnostics);
    }

    let mut formatter = Formatter::new();
    let formatted = formatter.format(&program);

    if check {
        if source != formatted {
            eprintln!("{}: would be reformatted", resolved.display());
            std::process::exit(1);
        }
    } else if write {
        match fs::write(&resolved, &formatted) {
            Ok(_) => println!("Formatted {}", resolved.display()),
            Err(e) => eprintln!("error writing to '{}': {}", resolved.display(), e),
        }
    } else {
        print!("{}", formatted);
    }
}

fn cmd_run(file: &Path, no_check: bool, debug: bool, _use_vm: bool) {
    // Bytecode artifact (.cb): deserialize and run directly, skipping
    // parse/typecheck/safety/compile. The artifact is already validated.
    let is_cb = file.extension().map(|e| e == "cb").unwrap_or(false);
    if is_cb {
        run_cb_artifact(file, debug);
        return;
    }

    let (source, resolved) = match read_source(file) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(1);
        }
    };

    if !no_check {
        let mut parser = Parser::new(&source);
        let program = parser.parse_program();
        if !parser.diagnostics().is_empty() {
            report_parser_diagnostics(&source, &resolved, &parser.diagnostics());
            eprintln!(
                "{}: {} parse error(s)",
                resolved.display(),
                parser.diagnostics().len()
            );
            std::process::exit(1);
        }

        let result = coco_typeck::check(&program);
        if result.has_errors() {
            report_type_errors(&source, &resolved, &result);
            eprintln!(
                "\n{} type error(s). Use --no-check to skip.",
                result.errors.len()
            );
            std::process::exit(1);
        }

        let safety_result = safety::analyze(&program);
        if safety_result.has_errors() {
            report_safety_errors(&source, &resolved, &safety_result);
            eprintln!(
                "\n{} safety error(s). Use --no-check to skip.",
                safety_result.errors.len()
            );
            std::process::exit(1);
        }
    }

    // VM is the sole dev runtime. The `--vm` flag is a no-op.
    run_with_vm(&source, debug);
}

fn run_with_vm(source: &str, debug: bool) {
    let mut parser = Parser::new(source);
    let program = parser.parse_program();

    // Run the type checker to obtain inferred types (keyed by span). The
    // compiler consults this map to emit type-specialized arithmetic opcodes
    // (the adaptive numeric tower's static tier: OP_ADD_I for int+int, OP_ADD_F
    // for float-involved). Type errors are NOT enforced here — the `run`
    // command's --check gate handles that; this is purely for specialization,
    // so gradual/untyped code still compiles via generic opcodes.
    let typeck_result = coco_typeck::check(&program);
    let mut compiler = coco_interpreter::compiler::Compiler::new().with_types(typeck_result.types);
    let chunk = match compiler.compile_script(&program) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Compile error: {}", e);
            std::process::exit(1);
        }
    };

    run_chunk(&chunk, debug);
}

/// Run a `.cb` bytecode artifact: load, deserialize, execute. Skips
/// parse/typecheck/safety/compile entirely — the artifact is pre-validated.
fn run_cb_artifact(path: &Path, debug: bool) {
    let bytes = match fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("error reading {}: {}", path.display(), e);
            std::process::exit(1);
        }
    };
    let chunk = match coco_interpreter::deserialize_chunk(&bytes) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error loading {}: {}", path.display(), e);
            std::process::exit(1);
        }
    };
    run_chunk(&chunk, debug);
}

/// Execute a compiled `Chunk` through the VM and exit with its return code.
/// Shared by the source-run path (parse + compile) and the artifact-run path
/// (deserialize), so both exit identically.
fn run_chunk(chunk: &coco_interpreter::ir::Chunk, debug: bool) {
    let mut vm = coco_interpreter::vm::Vm::new();
    vm.set_debug(debug);
    match vm.run(chunk) {
        Ok(val) => {
            if let Some(code) = val.as_i64() {
                std::process::exit(code as i32);
            }
        }
        Err(e) => {
            eprintln!("VM error: {}", e);
            std::process::exit(1);
        }
    }
}

fn cmd_build(file: &Path, binary: bool, disasm: bool, release: bool) {
    let (source, resolved) = match read_source(file) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(1);
        }
    };

    let mut parser = Parser::new(&source);
    let program = parser.parse_program();
    if !parser.diagnostics().is_empty() {
        report_parser_diagnostics(&source, &resolved, &parser.diagnostics());
        std::process::exit(1);
    }

    // Type/safety analysis runs as warnings only: artifacts are still emitted
    // for gradual-typed code. This matches the build = validated-but-not-gated
    // contract; `coco run foo.co` keeps the hard-check behavior.
    let typeck_result = coco_typeck::check(&program);
    for err in &typeck_result.errors {
        eprintln!("[type warning] {} {}", err.code, err.message);
    }
    let safety_result = safety::analyze(&program);
    for err in &safety_result.errors {
        eprintln!("[safety warning] {} {}", err.code, err.message);
    }

    let mut compiler = coco_interpreter::compiler::Compiler::new().with_types(typeck_result.types);
    if release {
        compiler.enable_tree_shake = true;
    }
    let chunk = match compiler.compile_script(&program) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Compile error: {}", e);
            std::process::exit(1);
        }
    };

    // --disasm: print the disassembly and stop (debugging aid for the compiler).
    if disasm {
        println!("== {} (script) ==", resolved.display());
        println!("{}", coco_interpreter::ir::disassemble(&chunk, "script"));
        for val in &chunk.constants {
            if let coco_interpreter::Value::FnObj(fo) = val {
                println!("{}", coco_interpreter::ir::disassemble(&fo.chunk, &fo.name));
            }
        }
        return;
    }

    let bytecode = match coco_interpreter::serialize_chunk(&chunk) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("error serializing bytecode: {}", e);
            std::process::exit(1);
        }
    };

    // Default: write a .cb artifact (foo.co -> foo.cb).
    let cb_path = resolved.with_extension("cb");
    if let Err(e) = fs::write(&cb_path, &bytecode) {
        eprintln!("error writing {}: {}", cb_path.display(), e);
        std::process::exit(1);
    }
    println!("Compiled {} -> {}", resolved.display(), cb_path.display());

    // --binary: also produce a standalone executable embedding the VM + bytecode.
    if binary {
        let bin_path = resolved.with_extension("");
        match build_embedded_binary(&bytecode, &bin_path, release) {
            Ok(()) => println!("Linked {} -> {}", resolved.display(), bin_path.display()),
            Err(e) => {
                eprintln!("error building binary: {}", e);
                std::process::exit(1);
            }
        }
    }
}

/// Produce a standalone executable that embeds the VM and the given bytecode.
///
/// Generates a throwaway Rust crate whose `main.rs` `include_bytes!`s the
/// `.cb` payload and calls `coco_interpreter` as a library, then shells out to
/// `cargo build` to compile it. The resulting binary is copied to `out_path`.
fn build_embedded_binary(bytecode: &[u8], out_path: &Path, release: bool) -> Result<(), String> {
    use std::io::Write;

    // Locate the in-tree coco_interpreter the generated crate will depend on.
    let workspace_root = env!("COCO_WORKSPACE_ROOT");
    let interpreter_path = Path::new(workspace_root).join("crates/coco_interpreter");
    if !interpreter_path.exists() {
        return Err(format!(
            "cannot locate coco_interpreter at {} — the `coco` binary was built \
             from a relocated workspace; rebuilding coco from its source tree fixes this",
            interpreter_path.display()
        ));
    }

    let tmp = std::env::temp_dir().join(format!(
        "coco-build-{}-{}",
        std::process::id(),
        out_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("out")
    ));
    std::fs::create_dir_all(tmp.join("src"))
        .map_err(|e| format!("failed to create temp crate dir: {}", e))?;

    // Cargo.toml: depend on the in-tree coco_interpreter by path.
    let cargo_toml = format!(
        r#"[package]
name = "coco_embedded"
version = "0.0.0"
edition = "2021"

[[bin]]
name = "coco_embedded"
path = "src/main.rs"

[dependencies]
# default-features = false keeps the embedded binary slim (no SQLite/mio
# unless the program uses db/io_wait builtins). Re-enable with features =
# ["db", "async-io"] if needed.
coco_interpreter = {{ path = {:?}, default-features = false }}
num-traits = "0.2"
"#,
        interpreter_path.display().to_string()
    );
    fs::write(tmp.join("Cargo.toml"), cargo_toml)
        .map_err(|e| format!("failed to write Cargo.toml: {}", e))?;

    // payload.cb: the embedded bytecode.
    let mut payload = fs::File::create(tmp.join("src/payload.cb"))
        .map_err(|e| format!("failed to write payload: {}", e))?;
    payload
        .write_all(bytecode)
        .map_err(|e| format!("failed to write payload: {}", e))?;

    // main.rs: load the embedded bytecode and run it through the VM.
    let main_rs = r#"fn main() {
    let bytecode: &[u8] = include_bytes!("payload.cb");
    let chunk = match coco_interpreter::deserialize_chunk(bytecode) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("corrupt embedded bytecode: {e}");
            std::process::exit(1);
        }
    };
    let mut vm = coco_interpreter::vm::Vm::new();
    match vm.run(&chunk) {
        Ok(val) => {
            if let Some(code) = val.as_i64() {
                std::process::exit(code as i32);
            }
        }
        Err(e) => {
            eprintln!("VM error: {e}");
            std::process::exit(1);
        }
    }
}
"#;
    fs::write(tmp.join("src/main.rs"), main_rs)
        .map_err(|e| format!("failed to write main.rs: {}", e))?;

    // Build the generated crate.
    let target_dir = tmp.join("target");
    let mut cmd = std::process::Command::new("cargo");
    cmd.arg("build")
        .arg("--bin")
        .arg("coco_embedded")
        .arg("--manifest-path")
        .arg(tmp.join("Cargo.toml"))
        .arg("--target-dir")
        .arg(&target_dir);
    if release {
        cmd.arg("--release");
    }
    let status = cmd
        .status()
        .map_err(|e| format!("failed to run cargo: {}", e))?;
    if !status.success() {
        return Err("cargo build for embedded crate failed".to_string());
    }

    // Locate the built binary and copy it to the requested output path.
    let profile = if release { "release" } else { "debug" };
    let built = target_dir.join(profile).join("coco_embedded");
    if !built.exists() {
        return Err(format!(
            "built binary not found at {} (expected after successful cargo build)",
            built.display()
        ));
    }
    fs::copy(&built, out_path)
        .map_err(|e| format!("failed to copy binary to {}: {}", out_path.display(), e))?;

    // Best-effort cleanup of the temp crate (keep target/ out of the way).
    let _ = std::fs::remove_dir_all(&tmp);
    Ok(())
}

/// Discover and run test files in tests/*.co.
fn cmd_test(filter: Option<&str>) {
    let tests_dir = Path::new("tests");
    if !tests_dir.exists() || !tests_dir.is_dir() {
        eprintln!("error: no tests/ directory found");
        std::process::exit(1);
    }

    let mut entries: Vec<PathBuf> = match fs::read_dir(tests_dir) {
        Ok(read) => read
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().map(|e| e == "co").unwrap_or(false))
            .collect(),
        Err(e) => {
            eprintln!("error reading tests/: {}", e);
            std::process::exit(1);
        }
    };

    entries.sort();

    if let Some(pattern) = filter {
        let pattern_lower = pattern.to_lowercase();
        entries.retain(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.to_lowercase().contains(&pattern_lower))
                .unwrap_or(false)
        });
    }

    if entries.is_empty() {
        println!("No test files found.");
        return;
    }

    let mut passed = 0;
    let mut failed = 0;

    for entry in &entries {
        let source = match fs::read_to_string(entry) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("{}: read error: {}", entry.display(), e);
                failed += 1;
                continue;
            }
        };

        // Parse
        let mut parser = Parser::new(&source);
        let program = parser.parse_program();
        if !parser.diagnostics().is_empty() {
            eprintln!("{}: parse error", entry.display());
            failed += 1;
            continue;
        }

        // Compile + run via VM (with type-driven opcode specialization).
        let typeck_result = coco_typeck::check(&program);
        let mut compiler =
            coco_interpreter::compiler::Compiler::new().with_types(typeck_result.types);
        let chunk = match compiler.compile_script(&program) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("{}: compile error: {}", entry.display(), e);
                failed += 1;
                continue;
            }
        };

        let mut vm = coco_interpreter::vm::Vm::new();
        match vm.run(&chunk) {
            Ok(_) => {
                println!(
                    "  PASS  {}",
                    entry.file_name().unwrap_or_default().to_string_lossy()
                );
                passed += 1;
            }
            Err(e) => {
                println!(
                    "  FAIL  {}: {}",
                    entry.file_name().unwrap_or_default().to_string_lossy(),
                    e
                );
                failed += 1;
            }
        }
    }

    println!(
        "\n{} passed, {} failed, {} total",
        passed,
        failed,
        passed + failed
    );
    if failed > 0 {
        std::process::exit(1);
    }
}

fn cmd_check(file: &Path) {
    let (source, resolved) = match read_source(file) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{}", e);
            return;
        }
    };

    let mut parser = Parser::new(&source);
    let program = parser.parse_program();

    let diagnostics = parser.diagnostics();
    if !diagnostics.is_empty() {
        report_parser_diagnostics(&source, &resolved, &diagnostics);
    }

    let safety_result = safety::analyze(&program);
    let typeck_result = coco_typeck::check(&program);

    let parse_ok = diagnostics.is_empty();
    let type_ok = !typeck_result.has_errors();
    let safety_ok = !safety_result.has_errors();

    if !type_ok {
        report_type_errors(&source, &resolved, &typeck_result);
    }
    if !safety_ok {
        report_safety_errors(&source, &resolved, &safety_result);
    }

    if parse_ok && type_ok && safety_ok {
        println!(
            "{}: OK ({} items parsed, types OK, safety OK)",
            resolved.display(),
            program.items.len()
        );
    } else {
        eprintln!(
            "{}: {} parse error(s), {} type error(s), {} safety error(s)",
            resolved.display(),
            diagnostics.len(),
            typeck_result.error_count(),
            safety_result.error_count()
        );
    }
}

fn cmd_typecheck(file: &Path) {
    let (source, resolved) = match read_source(file) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(1);
        }
    };

    let mut parser = Parser::new(&source);
    let program = parser.parse_program();
    if !parser.diagnostics().is_empty() {
        report_parser_diagnostics(&source, &resolved, &parser.diagnostics());
        std::process::exit(1);
    }

    let result = coco_typeck::check(&program);
    if result.has_errors() {
        report_type_errors(&source, &resolved, &result);
        eprintln!(
            "\n{} type error(s) found in {}",
            result.errors.len(),
            resolved.display()
        );
        std::process::exit(1);
    }

    println!("{}: types OK", resolved.display());
}

fn cmd_safety(file: &Path) {
    let (source, resolved) = match read_source(file) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(1);
        }
    };

    let mut parser = Parser::new(&source);
    let program = parser.parse_program();
    if !parser.diagnostics().is_empty() {
        report_parser_diagnostics(&source, &resolved, &parser.diagnostics());
        std::process::exit(1);
    }

    let result = safety::analyze(&program);
    if result.has_errors() {
        report_safety_errors(&source, &resolved, &result);
        eprintln!(
            "\n{} safety error(s) found in {}",
            result.errors.len(),
            resolved.display()
        );
        std::process::exit(1);
    }

    println!(
        "{}: safety OK ({} warnings)",
        resolved.display(),
        result.warning_count()
    );
}

/// Build a SourceMap, register the main source file, and return it along with its FileId.
fn build_source_map(source: &str, file: &Path) -> (SourceMap, FileId) {
    let mut map = SourceMap::new();
    let id = map.add_file(file.to_path_buf(), source.to_string());
    (map, id)
}

/// Emit safety errors with ariadne-rendered diagnostics.
fn report_safety_errors(source: &str, file: &Path, result: &safety::SafetyResult) {
    let (source_map, file_id) = build_source_map(source, file);
    for error in &result.errors {
        let mut diag = Diagnostic::error(file_id, format!("[{}] {}", error.code, error.message));
        if !error.span.is_empty() {
            diag = diag.with_label(error.span, "here", true);
        }
        diag.emit(&source_map);
    }
    for warning in &result.warnings {
        let mut diag =
            Diagnostic::warning(file_id, format!("[{}] {}", warning.code, warning.message));
        if !warning.span.is_empty() {
            diag = diag.with_label(warning.span, "here", true);
        }
        diag.emit(&source_map);
    }
}

/// Emit type errors with ariadne-rendered diagnostics.
fn report_type_errors(source: &str, file: &Path, result: &coco_typeck::TypeckResult) {
    let (source_map, file_id) = build_source_map(source, file);
    for error in &result.errors {
        let mut diag = Diagnostic::error(file_id, format!("[{}] {}", error.code, error.message));
        if !error.span.is_empty() {
            diag = diag.with_label(error.span, "here", true);
        }
        diag.emit(&source_map);
    }
}

/// Emit parser diagnostics with ariadne-rendered labels.
/// Parser diagnostics now carry their own span labels.
fn report_parser_diagnostics(
    source: &str,
    file: &Path,
    diagnostics: &[coco_diagnostics::Diagnostic],
) {
    if diagnostics.is_empty() {
        return;
    }
    let (source_map, file_id) = build_source_map(source, file);
    for diag in diagnostics {
        // Clone the diagnostic and reassign file_id to the one from our SourceMap
        let mut d = diag.clone();
        d.file = file_id;
        d.emit(&source_map);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir() -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("coco-cli-test-{}-{}", std::process::id(), suffix));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn resolve_file_finds_exact_path_first() {
        let dir = temp_dir();
        let file = dir.join("main.co");
        fs::write(&file, "fn main() {}").unwrap();

        assert_eq!(resolve_file_in(&dir, &PathBuf::from("main.co")), Some(file));

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn resolve_file_adds_co_extension() {
        let dir = temp_dir();
        let file = dir.join("main.co");
        fs::write(&file, "fn main() {}").unwrap();

        assert_eq!(resolve_file_in(&dir, &PathBuf::from("main")), Some(file));

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn resolve_file_checks_src_directory() {
        let dir = temp_dir();
        let src = dir.join("src");
        fs::create_dir_all(&src).unwrap();
        let file = src.join("main.co");
        fs::write(&file, "fn main() {}").unwrap();

        assert_eq!(resolve_file_in(&dir, &PathBuf::from("main")), Some(file));

        fs::remove_dir_all(dir).unwrap();
    }
}
