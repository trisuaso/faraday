pub use pest::{Parser, iterators::Pair, iterators::Pairs};
use pest_derive::Parser;

#[derive(Parser)]
#[grammar = "grammar.pest"]
pub struct FaradayParser {}
