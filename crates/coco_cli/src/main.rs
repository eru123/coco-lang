//! Coco compiler CLI — lex, parse, fmt, check commands.

use clap::{Parser as ClapParser, Subcommand};
use coco_lexer::{Lexer, TokenKind};
use coco_parser::Parser;
use coco_formatter::Formatter;
use coco_interpreter::Interpreter;
use coco_syntax::Item;
use std::fs;
use std::path::PathBuf;

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
    /// Run a .co file
    Run {
        /// Path to the .co file
        file: PathBuf,
    },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Lex { file } => cmd_lex(&file),
        Commands::Parse { file } => cmd_parse(&file),
        Commands::Fmt { file, write } => cmd_fmt(&file, write),
        Commands::Check { file } => cmd_check(&file),
        Commands::Run { file } => cmd_run(&file),
    }
}

fn read_source(file: &PathBuf) -> Result<String, String> {
    fs::read_to_string(file).map_err(|e| format!("error reading file '{}': {}", file.display(), e))
}

fn cmd_lex(file: &PathBuf) {
    let source = match read_source(file) {
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

fn cmd_parse(file: &PathBuf) {
    let source = match read_source(file) {
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
        eprintln!("Diagnostics for {}:", file.display());
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

fn cmd_fmt(file: &PathBuf, write: bool) {
    let source = match read_source(file) {
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
        eprintln!("Warning: {} diagnostics while parsing {}:", diagnostics.len(), file.display());
        for diag in diagnostics {
            eprintln!("  - {}", diag);
        }
    }

    let mut formatter = Formatter::new();
    let formatted = formatter.format(&program);

    if write {
        match fs::write(file, &formatted) {
            Ok(_) => println!("Formatted {}", file.display()),
            Err(e) => eprintln!("error writing to '{}': {}", file.display(), e),
        }
    } else {
        print!("{}", formatted);
    }
}

fn cmd_run(file: &PathBuf) {
    let source = match read_source(file) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(1);
        }
    };

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

fn cmd_check(file: &PathBuf) {
    let source = match read_source(file) {
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
        println!("{}: OK ({} items parsed)", file.display(), program.items.len());
    } else {
        eprintln!("{}: {} error(s)", file.display(), diagnostics.len());
        for diag in diagnostics {
            eprintln!("  - {}", diag);
        }
    }
}
