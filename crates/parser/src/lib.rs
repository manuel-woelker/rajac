mod decl;
mod expr;
mod parser;
mod stmt;

pub use parser::{ParseResult, Parser};

pub fn parse(source: &str) -> ParseResult {
    let lexer = rajac_lexer::Lexer::new(source);
    let parser = Parser::new(lexer, source);
    parser.parse_compilation_unit()
}
