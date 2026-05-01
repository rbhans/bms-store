//! Xeto language support — parser, AST, runtime loader.
//!
//! Step 1 ships only the pinned upstream version constant. The parser and
//! loader land in step 2 (build-time generation) and step 5 (runtime loader).

pub mod loader;
pub mod parser;
pub mod version;

pub use loader::{HaystackNamespace, SharedNamespace};
pub use parser::{parse_lib_dir, parse_source, ParseError, ParsedLib, RuntimeGlobal, RuntimeSpec};
