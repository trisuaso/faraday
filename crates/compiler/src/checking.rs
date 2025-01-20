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
                "{}{} = function ({})\n   return coroutine.create(function ()\n    {}\nend)\nend\n",
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

impl From<Pair<'_, Rule>> for Function {
    fn from(value: Pair<'_, Rule>) -> Self {
        let mut inner = value.into_inner();

        let mut name = String::new();
        let mut keys: Vec<String> = Vec::new();
        let mut types: Vec<Type> = Vec::new();
        let mut return_type: Type = Type::default();
        let mut visibility: TypeVisiblity = TypeVisiblity::Private;
        let mut execution: ExecutionType = ExecutionType::Sync;
        let mut body: String = String::new();

        while let Some(pair) = inner.next() {
            let rule = pair.as_rule();
            match rule {
                Rule::identifier => {
                    name = pair.as_str().to_string();
                }
                Rule::type_modifier => visibility = pair.into(),
                Rule::sync_modifier => execution = pair.into(),
                Rule::typed_parameter => {
                    let mut inner = pair.into_inner();
                    // it's safe to unwrap here because the grammar REQUIRES
                    // a type definition for arguments
                    types.push(inner.next().unwrap().into());
                    keys.push(inner.next().unwrap().as_str().to_string());
                }
                Rule::r#type => return_type = pair.into(),
                Rule::block => body = crate::process(pair.into_inner(), Registers::default()).0,
                _ => unreachable!("reached impossible rule in function processing"),
            }
        }

        // special function names
        let mut name_association_split = name.split(":");

        let struct_name = name_association_split.next().unwrap_or("");
        let true_name = name_association_split.next().unwrap_or(&name);

        if true_name == "new" {
            // imitate class
            body = format!(
                "{struct_name}.__index = {struct_name}
local self = {{}}
setmetatable(self, {struct_name})
{body}
return self"
            )
        }

        // ...
        Function {
            name: name.clone(),
            arguments: FunctionArguments { keys, types },
            return_type,
            body,
            visibility,
            execution,
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

/// A simple structure representing a field of a struct.
#[derive(Serialize, Deserialize)]
pub struct StructField {
    pub ident: String,
    pub r#type: Type,
    pub visibility: TypeVisiblity,
}

/// A simple type structure.
#[derive(Serialize, Deserialize)]
pub struct Type {
    pub ident: String,
    pub generics: Vec<String>,
    /// Registered fields on a type. Empty for regular types; populated for structs.
    pub properties: BTreeMap<String, StructField>,
    pub visibility: TypeVisiblity,
}

impl From<Pair<'_, Rule>> for Type {
    fn from(value: Pair<'_, Rule>) -> Self {
        let inner = value.into_inner();
        let mut generics: Vec<String> = Vec::new();
        let mut ident: String = String::new();
        let mut properties: BTreeMap<String, StructField> = BTreeMap::new();
        let mut visibility: TypeVisiblity = TypeVisiblity::Private;

        for pair in inner {
            let rule = pair.as_rule();
            match rule {
                Rule::generic => {
                    let inner = pair.into_inner();

                    for pair in inner {
                        if pair.as_rule() != Rule::identifier {
                            unreachable!("generics only accept identifiers, how did we get here?");
                        }

                        generics.push(pair.as_str().to_string())
                    }
                }
                Rule::type_modifier => visibility = pair.into(),
                Rule::identifier => ident = pair.as_str().to_string(),
                Rule::r#type => ident = pair.into_inner().next().unwrap().as_str().to_string(),
                Rule::struct_block => {
                    let mut inner = pair.into_inner();
                    while let Some(pair) = inner.next() {
                        let rule = pair.as_rule();
                        match rule {
                            Rule::struct_type => {
                                // last layer
                                let mut ident: String = String::new();
                                let mut r#type: Type = Type::default();
                                let mut visibility: TypeVisiblity = TypeVisiblity::Private;

                                let mut inner = pair.into_inner();
                                while let Some(pair) = inner.next() {
                                    let rule = pair.as_rule();
                                    match rule {
                                        Rule::type_modifier => visibility = pair.into(),
                                        Rule::r#type => r#type = pair.into(),
                                        Rule::identifier => ident = pair.as_str().to_string(),
                                        _ => unreachable!("reached impossible rule in struct type"),
                                    }
                                }

                                if !ident.is_empty() {
                                    properties.insert(ident.clone(), StructField {
                                        ident,
                                        r#type,
                                        visibility,
                                    });
                                }
                            }
                            _ => unreachable!("reached impossible rule in struct block"),
                        }
                    }
                }
                _ => unreachable!("reached impossible rule in type processing"),
            }
        }

        Self {
            generics,
            ident,
            properties,
            visibility,
        }
    }
}

impl Default for Type {
    fn default() -> Self {
        Self {
            ident: String::new(),
            generics: Vec::new(),
            properties: BTreeMap::new(),
            visibility: TypeVisiblity::Private,
        }
    }
}

impl ToLua for Type {
    fn transform(&self) -> String {
        format!("{}{} = {{}}\n", self.visibility, self.ident)
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
                Rule::block => {
                    args.push_str(&crate::process(pair.into_inner(), Registers::default()).0)
                }
                _ => args.push_str(pair.as_str()),
            }
        }

        if is_async {
            lua_out.push_str(&format!("select(2, coroutine.resume({ident}({args})))\n"));
        } else {
            lua_out.push_str(&format!("{ident}({args})\n"));
        }

        lua_out
    }
}

