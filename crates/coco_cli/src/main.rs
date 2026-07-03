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
        /// (no-op: VM is now the default runtime) Use the bytecode VM instead of the tree-walking interpreter
        #[arg(long = "vm")]
        use_vm: bool,
    },
    /// Compile a .co file to bytecode and print disassembly
    Build {
        /// Path to the .co file (defaults to src/main.co)
        file: Option<PathBuf>,
        /// Compile to native binary via LLVM instead of bytecode disassembly
        #[arg(long = "native")]
        native: bool,
        /// Enable optimizations (constant folding, dead code in bytecode; LLVM -O3 for native)
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
        Commands::Build { file, native, release } => {
            let f = resolve_entry(file.as_deref());
            cmd_build(&f, native, release)
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

    let src_exact = base.join("src").join(file);
    if src_exact.exists() {
        return Some(src_exact);
    }

    let src_with_extension = base.join("src").join(file.with_extension("co"));
    if src_with_extension.exists() {
        return Some(src_with_extension);
    }

    None
}

/// Locate the `libcoco_rt.a` static archive produced by the `coco_rt` crate.
///
/// Searched relative to the current executable (the `coco` binary lives in
/// `target/<profile>/coco`, and the archive in `target/<profile>/libcoco_rt.a`).
/// Returns None if not found, in which case native linking proceeds without
/// it (and will fail to resolve `coco_rt_alloc` if the codegen emits calls to
/// it — surfacing the missing-runtime dependency clearly).
fn locate_coco_rt() -> Option<std::path::PathBuf> {
    // The `coco` binary lives in `target/<profile>/coco`; the coco_rt
    // staticlib output is `target/<profile>/libcoco_rt.a`.
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    let archive = dir.join("libcoco_rt.a");
    if archive.exists() {
        Some(archive)
    } else {
        None
    }
}

/// Resolve the entry point file. If none given, look for src/main.co, main.co.
fn resolve_entry(file: Option<&Path>) -> PathBuf {
    if let Some(f) = file { return f.to_path_buf(); }
    for candidate in &["src/main.co", "main.co", "src/index.co", "index.co"] {
        let p = Path::new(candidate);
        if p.exists() { return p.to_path_buf(); }
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
target = "native"
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

    let mut compiler = coco_interpreter::compiler::Compiler::new();
    let chunk = match compiler.compile_script(&program) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Compile error: {}", e);
            std::process::exit(1);
        }
    };

    let mut vm = coco_interpreter::vm::Vm::new();
    vm.set_debug(debug);
    match vm.run(&chunk) {
        Ok(val) => {
            if let coco_interpreter::Value::Int(code) = val {
                std::process::exit(code.to_i32().unwrap_or(1));
            }
        }
        Err(e) => {
            eprintln!("VM error: {}", e);
            std::process::exit(1);
        }
    }
}

fn cmd_build(file: &Path, native: bool, release: bool) {
    let (source, resolved) = match read_source(file) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(1);
        }
    };

    let mut parser = Parser::new(&source);
    let program = parser.parse_program();

    if native {
        #[cfg(feature = "native")]
        {
        // AOT native compilation via LLVM.
        // Reference coco_rt so it's built (its staticlib libcoco_rt.a is
        // linked by locate_coco_rt + cc below to resolve coco_rt_alloc).
        let _rt = std::marker::PhantomData::<coco_rt::CocoValue>;
        let context = inkwell::context::Context::create();
        let mut codegen = coco_codegen::Codegen::new(&context, &resolved.file_stem()
            .unwrap_or_default()
            .to_string_lossy());
        let obj_path = resolved.with_extension("o");
        let result = (|| -> Result<(), String> {
            codegen.generate(&program)?;
            codegen.compile_to_object(&obj_path.to_string_lossy())?;
            // Link: cc obj.o -o binary, linking the coco_rt static runtime
            // (provides coco_rt_alloc) produced by the coco_rt crate's
            // staticlib output.
            let bin_path = resolved.with_extension("");
            let rt_archive = locate_coco_rt();
            let mut cmd = std::process::Command::new("cc");
            cmd.arg(&obj_path);
            if let Some(rt) = &rt_archive {
                cmd.arg(rt);
            }
            cmd.arg("-o").arg(&bin_path);
            let status = cmd.status().map_err(|e| format!("failed to run linker: {}", e))?;
            if !status.success() {
                return Err("linking failed".to_string());
            }
            println!("Compiled {} -> {}", resolved.display(), bin_path.display());
            let _ = std::fs::remove_file(&obj_path);
            Ok(())
        })();
        if let Err(e) = result {
            // Clean up any partial object file on failure.
            let _ = std::fs::remove_file(&obj_path);
            eprintln!("Codegen error: {}", e);
            std::process::exit(1);
        }
        } // end #[cfg(feature = "native")]
        #[cfg(not(feature = "native"))]
        {
            eprintln!("Native compilation requires LLVM — rebuild with: cargo build --features native");
            eprintln!("Set LLVM_SYS_180_PREFIX=/usr/lib/llvm-18 and ensure Polly is available.");
            std::process::exit(1);
        }
        return;
    }

    // Bytecode disassembly mode
    let mut compiler = coco_interpreter::compiler::Compiler::new();
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

    println!("== {} (script) ==", resolved.display());
    println!("{}", coco_interpreter::ir::disassemble(&chunk, "script"));

    for val in &chunk.constants {
        if let coco_interpreter::Value::FnObj(fo) = val {
            println!("{}", coco_interpreter::ir::disassemble(&fo.chunk, &fo.name));
        }
    }
}

/// Discover and run test files in tests/*.co.
fn cmd_test(filter: Option<&str>) {
    let tests_dir = Path::new("tests");
    if !tests_dir.exists() || !tests_dir.is_dir() {
        eprintln!("error: no tests/ directory found");
        std::process::exit(1);
    }

    let mut entries: Vec<PathBuf> = match fs::read_dir(tests_dir) {
        Ok(read) => read.filter_map(|e| e.ok().map(|e| e.path()))
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
        entries.retain(|p| p.file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.to_lowercase().contains(&pattern_lower))
            .unwrap_or(false));
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

        // Compile + run via VM
        let mut compiler = coco_interpreter::compiler::Compiler::new();
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
                println!("  PASS  {}", entry.file_name().unwrap_or_default().to_string_lossy());
                passed += 1;
            }
            Err(e) => {
                println!("  FAIL  {}: {}", entry.file_name().unwrap_or_default().to_string_lossy(), e);
                failed += 1;
            }
        }
    }

    println!("\n{} passed, {} failed, {} total", passed, failed, passed + failed);
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
        let mut diag = Diagnostic::warning(file_id, format!("[{}] {}", warning.code, warning.message));
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
fn report_parser_diagnostics(source: &str, file: &Path, diagnostics: &[coco_diagnostics::Diagnostic]) {
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
