use crate::{
    bindings::{
        FUNCTION_BINDINGS, TYPE_BINDINGS, TYPE_NAME_ANY, TYPE_NAME_STRING, TYPE_NAME_TABLE,
    },
    data::{Function, FunctionCall, Type, Variable},
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fmt::Display};

pub enum CompilerError {
    InvalidGenericCount,
    NoSuchFunction,
    NoSuchVariable,
    NoSuchProperty,
    NoSuchVariant,
    InvalidType,
    NoSuchType,
    Unknown,
}

impl Display for CompilerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use CompilerError::*;
        write!(f, "{}", match self {
            InvalidGenericCount => "invalid generic count",
            NoSuchFunction => "no such function found in registers",
            NoSuchVariable => "no such variable found in registers",
            NoSuchProperty => "no such property in struct",
            NoSuchVariant => "no such variant in enum",
            InvalidType => "invalid type for operation",
            NoSuchType => "no such type id found in registers",
            Unknown => "unknown compiler error",
        })
    }
}

pub fn fcompiler_error_print(args: std::fmt::Arguments) -> String {
    let string = if let Some(s) = args.as_str() {
        s.to_string()
    } else {
        args.to_string()
    };

    return string;
}

#[macro_export]
macro_rules! fcompiler_error {
    ($($arg:tt)*) => {
        panic!("\x1b[31;1merror:\x1b[0m \x1b[1m{}\x1b[0m", $crate::checking::fcompiler_error_print(std::format_args!($($arg)*)))
    }
}

/// Create a type error.
pub fn fcompiler_type_error(expected: String, received: String) -> ! {
    fcompiler_error!(
        "\x1b[93m{}:\x1b[0m expected \"{expected}\", received \"{received}\"",
        CompilerError::InvalidType
    )
}

/// Create a general error.
pub fn fcompiler_general_error(error: CompilerError, additional: String) -> ! {
    fcompiler_error!("\x1b[93m{error}:\x1b[0m {additional}",)
}

// traits
pub trait ToLua {
    fn transform(&self) -> String;
}

pub trait TypeChecking {
    /// Check the type of the struct vs. the `supplied` [`Type`].
    fn check(&self, supplied: Type, registers: &Registers) -> ();
}

pub trait MultipleTypeChecking {
    /// Check the type of the struct vs. the `supplied` [`Type`]s.
    fn check_multiple(&self, supplied: Vec<Type>, registers: &Registers) -> ();
}

pub trait MultipleGenericChecking {
    /// Check the generics of two [`Types`].
    fn check_generics(&self, supplied: Vec<String>, registers: &Registers) -> ();
}

// ...

/// Compiler state registers.
#[derive(Clone, Serialize, Deserialize)]
pub struct Registers {
    pub types: BTreeMap<String, Type>,
    pub functions: BTreeMap<String, Function>,
    pub variables: BTreeMap<String, Variable>,
}

impl Default for Registers {
    fn default() -> Self {
        Self {
            types: TYPE_BINDINGS.clone(),
            functions: FUNCTION_BINDINGS.clone(),
            variables: BTreeMap::default(),
        }
    }
}

impl Registers {
    pub fn get_type(&self, key: &str) -> Type {
        match self.types.get(key) {
            Some(t) => t.to_owned(),
            None => fcompiler_general_error(CompilerError::NoSuchType, key.to_string()),
        }
    }

