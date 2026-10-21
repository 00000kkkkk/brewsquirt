pub mod ast;
pub mod builtins;
pub mod bytecode;
pub mod compiler;
pub mod event_loop;
pub mod gc;
pub mod inspect;
pub mod io;
pub mod lexer;
pub mod module;
pub mod number;
pub mod object;
pub mod parser;
pub mod pm;
pub mod regexp;
pub mod runner;
pub mod token;
pub mod value;
pub mod vm;

pub use value::Value;
pub use vm::Vm;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
