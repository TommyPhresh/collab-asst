//! Config expression language. Steps:
//!
//! `lexer` converts str to tokens
//! `parser` converts tokens to AST
//! `types` validates
//! `eval` creates planner-time

#![allow(dead_code)]

pub mod lexer;

