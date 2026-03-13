mod decl;
mod expr;
mod parser;
mod stmt;

pub use parser::{ParseResult, Parser};

use rajac_base::file_path::FilePath;

pub fn parse(source: &str, path: FilePath) -> ParseResult {
    let lexer = rajac_lexer::Lexer::new(source, path);
    let parser = Parser::new(lexer, source);
    parser.parse_compilation_unit()
}
