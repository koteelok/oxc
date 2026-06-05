#![expect(clippy::print_stdout)]
//! # React Compiler Plugin Example
//!
//! Runs the React Compiler ([facebook/react#36173]) plugin over a file and
//! prints the memoized output.
//!
//! Unlike a `Traverse`-based plugin, the React Compiler is a standalone pass:
//! [`react_compiler::run`] rewrites the whole program in place and returns the
//! [`Scoping`](oxc_semantic::Scoping) the rest of the pipeline should use. In a
//! full transform it runs **first**, on the pristine AST, and the returned
//! scoping is handed to [`oxc_transformer::Transformer`] for the remaining
//! transforms (JSX, ES lowering, ...).
//!
//! ## Usage
//!
//! ```bash
//! cargo run -p oxc_transformer_plugins --example react_compiler             # built-in sample component
//! cargo run -p oxc_transformer_plugins --example react_compiler -- MyFile.jsx  # or pass a file path
//! ```
//!
//! [facebook/react#36173]: https://github.com/facebook/react/pull/36173

use std::path::Path;

use pico_args::Arguments;

use oxc_allocator::Allocator;
use oxc_ast::ast::Program;
use oxc_codegen::Codegen;
use oxc_diagnostics::OxcDiagnostic;
use oxc_parser::Parser;
use oxc_semantic::SemanticBuilder;
use oxc_span::SourceType;
use oxc_transformer_plugins::react_compiler::{self, default_plugin_options};

const DEFAULT_SOURCE: &str = "function Component(props) {
  return <div onClick={() => props.onClick()}>{props.text}</div>;
}
";

fn main() -> std::io::Result<()> {
    let mut args = Arguments::from_env();

    let (source_text, source_type) = match args.free_from_str::<String>() {
        Ok(name) => {
            let path = Path::new(&name);
            let source = std::fs::read_to_string(path)?;
            let source_type = SourceType::from_path(path).unwrap_or_else(|_| SourceType::tsx());
            (source, source_type)
        }
        // No file given: compile the built-in sample component.
        Err(_) => (DEFAULT_SOURCE.to_string(), SourceType::tsx()),
    };

    let allocator = Allocator::default();
    let mut program = parse(&allocator, &source_text, source_type);
    let scoping = SemanticBuilder::new().build(&program).semantic.into_scoping();

    // Run the React Compiler on the pristine AST. `run` mutates `program` in
    // place; the returned scoping is what downstream transforms would consume.
    let mut errors: Vec<OxcDiagnostic> = Vec::new();
    let _scoping = react_compiler::run(
        &mut program,
        &allocator,
        scoping,
        &default_plugin_options(),
        &mut errors,
    );

    for error in errors {
        println!("{:?}", error.with_source_code(source_text.clone()));
    }

    println!("{}", Codegen::new().build(&program).code);

    Ok(())
}

/// Parse JavaScript/TypeScript source code into an AST.
fn parse<'a>(
    allocator: &'a Allocator,
    source_text: &'a str,
    source_type: SourceType,
) -> Program<'a> {
    let ret = Parser::new(allocator, source_text, source_type).parse();
    for error in ret.errors {
        println!("{:?}", error.with_source_code(source_text.to_string()));
    }
    ret.program
}
