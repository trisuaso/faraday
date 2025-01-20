use parser::{Pair, Rule};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fmt::Display};

pub trait ToLua {
    fn transform(&self) -> String;
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

/// The parameter supplied to a function during creation.
#[derive(Serialize, Deserialize)]
pub struct FunctionArguments {
    pub keys: Vec<String>,
    pub types: Vec<Type>,
}

impl FunctionArguments {
    /// Get the idenfitier and required type of a function parameter by its index.
    pub fn get(&self, index: usize) -> Option<(&String, &Type)> {
        if let Some(value) = self.keys.get(index) {
            return Some((value, self.types.get(index).unwrap()));
        }

        None
    }
}

/// Async/sync modifiers for [`Function`]s.
#[derive(Serialize, Deserialize, PartialEq, Eq)]
pub enum ExecutionType {
    /// <https://www.lua.org/pil/9.1.html>
    Async,
    Sync,
}

impl From<Pair<'_, Rule>> for ExecutionType {
    fn from(value: Pair<Rule>) -> Self {
        match value.as_str() {
            "async" => ExecutionType::Async,
            "sync" => ExecutionType::Sync,
            _ => unreachable!("reached impossible execution type value"),
        }
    }
}

/// A typed function definition.
#[derive(Serialize, Deserialize)]
pub struct Function {
    pub name: String,
    pub arguments: FunctionArguments,
    pub return_type: Type,
    pub body: String,
    pub visibility: TypeVisiblity,
    pub execution: ExecutionType,
}

impl Function {
    pub fn args_string(&self) -> String {
        let mut lua_out: String = String::new();

        for (i, param) in self.arguments.keys.clone().iter().enumerate() {
            if i != self.arguments.keys.len() - 1 {
                lua_out.push_str(&format!("{param}, "));
            } else {
                lua_out.push_str(&format!("{param}"));
            }
        }

        lua_out
    }
}

impl ToLua for Function {
    fn transform(&self) -> String {
        if self.execution == ExecutionType::Async {
            // async coroutine function
            format!(
                "{}{} = coroutine.create(function ({})\n    {}\nend)\n",
                self.visibility.to_string(),
                self.name,
                self.args_string(),
                self.body
            )
        } else {
            // regular, sync function
            format!(
                "{}function {}({})\n    {}\nend\n",
                self.visibility.to_string(),
                self.name,
                self.args_string(),
                self.body
            )
        }
    }
}

/// A variable binding.
#[derive(Serialize, Deserialize)]
pub struct Variable {
    pub name: String,
    pub r#type: Type,
    pub value: String,
    pub visibility: TypeVisiblity,
}

impl ToLua for Variable {
    fn transform(&self) -> String {
        format!("{}{} = {}\n", self.visibility, self.name, self.value)
    }
}

/// A simple type structure.
#[derive(Serialize, Deserialize)]
pub struct Type {
    pub ident: String,
    pub generics: Vec<String>,
}

impl From<Pair<'_, Rule>> for Type {
    fn from(value: Pair<'_, Rule>) -> Self {
        let inner = value.into_inner();
        let mut generics: Vec<String> = Vec::new();
        let mut ident: String = String::new();

        for pair in inner {
            let rule = pair.as_rule();
            match rule {
                parser::Rule::generic => {
                    let inner = pair.into_inner();

                    for pair in inner {
                        if pair.as_rule() != Rule::identifier {
                            unreachable!("generics only accept identifiers, how did we get here?");
                        }

                        generics.push(pair.as_str().to_string())
                    }
                }
                parser::Rule::identifier => ident = pair.as_str().to_string(),
                _ => unreachable!("reached impossible rule in type processing"),
            }
        }

        Self { generics, ident }
    }
}

impl Default for Type {
    fn default() -> Self {
        Self {
            ident: String::new(),
            generics: Vec::new(),
        }
    }
}

impl ToLua for Type {
    fn transform(&self) -> String {
        String::new()
    }
}

/// The visibility of a type (<https://www.lua.org/pil/14.2.html>).
#[derive(Serialize, Deserialize)]
pub enum TypeVisiblity {
    Public,
    Private,
}

impl From<Pair<'_, Rule>> for TypeVisiblity {
    fn from(value: Pair<Rule>) -> Self {
        match value.as_str() {
            "pub" => TypeVisiblity::Public,
            "prv" => TypeVisiblity::Private,
            _ => unreachable!("reached impossible type modifier value"),
        }
    }
}

impl Display for TypeVisiblity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", match self {
            Self::Public => "",
            Self::Private => "local ",
        })
    }
}

/// A call to a stored function.
#[derive(Serialize)]
pub struct FunctionCall<'a> {
    pub pair: Pair<'a, Rule>,
}

impl ToLua for FunctionCall<'_> {
    fn transform(&self) -> String {
        let mut lua_out: String = String::new();
        let mut inner = self.pair.clone().into_inner();

        let mut ident: String = String::new();
        let mut args: String = String::new();
        let mut is_async: bool = false;

        while let Some(pair) = inner.next() {
            let rule = pair.as_rule();
            match rule {
                Rule::identifier => {
                    if ident.is_empty() {
                        let string = pair.as_str().to_string();
                        is_async = string.starts_with("#");
                        ident = string.replacen("#", "", 1)
                    } else {
                        args.push_str(pair.as_str());
                    }
                }
                Rule::block => args.push_str(&crate::process(pair.into_inner()).0),
                _ => args.push_str(pair.as_str()),
            }
        }

        if is_async {
            lua_out.push_str(&format!("coroutine.resume({ident}, {args}) "));
        } else {
            lua_out.push_str(&format!("{ident}({args}) "));
        }

        lua_out
    }
}
