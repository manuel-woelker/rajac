# Compilation Pipeline

This document describes how the rajac compiler transforms Java source code into JVM bytecode, modeled after the structure and behavior of the `javac` compiler.

## Overview

The compilation pipeline consists of multiple stages that progressively transform source code from raw text to executable bytecode. Each stage has explicit inputs and outputs.

> **Note:** In the OpenJDK `javac` compiler, the Collect and Resolve phases are combined into a single phase called **Enter** (class `com.sun.tools.javac.comp.Enter`).

```
┌────────────┐
│   Source   │
│   Files    │
└─────┬──────┘
      │ .java files
      ▼
┌────────────┐
│ 1. Lexer   │
│  (Tokens)  │
└─────┬──────┘
      │ tokens
      ▼
┌────────────┐
│ 2. Parser  │
│   (AST)    │
└─────┬──────┘
      │ AST
      ▼
┌────────────┐
│ 3. Collect │
│ (Classes)  │
└─────┬──────┘
      │ class symbols
      ▼
┌────────────┐
│ 4. Resolve │
│  (Types)   │
└─────┬──────┘
      │ resolved types
      ▼
┌────────────┐
│ 5. Attribute│
│(Type Check)│
└─────┬──────┘
      │ annotated AST
      ▼
┌────────────┐
│ 6. Flow    │
│  Analysis  │
└─────┬──────┘
      │ flow-analyzed AST
      ▼
┌────────────┐
│ 7. Desugar │
│            │
└─────┬──────┘
      │ desugared AST
      ▼
┌────────────┐
│ 8. Generate│
│ (Bytecode) │
└─────┬──────┘
      │ bytecode IR
      ▼
┌────────────┐
│ 9. Write   │
│  (.class)  │
└────────────┘
```

## Stage 1: Lexical Analysis (Lexer)

**Purpose:** Convert raw source text into a stream of tokens.

**Input:**
- Source files (`.java` files) as raw UTF-8 text
- Each file is processed independently at this stage

**Output:**
- Token stream for each source file
- Tokens include: identifiers, keywords, operators, separators, literals (integer, floating-point, character, string, boolean, null), and comments (discarded)
- Line and column information for each token for error reporting

**Key responsibilities:**
- Recognize Java language constructs as tokens
- Handle Unicode escapes (`\uXXXX`)
- Handle line terminators (`\n`, `\r`, `\r\n`) for line number tracking
- Process string literals, including escape sequences (`\n`, `\t`, `\\`, etc.)
- Handle integer literals with suffixes (`L` for long)
- Handle floating-point literals with suffixes (`f`, `d` for float/double)
- Validate token formation (e.g., illegal character sequences)

## Stage 2: Parsing (Syntactic Analysis)

**Purpose:** Transform token stream into an Abstract Syntax Tree (AST) that represents the program structure.

**Input:**
- Token stream from the lexer

**Output:**
- AST nodes representing the program structure
- Each node corresponds to a syntactic construct (package declaration, import statements, class declarations, method declarations, statements, expressions, etc.)
- The AST preserves the hierarchical structure of the source code

**Key responsibilities:**
- Verify that tokens form valid Java syntax according to the language grammar
- Build a tree structure where:
  - Root nodes represent compilation units
  - Class declarations contain field and method nodes
  - Method declarations contain parameter and body nodes
  - Statements are nested according to control flow structures
- Perform basic error recovery (attempt to continue parsing after syntax errors)
- Handle ambiguous grammar constructs using context (e.g., generic type vs. shift expression)

**AST node types include:**
- `CompilationUnit` - the root node for a source file
- `PackageDeclaration` - package statement
- `ImportDeclaration` - import statements
- `ClassDeclaration` - class, interface, enum, or record definitions
- `MethodDeclaration` - method or constructor definitions
- `VariableDeclaration` - field or local variable declarations
- `Statement` - control flow statements (if, while, for, switch, etc.)
- `Expression` - expressions (method invocation, field access, binary operations, etc.)

## Stage 3: Collect