/// A standard for loop.
///
/// <https://www.lua.org/pil/4.3.5.html>
///
/// We do not support <https://www.lua.org/pil/4.3.4.html> (numeric for) at this time.
pub struct ForLoop<'a> {
    pub pair: Pair<'a, Rule>,
}

impl ToLua for ForLoop<'_> {
    fn transform(&self) -> String {
        let mut inner = self.pair.clone().into_inner();

        let mut ident: String = String::new();
        let mut iterator: String = String::new();
        let mut block: String = String::new();

        while let Some(pair) = inner.next() {
            let rule = pair.as_rule();
            match rule {
                Rule::identifier => ident = pair.as_str().to_string(),
                Rule::block => block = crate::process(pair.into_inner(), Registers::default()).0,
                _ => iterator = pair.as_str().to_string(),
            }
        }

        format!("for {ident} in {iterator} do\n{block}\nend\n")
    }
}

/// A standard while loop.
///
/// <https://www.lua.org/pil/4.3.2.html>
pub struct WhileLoop<'a> {
    pub pair: Pair<'a, Rule>,
}

impl ToLua for WhileLoop<'_> {
    fn transform(&self) -> String {
        let mut inner = self.pair.clone().into_inner();

        let mut condition: String = String::new();
        let mut block: String = String::new();

        while let Some(pair) = inner.next() {
            let rule = pair.as_rule();
            match rule {
                Rule::block => block = crate::process(pair.into_inner(), Registers::default()).0,
                _ => condition = pair.as_str().to_string(),
            }
        }

        format!("while {condition} do\n{block}\nend\n")
    }
}

/// A standard conditional (if, else, else if).
///
/// <https://www.lua.org/pil/4.3.1.html>
pub struct Conditional<'a> {
    pub pair: Pair<'a, Rule>,
}

impl ToLua for Conditional<'_> {
    fn transform(&self) -> String {
        let keyword = match self.pair.as_rule() {
            Rule::conditional_else => "else",
            Rule::conditional_elseif => "elseif",
            _ => "if",
        };

        let mut inner = self.pair.clone().into_inner();

        let mut condition: String = String::new();
        let mut block: String = String::new();

        while let Some(pair) = inner.next() {
            let rule = pair.as_rule();
            match rule {
                Rule::block => block = crate::process(pair.into_inner(), Registers::default()).0,
                Rule::conditional_else => {
                    if block.ends_with("end\n") {
                        // reopen block
                        block = block[..block.len() - 4].to_string();
                    }

                    block.push_str(&Conditional { pair }.transform())
                }
                Rule::conditional_elseif => {
                    if block.ends_with("end\n") {
                        block = block[..block.len() - 4].to_string();
                    }

                    block.push_str(&Conditional { pair }.transform())
                }
                _ => condition = pair.as_str().to_string(),
            }
        }

        format!(
            "\n{keyword} {condition}{}\n{block}\n{}",
            if keyword == "else" { "" } else { " then" },
            if !block.ends_with("end\n") {
                "end\n"
            } else {
                ""
            }
        )
    }
}
