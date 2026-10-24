pub mod ast;
pub mod bigint;
pub mod builtins;
pub mod bytecode;
pub mod compiler;
pub mod entities;
pub mod event_loop;
pub mod gc;
pub mod inspect;
pub mod io;
pub mod jit;
pub mod jsstr;
pub mod lexer;
pub mod module;
pub mod napi;
pub mod number;
pub mod object;
pub mod parser;
pub mod pm;
pub mod props;
pub mod regexp;
pub mod regexp_sets;
pub mod regexp_syntax;
pub mod runner;
pub mod token;
pub mod unicode_props;
pub mod value;
pub mod vm;

pub use value::Value;
pub use vm::Vm;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

use mimalloc as _;