**Purpose:** Collect all class, interface, enum, and record declarations into the symbol table without resolving any type references.

**Input:**
- AST from the parser

**Output:**
- `ClassSymbol` entries for every named type in all compilation units:
  - Top-level classes, interfaces, enums, records
  - Member classes (static nested)
  - Inner classes (non-static nested)
  - Local classes (declared in method bodies)
  - Anonymous classes (synthetic names, e.g., `Outer$1`)
- Packages with their compilation units mapped
- Symbols attached to AST nodes via the `sym` field (class symbols at minimum)
- Names are fully qualified (e.g., `com.example.MyClass`)

**Key responsibilities:**
- Walk all compilation units and declare each named type
- Recursively handle nested classes (member and local)
- Assign synthetic names to anonymous classes
- Process import declarations to map simple names to qualified names
- Do NOT resolve any type references (extends, implements, field types, method signatures)
- Do NOT enter method or field symbols yet

**Why two passes?**
Collecting all classes first enables simple deterministic resolution in the next phase. Since all classes are known, forward references (class B extends class A where A is defined later) are trivial to handle - the target class already exists in the table.

## Stage 4: Resolve

**Purpose:** Resolve all type references using the complete set of known classes.

**Input:**
- AST from Stage 3 (with class symbols in the table)
- Complete symbol table with all classes, interfaces, enums, records

**Output:**
- Fully resolved symbol table:
  - Class symbols have their `superclass` and `interfaces` fields populated
  - Method symbols with parameters and return type resolved
  - Field symbols with types resolved
  - Type parameter symbols with bounds resolved
- Symbol objects fully populated for all declarations

**Key responsibilities:**
- Resolve superclass (`extends`) and implemented interfaces (`implements`)
- Resolve field types (including generic types)
- Resolve method return types and parameter types
- Resolve generic type parameters and their bounds
- Enter method symbols into their enclosing class scope
- Enter field symbols into their enclosing class scope
- Enter local variable symbols into their enclosing method scope
- Handle type parameter shadowing

**Symbol types:**
- `PackageSymbol` - represents a package
- `ClassSymbol` - represents a class, interface, enum, or record
- `MethodSymbol` - represents a method or constructor
- `VarSymbol` - represents a field or local variable
- `TypeParameterSymbol` - represents a type parameter

## Stage 5: Attribute Analysis (Type Checking)

**Purpose:** Perform semantic analysis including type checking, constant evaluation, and overload resolution.

**Input:**
- AST with symbol table entries from the Resolve phase

**Output:**
- Annotated AST with:
  - Types computed and attached to expression nodes (via `type` field)
  - Method resolution results (attached to method invocation nodes)
  - Cast conversion information
  - Constant values computed where applicable
- Detection and reporting of type errors (incompatible types, unreachable statements, etc.)

**Key responsibilities:**
- Type checking for all expressions
- Validate assignment compatibility
- Check method invocation applicability and resolve overloads
- Validate cast expressions
- Compute the type of each expression (primitive types, reference types, array types)
- Process generic type arguments and type bounds
- Validate enum constant definitions
- Check annotation semantics

**Type system:**
- Primitive types: `byte`, `short`, `int`, `long`, `char`, `float`, `double`, `boolean`
- Reference types: class types, interface types, array types, type variables
- The `Type` class hierarchy mirrors the Java type system

## Stage 5: Flow Analysis

**Purpose:** Perform data-flow analysis to verify program correctness and enforce language rules.

**Input:**
- Annotated AST from the Attribute phase

**Output:**
- Augmented AST with flow analysis results:
  - Assignable variables set for each statement
  - Exception flow information
  - Reachability determination
- Errors for:
  - Uninitialized variable usage
  - Unreachable statements
  - Non-exhaustive switch expressions (when required)
  - Invalid break/continue statements

**Key responsibilities:**
- Definite assignment analysis - ensure variables are assigned before use
- Definite unassignment analysis - ensure final variables are not assigned in loops
- Reachability analysis - verify statements can be executed
- Exception checking - verify caught exceptions are declared or checked
- Resource cleanup analysis (try-with-resources)

