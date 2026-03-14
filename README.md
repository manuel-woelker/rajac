# rajac

rajac is an alternative Java compiler written in Rust.

Note: This project is still in development and not ready for use.

## Overview

This project aims to provide a high-performance, memory-efficient Java compiler implementation using modern Rust practices. The compiler is designed to be modular and extensible while maintaining compatibility with standard Java bytecode.

## Features

- **Fast compilation**: Optimized for performance with Rust's zero-cost abstractions
- **Memory efficient**: Careful memory management using `SharedString` and other optimizations
- **Modular architecture**: Clean separation of concerns across multiple crates
- **Java compatibility**: Generates standard bytecode compatible with JVM

## Architecture

The project is organized as a Cargo workspace with the following main crates:

- `rajac-compiler` - Main compiler entry point
- `rajac-ast` - Abstract syntax tree definitions
- `rajac-parser` - Java source code parsing
- `rajac-lexer` - Tokenization of Java source code
- `rajac-bytecode` - Bytecode generation and manipulation
- `rajac-diagnostics` - Error reporting and diagnostics
- `rajac-symbols` - Symbol table and type resolution
- `rajac-classpath` - Classpath handling
- `rajac-base` - Shared utilities and types
- `rajac-verification` - Compiler correctness verification

## Getting Started

### Prerequisites

- Rust 1.70+ (recommended latest stable)
- Java runtime for testing compiled output

### Building

```bash
# Build in debug mode (faster compilation)
cargo build

# Build in release mode (optimized binary)
cargo build --release
```

### Running the Compiler

```bash
# Compile Java source files (debug mode)
cargo run -p rajac-compiler -- <source-directory>

# Compile Java source files (release mode)
cargo run --release -p rajac-compiler -- <source-directory>
```

## Development

### Running Tests

```bash
# Run all tests
cargo test

# Run tests with output
cargo test -- --nocapture

# Run specific crate tests
cargo test -p rajac-parser
```

### Code Quality

Run the code quality checks:

```bash
./scripts/check-code.sh
```

This will run formatting, linting, and tests to ensure code quality.

### Project Structure

- Each struct, enum, and trait has its own file
- Tests are colocated with the code they test
- Documentation follows question-driven format
- Use `SharedString` instead of `String` for struct fields

## Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Run `./scripts/check-code.sh` to verify quality
5. Commit using conventional commit format
6. Submit a pull request

## Status

This is an active development project.
