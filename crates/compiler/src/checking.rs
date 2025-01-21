use crate::data::{Function, FunctionCall, Type, Variable};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub trait ToLua {
    fn transform(&self) -> String;
}

pub trait TypeChecking {
    /// Check the type of the struct vs. the `supplied` [`Type`].
    fn check(&self, supplied: Type, registers: &Registers) -> bool;
}

pub trait MultipleTypeChecking {
    /// Check the type of the struct vs. the `supplied` [`Type`]s.
    fn check(&self, supplied: Vec<Type>, registers: &Registers) -> bool;
}

/// Compiler state registers.
#[derive(Serialize, Deserialize)]
pub struct Registers {
    pub types: BTreeMap<String, Type>,
    pub functions: BTreeMap<String, Function>,
    pub variables: BTreeMap<String, Variable>,
}

impl Default for Registers {
    fn default() -> Self {
        Self {
            types: BTreeMap::default(),
            functions: BTreeMap::default(),
            variables: BTreeMap::default(),
        }
    }
}

// ...
impl TypeChecking for Variable {
    fn check(&self, supplied: Type, _registers: &Registers) -> bool {
        supplied == self.r#type
    }
}

impl MultipleTypeChecking for FunctionCall {
    fn check(&self, supplied: Vec<Type>, registers: &Registers) -> bool {
        let function = match registers.functions.get(&self.ident) {
            Some(f) => f,
            None => return false,
        };

        for (i, r#type) in function.arguments.types.iter().enumerate() {
            let matching = match supplied.get(i) {
                Some(t) => t,
                None => return false,
            };

            if r#type != matching {
                return false;
            }
        }

        true
    }
}