## Stage 6: Desugar (Translation to Lower-Level Constructs)

**Purpose:** Translate high-level language features into equivalent lower-level constructs.

**Input:**
- AST with full type information from the Flow phase

**Output:**
- Simplified AST representing the desugared program:
  - Lambda expressions converted to functional interface instantiations
  - Switch expressions converted to switch statements
  - Enhanced for-loops converted to iterator-based loops
  - Type annotations removed (post-processing)
  - Pattern matching constructs converted to equivalent code

**Key responsibilities:**
- Convert lambda expressions to `invokedynamic` calls or inner classes
- Desugar switch expressions to switch statements with variable assignment
- Convert foreach loops to iterator loops
- Handle yield statements in switch expressions
- Desugar records to equivalent class structure
- Convert sealed classes to appropriate modifiers

## Stage 7: Generate (Bytecode Emission)

**Purpose:** Transform the desugared AST into JVM bytecode.

**Input:**
- Desugared AST with complete type information

**Output:**
- `.class` files containing:
  - Constant pool
  - Field info
  - Method info (including code attribute with bytecode)
  - Attributes (SourceFile, RuntimeVisibleAnnotations, etc.)

**Key responsibilities:**
- Generate constant pool entries for:
  - Class and interface names
  - Field references
  - Method references
  - String literals
  - Numeric literals
- Generate bytecode instructions for each AST node:
  - Load/store instructions for variables
  - Arithmetic instructions
  - Control flow instructions (if, goto, tableswitch, lookupswitch)
  - Method invocation instructions (`invokevirtual`, `invokestatic`, `invokeinterface`, `invokespecial`)
  - Field access instructions (`getfield`, `putfield`, `getstatic`, `putstatic`)
  - Array instructions (`newarray`, `arraylength`, `arrayload`, `arraystore`)
  - Object creation instructions (`new`, `dup`)
  - Type checking instructions (`instanceof`, `checkcast`)
  - Exception handling (`athrow`, try-catch blocks)
- Compute and emit stack map frames for bytecode verification
- Handle synthetic and bridge methods
- Generate debugging information (optional line numbers, local variable tables)

**Bytecode instruction categories:**
- Stack operations: `pop`, `dup`, `swap`
- Load/store: `iload`, `aload`, `istore`, `astore`
- Arithmetic: `iadd`, `isub`, `imul`, `idiv`, `irem`, etc.
- Comparison: `if_icmpeq`, `if_acmpne`, etc.
- Control flow: `goto`, `jsr`, `tableswitch`, `lookupswitch`
- Invocation: `invokevirtual`, `invokeinterface`, `invokestatic`, `invokespecial`
- Field access: `getfield`, `putfield`, `getstatic`, `putstatic`
- Object creation: `new`, `dup`, ` invokespecial`
- Type conversion: `i2l`, `i2f`, `i2d`, etc.

## Parallel Compilation

The compiler supports compiling multiple source files concurrently. The dependency graph determines the order:

1. **Parse/Collect/Resolve** can run in parallel for independent files
2. **Flow Analysis** requires complete attribution
3. **Generation** requires complete flow analysis
4. **Output** is serialized per class file

A class `C` depends on class `D` if:
- `C` extends or implements `D`
- `C` references a static field of `D`
- `C` invokes a static method of `D`
- `C` creates an instance of `D`

## Error Handling

Each stage reports errors with:
- Source file and line number
- Column position
- Error message describing the issue
- Optional suggestions for correction

The compiler continues processing to report multiple errors in a single run, but the output (`.class` files) is only produced if compilation succeeds without errors (or if explicitly requested with `-force`).

## Implementation Notes

- Use visitor patterns for AST traversal where appropriate
- Store source positions on all nodes for error reporting
- Maintain a symbol table as a hash map keyed by fully qualified names
- Lazy evaluation where possible to improve performance
- Cache type resolution results to avoid redundant computation