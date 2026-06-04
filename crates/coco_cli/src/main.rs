//! Coco compiler CLI — lex, parse, fmt, check commands.

use clap::{Parser as ClapParser, Subcommand};
use coco_formatter::Formatter;
use coco_interpreter::Interpreter;
use coco_lexer::{Lexer, TokenKind};
use coco_parser::Parser;
use coco_span::{FileId, SourceFile};
use coco_syntax::Item;
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
    /// Run a .co file
    Run {
        /// Path to the .co file
        file: PathBuf,
        /// Skip type checking before execution
        #[arg(long = "no-check")]
        no_check: bool,
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
        Commands::Run { file, no_check } => cmd_run(&file, no_check),
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

fn cmd_run(file: &Path, no_check: bool) {
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
    }

    let mut interp = Interpreter::new();
    match interp.run_main(&source) {
        Ok(val) => {
            if let coco_interpreter::Value::Int(code) = val {
                std::process::exit(code as i32);
            }
        }
        Err(e) => {
            eprintln!("Runtime error: {}", e);
            std::process::exit(1);
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
    if diagnostics.is_empty() {
        println!(
            "{}: OK ({} items parsed)",
            resolved.display(),
            program.items.len()
        );
    } else {
        eprintln!("{}: {} error(s)", resolved.display(), diagnostics.len());
        for diag in diagnostics {
            eprintln!("  - {}", diag);
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