    pub fn get_var(&self, key: &str) -> Variable {
        let mut key_split = key.split("[");
        let true_key = key_split.next().unwrap();

        let mut property_key_split = key.split(".");
        let possible_root_name = property_key_split.next().unwrap();
        if let Some(property) = property_key_split.next() {
            // being at this point means that we're trying to access a property
            // which belongs on the type of the root variable; we just need to
            // check if the type of the root variable has a property with this ident
            let var = self.get_var(possible_root_name);
            let expanded_type = self.get_type(&var.r#type.ident); // we need to expand the type to access its properties

            if expanded_type.ident != TYPE_NAME_TABLE {
                // we can access any value on tables because they work like js objects
                match expanded_type.properties.get(property) {
                    Some(property_type) => {
                        // this is just a linter, so we honestly don't care about the
                        // value of the variable... this means we can just create a new
                        // variable with an empty value
                        return (property.to_string(), property_type.r#type.clone()).into();
                    }
                    None => {
                        // check variant
                        if !expanded_type.variants.is_empty() {
                            match expanded_type.variants.get(property) {
                                Some(var) => {
                                    return var.to_owned();
                                }
                                None => {
                                    // no such property on struct
                                    fcompiler_general_error(
                                        CompilerError::NoSuchVariant,
                                        format!("{}.{}", var.r#type.ident, property),
                                    )
                                }
                            }
                        }

                        // no such property on struct
                        fcompiler_general_error(
                            CompilerError::NoSuchProperty,
                            format!("{}.{}", var.r#type.ident, property),
                        )
                    }
                }
            } else {
                // this is ONLY for table types since they don't have a predefined
                // set of properties
                return (key.to_string(), Type::from(TYPE_NAME_ANY)).into();
            }
        }

        if let Some(_) = key_split.next() {
            // being at this point means that our key contained a table index reference,
            // this means that our `true_key` is ACTUALLY the identifier of a
            // table... we need to get *that* table variable, and THEN return a
            // variable with the correct generic type
            let table = self.get_var(true_key);

            if table.r#type.ident != TYPE_NAME_TABLE {
                if table.r#type.ident == TYPE_NAME_STRING {
                    // string slices, acceptable (returns string)
                    return (key.to_string(), Type::from(TYPE_NAME_STRING)).into();
                }

                fcompiler_type_error(TYPE_NAME_TABLE.to_owned(), table.r#type.ident.clone());
            }

            return (
                key.to_string(),
                // the generic values stored in `table` is actually the values
                // of the `K, V` generics! we need to select the value of `V`
                Type::from(table.r#type.generics.get(1).unwrap().as_str()),
            )
                .into();
        }

        // return variable
        match self.variables.get(true_key) {
            Some(v) => v.to_owned(),
            None => fcompiler_general_error(CompilerError::NoSuchVariable, true_key.to_string()),
        }
    }

    /// [`get_var`] which doesn't dig through properties to find the variable.
    pub fn shallow_get_var(&self, key: &str) -> Variable {
        match self.variables.get(key) {
            Some(v) => v.to_owned(),
            None => fcompiler_general_error(CompilerError::NoSuchVariable, key.to_string()),
        }
    }

    pub fn get_fn(&self, key: &str) -> Function {
        let mut key_split = key.split(":");
        let possible_var_name = key_split.next().unwrap();

        if let Some(method) = key_split.next() {
            // being at this point means that we're trying to access a method
            // using the colon character; all we need to do is check inside
            // the parent type for the method
            let var = self.get_var(possible_var_name);
            return self.shallow_get_fn(&format!("{}:{method}", var.r#type.ident));
        }

        // return function
        match self.functions.get(key) {
            Some(f) => f.to_owned(),
            None => fcompiler_general_error(CompilerError::NoSuchFunction, key.to_string()),
        }
    }

    /// [`get_fn`] which doesn't dig through methods to find the function.
    pub fn shallow_get_fn(&self, key: &str) -> Function {
        match self.functions.get(key) {
            Some(f) => f.to_owned(),
            None => fcompiler_general_error(CompilerError::NoSuchFunction, key.to_string()),
        }
    }
}

// ...
impl TypeChecking for Variable {
    fn check(&self, supplied: Type, registers: &Registers) -> () {
        if supplied != self.r#type {
            fcompiler_type_error(self.r#type.ident.clone(), supplied.ident)
        } else {
            // check generics
            self.r#type
                .check_generics(supplied.generics.clone(), registers);
        }
    }
}

impl MultipleTypeChecking for FunctionCall<'_> {
    fn check_multiple(&self, supplied: Vec<Type>, registers: &Registers) -> () {
        let function = match registers.functions.get(&self.ident) {
            Some(f) => f,
            None => fcompiler_general_error(CompilerError::NoSuchFunction, self.ident.clone()),
        };

        for (i, r#type) in function.arguments.types.iter().enumerate() {
            let matching = match supplied.get(i) {
                Some(t) => t, // expand type
                None => continue,
            };

            let expanded = registers.get_type(&r#type.ident);
            let expanded_matching = registers.get_type(&matching.ident);
            if expanded != expanded_matching {
                fcompiler_type_error(expanded.ident.clone(), expanded_matching.ident.clone());
            } else {
                // check generics
                r#type.check_generics(matching.generics.clone(), registers);
            }
        }
    }
}

impl TypeChecking for Function {
    /// Check the **return type** of the function.
    fn check(&self, supplied: Type, registers: &Registers) -> () {
        registers.get_type(&supplied.ident);
    }
}

impl MultipleTypeChecking for Function {
    /// Check the **argument types** of the function.
    fn check_multiple(&self, supplied: Vec<Type>, registers: &Registers) -> () {
        for supplied in supplied {
            if let None = registers.types.get(&supplied.ident) {
                fcompiler_general_error(CompilerError::NoSuchType, supplied.ident)
            }
        }
    }
}

impl MultipleGenericChecking for Type {
    /// Go through all generics applied and make sure there aren't too few,
    /// too many, or invalid types.
    fn check_generics(&self, supplied: Vec<String>, registers: &Registers) -> () {
        if (supplied.len() < self.generics.len()) | (supplied.len() > self.generics.len()) {
            fcompiler_general_error(
                CompilerError::InvalidGenericCount,
                format!(
                    "expected {}, received {}",
                    self.generics.len(),
                    supplied.len()
                ),
            )
        }

        // check that all supplied types are valid
        for supplied in supplied {
            registers.get_type(&supplied);
        }
    }
}
