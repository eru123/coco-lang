//! Coco compiler CLI — lex, parse, fmt, check commands.

use clap::{Parser as ClapParser, Subcommand};
use coco_diagnostics::{Diagnostic, DiagnosticLevel};
use coco_formatter::Formatter;
use coco_lexer::{Lexer, TokenKind};
use coco_parser::Parser;
use coco_safety as safety;
use coco_span::{FileId, SourceFile};
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
        Commands::Fmt { file, write } => cmd_fmt(&file, write),
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
        Commands::Build { file, native } => {
            let f = resolve_entry(file.as_deref());
            cmd_build(&f, native)
        }
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

    // Print diagnostics
    let diagnostics = parser.diagnostics();
    if !diagnostics.is_empty() {
        eprintln!("Diagnostics for {}:", resolved.display());
        for diag in diagnostics {
            eprintln!("  - {}", diag);
        }
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

fn cmd_fmt(file: &Path, write: bool) {
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
        eprintln!(
            "Warning: {} diagnostics while parsing {}:",
            diagnostics.len(),
            resolved.display()
        );
        for diag in diagnostics {
            eprintln!("  - {}", diag);
        }
    }

    let mut formatter = Formatter::new();
    let formatted = formatter.format(&program);

    if write {
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
            eprintln!(
                "{}: {} parse error(s)",
                resolved.display(),
                parser.diagnostics().len()
            );
            for diag in parser.diagnostics() {
                eprintln!("  - {}", diag);
            }
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
        if safety_result.has_warnings() {
            for w in &safety_result.warnings {
                eprintln!("warning[{}]: {}", w.code, w.message);
            }
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

fn cmd_build(file: &Path, native: bool) {
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
        // AOT native compilation via LLVM
        let context = inkwell::context::Context::create();
        let mut codegen = coco_codegen::Codegen::new(&context, &resolved.file_stem()
            .unwrap_or_default()
            .to_string_lossy());
        match codegen.generate(&program) {
            Ok(()) => {
                let obj_path = resolved.with_extension("o");
                match codegen.compile_to_object(&obj_path.to_string_lossy()) {
                    Ok(()) => {
                        // Link: cc obj.o -o binary
                        let bin_path = resolved.with_extension("");
                        let status = std::process::Command::new("cc")
                            .arg(&obj_path)
                            .arg("-o").arg(&bin_path)
                            .status();
                        match status {
                            Ok(s) if s.success() => {
                                println!("Compiled {} -> {}", resolved.display(), bin_path.display());
                                let _ = std::fs::remove_file(&obj_path);
                            }
                            _ => eprintln!("Linking failed"),
                        }
                    }
                    Err(e) => eprintln!("LLVM codegen error: {}", e),
                }
            }
            Err(e) => eprintln!("Codegen error: {}", e),
        }
        } // end #[cfg(feature = "native")]
        #[cfg(not(feature = "native"))]
        {
            eprintln!("Native compilation requires LLVM — rebuild with: cargo build --features native");
            eprintln!("Set LLVM_SYS_180_PREFIX=/usr/lib/llvm-18 and ensure Polly is available.");
        }
        return;
    }

    // Bytecode disassembly mode
    let mut compiler = coco_interpreter::compiler::Compiler::new();
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
        eprintln!(
            "{}: {} parse error(s)",
            resolved.display(),
            diagnostics.len()
        );
        for diag in diagnostics {
            eprintln!("  - {}", diag);
        }
    }

    let safety_result = safety::analyze(&program);
    let typeck_result = coco_typeck::check(&program);

    let parse_ok = diagnostics.is_empty();
    let type_ok = !typeck_result.has_errors();
    let safety_ok = !safety_result.has_errors();

    if parse_ok && type_ok && safety_ok {
        println!(
            "{}: OK ({} items parsed, types OK, safety OK)",
            resolved.display(),
            program.items.len()
        );
    } else {
        if !parse_ok {
            eprintln!("{}: parse FAILED", resolved.display());
        }
        if !type_ok {
            eprintln!(
                "{}: {} type error(s)",
                resolved.display(),
                typeck_result.error_count()
            );
        }
        if !safety_ok {
            eprintln!(
                "{}: {} safety error(s)",
                resolved.display(),
                safety_result.error_count()
            );
        }
        for w in &safety_result.warnings {
            eprintln!("  warning[{}]: {}", w.code, w.message);
        }
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
        eprintln!(
            "{}: {} parse error(s)",
            resolved.display(),
            parser.diagnostics().len()
        );
        for diag in parser.diagnostics() {
            eprintln!("  - {}", diag);
        }
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
        eprintln!(
            "{}: {} parse error(s)",
            resolved.display(),
            parser.diagnostics().len()
        );
        for diag in parser.diagnostics() {
            eprintln!("  - {}", diag);
        }
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

    if result.has_warnings() {
        for w in &result.warnings {
            eprintln!("warning[{}]: {}", w.code, w.message);
        }
    }

    println!(
        "{}: safety OK ({} warnings)",
        resolved.display(),
        result.warning_count()
    );
}

fn report_safety_errors(source: &str, file: &Path, result: &safety::SafetyResult) {
    let source_file = SourceFile::new(FileId(0), file.to_path_buf(), source.to_string());
    for error in &result.errors {
        let location = source_file.get_location(error.span.start);
        eprintln!("error[{}]: {}", error.code, error.message);
        eprintln!(
            " --> {}:{}:{}",
            file.display(),
            location.line,
            location.column
        );
    }
}

fn report_type_errors(source: &str, file: &Path, result: &coco_typeck::TypeckResult) {
    let source_file = SourceFile::new(FileId(0), file.to_path_buf(), source.to_string());
    for error in &result.errors {
        let location = source_file.get_location(error.span.start);
        eprintln!("error[{}]: {}", error.code, error.message);
        eprintln!(
            " --> {}:{}:{}",
            file.display(),
            location.line,
            location.column
        );
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
