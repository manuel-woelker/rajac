/* 📖 # Why have a stages module?
The compilation process is naturally divided into distinct stages:
1. Discovery - finding Java source files
2. Parsing - converting source code to AST
3. Collection - building symbol tables
4. Resolution - resolving identifiers and types
5. Generation - emitting bytecode

Separating these into modules makes the code more organized,
easier to test individual stages, and clearer to understand
the compilation pipeline flow.
*/

pub mod collection;
pub mod discovery;
pub mod generation;
pub mod parsing;
pub mod resolution;
