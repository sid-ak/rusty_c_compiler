//! The command-line interface: argv in, [`Options`] out.
//!
//! The debug flags mirror the pipeline stages, so any stage's output can be inspected in
//! isolation. They are mutually exclusive: a run stops at exactly one place.

use std::path::{Path, PathBuf};

use clap::Parser;

/// The pipeline stage after which compilation stops.
///
/// The variants are ordered as the pipeline runs, so a later stage implies every earlier one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Stage {
    /// Stop after the lexer and print the token stream (`--dump-tokens`).
    Tokens,
    /// Stop after the parser and print the AST (`--dump-ast`).
    Ast,
    /// Stop after semantic analysis, emitting nothing (`--check`).
    Check,
    /// Stop after code generation, leaving assembly (`-S`).
    Assembly,
    /// Run the whole pipeline, assemble, and link an executable.
    Executable,
}

/// Everything a single `rustycc` invocation was asked to do.
#[derive(Debug, Clone, Parser)]
#[command(name = "rustycc", version, about, long_about = None)]
pub struct Options {
    /// The C source file to compile.
    #[arg(value_name = "FILE")]
    pub input: PathBuf,

    /// Write the output to this path.
    #[arg(short = 'o', value_name = "FILE")]
    pub output: Option<PathBuf>,

    /// Stop after lexing and print the token stream.
    #[arg(long, group = "stop_after")]
    pub dump_tokens: bool,

    /// Stop after parsing and print the syntax tree.
    #[arg(long, group = "stop_after")]
    pub dump_ast: bool,

    /// Stop after semantic analysis, emitting no output.
    #[arg(long, group = "stop_after")]
    pub check: bool,

    /// Stop after code generation, leaving assembly rather than an executable.
    #[arg(short = 'S', group = "stop_after")]
    pub assembly_only: bool,

    /// Keep the intermediate files the driver would otherwise delete.
    #[arg(long)]
    pub keep_temps: bool,
}

impl Options {
    /// The stage this invocation stops after, resolved from the mutually exclusive debug flags.
    pub fn stage(&self) -> Stage {
        if self.dump_tokens {
            Stage::Tokens
        } else if self.dump_ast {
            Stage::Ast
        } else if self.check {
            Stage::Check
        } else if self.assembly_only {
            Stage::Assembly
        } else {
            Stage::Executable
        }
    }

    /// Options for compiling `input` up to `stage`, for callers driving the compiler as a library
    /// rather than through argv.
    pub fn for_source(input: &Path, stage: Stage) -> Self {
        Self {
            input: input.to_path_buf(),
            output: None,
            dump_tokens: stage == Stage::Tokens,
            dump_ast: stage == Stage::Ast,
            check: stage == Stage::Check,
            assembly_only: stage == Stage::Assembly,
            keep_temps: false,
        }
    }
}

#[cfg(test)]
mod tests;
