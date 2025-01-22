use crate::bindings::*;
use crate::checking::{
    CompilerError, MultipleGenericChecking, MultipleTypeChecking, Registers, ToLua, TypeChecking,
    fcompiler_general_error,
};
use crate::fcompiler_error;
use parser::{Pair, Rule};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fmt::Display};

/// The parameter supplied to a function during creation.
#[derive(Clone, Debug, Serialize, Deserialize)]
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
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
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
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Function {
    pub name: String,
    pub arguments: FunctionArguments,
    pub return_type: Type,
    pub body: String,
    pub visibility: TypeVisibility,
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

impl From<(Pair<'_, Rule>, &Registers)> for Function {
    fn from(value: (Pair<'_, Rule>, &Registers)) -> Self {
        let reg = value.1;
        let mut inner = value.0.into_inner();

        let mut name = String::new();
        let mut keys: Vec<String> = Vec::new();
        let mut types: Vec<Type> = Vec::new();
        let mut return_type: Type = Type::default();
        let mut visibility: TypeVisibility = TypeVisibility::Private;
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
                Rule::block => {
                    body = crate::process(pair.into_inner(), {
                        // we must update the registries with the arguments in order
                        // to allow the body to pass the type check
                        let mut reg = reg.clone();

                        for (k, t) in std::iter::zip(&keys, &types) {
                            reg.variables
                                .insert(k.clone(), (k.clone(), t.to_owned()).into());
                        }

                        reg
                    })
                    .0
                }
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
        let fun = Function {
            name: name.clone(),
            arguments: FunctionArguments { keys, types },
            return_type,
            body,
            visibility,
            execution,
        };

        fun.check(fun.return_type.clone(), reg);
        fun.check_multiple(fun.arguments.types.clone(), reg);

        fun
    }
}

/// A variable binding.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Variable {
    pub ident: String,
    pub r#type: Type,
    pub value: String,
    pub visibility: TypeVisibility,
}

impl ToLua for Variable {
    fn transform(&self) -> String {
        format!("{}{} = {}\n", self.visibility, self.ident, self.value)
    }
}

impl From<(String, Type)> for Variable {
    fn from(value: (String, Type)) -> Self {
        Self {
            ident: value.0,
            r#type: value.1,
            value: String::new(),
            visibility: TypeVisibility::Private,
        }
    }
}

impl From<Pair<'_, Rule>> for Variable {
    fn from(value: Pair<'_, Rule>) -> Self {
        let mut inner = value.into_inner();

        let mut name = String::new();
        let mut r#type = Type::default();
        let mut value: String = String::new();
        let mut visibility: TypeVisibility = TypeVisibility::Private;

        while let Some(pair) = inner.next() {
            let rule = pair.as_rule();
            match rule {
                Rule::identifier => {
                    name = pair.as_str().to_string();
                }
                Rule::type_modifier => {
                    visibility = pair.into();
                }
                Rule::r#type => r#type = pair.into(),
                _ => {
                    value = match rule {
                        // process blocks before using as value
                        Rule::block => crate::process(pair.into_inner(), Registers::default()).0,
                        // everything else just needs to be stringified
                        Rule::call => {
                            fcompiler_error!("{}", "cannot do compiler call in an enum")
                        }
                        _ => pair.as_str().to_string(),
                    }
                }
            }
        }

        Variable {
            ident: name.clone(),
            r#type,
            value,
            visibility,
        }
    }
}

impl From<(Pair<'_, Rule>, &Registers)> for Variable {
    fn from(value: (Pair<'_, Rule>, &Registers)) -> Self {
        let reg = value.1;
        let mut inner = value.0.into_inner();

        let mut name = String::new();
        let mut r#type = Type::default();
        let mut value: String = String::new();
        let mut visibility: TypeVisibility = TypeVisibility::Private;

        while let Some(pair) = inner.next() {
            let rule = pair.as_rule();
            match rule {
                Rule::identifier => {
                    name = pair.as_str().to_string();
                }
                Rule::type_modifier => {
                    visibility = pair.into();
                }
                Rule::r#type => r#type = (pair, reg).into(),
                _ => {
                    value = match rule {
                        // process blocks before using as value
                        Rule::block => crate::process(pair.into_inner(), Registers::default()).0,
                        // everything else just needs to be stringified
                        Rule::call => {
                            let call = FunctionCall::from(pair);
                            let supplied_types = call.arg_types(reg);
                            call.check_multiple(supplied_types, reg);

                            // check function return type
                            let function = reg.get_fn(&call.ident);
                            if function.return_type != r#type {
                                fcompiler_general_error(
                                    CompilerError::InvalidType,
                                    format!(
                                        "cannot assign \"{}\" to \"{}\"",
                                        function.return_type.ident, r#type.ident
                                    ),
                                )
                            }

                            // ...
                            call.transform()
                        }
                        _ => {
                            let t = Type::from_parser_type(pair.clone(), reg);

                            if t != r#type {
                                fcompiler_general_error(
                                    CompilerError::InvalidType,
                                    format!(
                                        "cannot assign \"{}\" to \"{}\"",
                                        t.ident, r#type.ident
                                    ),
                                )
                            }

                            pair.as_str().to_string()
                        }
                    }
                }
            }
        }

        Variable {
            ident: name.clone(),
            r#type,
            value,
            visibility,
        }
    }
}

/// A simple structure representing a field of a struct.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StructField {
    pub ident: String,
    pub r#type: Type,
    pub visibility: TypeVisibility,
}

/// A simple type structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Type {
    pub ident: String,
    pub generics: Vec<String>,
    /// Registered fields on a type. Empty for regular types; populated for structs.
    pub properties: BTreeMap<String, StructField>,
    pub variants: BTreeMap<String, Variable>,
    pub visibility: TypeVisibility,
}

impl PartialEq for Type {
    fn eq(&self, other: &Self) -> bool {
        // "any" types are always equal to anything
        if (self.ident == "any") | (other.ident == "any") {
            return true;
        }

        // we don't need to check the visibility of types to see if they're equal
        // generics are checked through [`MultipleGenericChecking`] trait
        // (self.ident == other.ident) && (self.properties == other.properties)
        self.ident == other.ident
    }
}

impl Eq for Type {
    fn assert_receiver_is_total_eq(&self) {
        assert!(true == true)
    }
}

impl Type {
    /// Get a [`Type`] given a parser [`Pair`]. Resolves register references.
    pub fn from_parser_type(pair: Pair<'_, Rule>, registers: &Registers) -> Self {
        let rule = pair.as_rule();
        match rule {
            Rule::string => (TYPE_NAME_STRING, TypeVisibility::Public).to_owned().into(),
            Rule::int => (TYPE_NAME_INT, TypeVisibility::Public).to_owned().into(),
            Rule::float => (TYPE_NAME_FLOAT, TypeVisibility::Public).to_owned().into(),
            Rule::identifier => {
                // since this is a variable reference, we must get the type of that
                // variable from the registers
                let variable = registers.get_var(pair.as_str());
                variable.r#type.clone()
            }
            Rule::call => {
                // since this is a function call, we must get the return type of
                // the function that is being called
                let mut inner = pair.into_inner();
                let ident = inner
                    .next()
                    .expect("function call requires a function ident to call");

                let function = registers.get_fn(ident.as_str());
                function.return_type.clone()
            }
            Rule::table => (
                TYPE_NAME_TABLE,
                vec!["any".to_string(), "any".to_string()],
                TypeVisibility::Public,
            )
                .into(),
            _ => fcompiler_error!("unknown parser type (could not translate to compiler type)"),
        }
    }
}

impl From<String> for Type {
    fn from(value: String) -> Self {
        Self {
            ident: value,
            generics: Vec::new(),
            properties: BTreeMap::new(),
            variants: BTreeMap::new(),
            visibility: TypeVisibility::Private,
        }
    }
}

impl From<&str> for Type {
    fn from(value: &str) -> Self {
        Self {
            ident: value.to_owned(),
            generics: Vec::new(),
            properties: BTreeMap::new(),
            variants: BTreeMap::new(),
            visibility: TypeVisibility::Private,
        }
    }
}

impl From<(String, TypeVisibility)> for Type {
    fn from(value: (String, TypeVisibility)) -> Self {
        Self {
            ident: value.0,
            generics: Vec::new(),
            properties: BTreeMap::new(),
            variants: BTreeMap::new(),
            visibility: value.1,
        }
    }
}

impl From<(&str, TypeVisibility)> for Type {
    fn from(value: (&str, TypeVisibility)) -> Self {
        Self {
            ident: value.0.to_owned(),
            generics: Vec::new(),
            properties: BTreeMap::new(),
            variants: BTreeMap::new(),
            visibility: value.1,
        }
    }
}

impl From<(String, Vec<String>, TypeVisibility)> for Type {
    fn from(value: (String, Vec<String>, TypeVisibility)) -> Self {
        Self {
            ident: value.0,
            generics: value.1,
            properties: BTreeMap::new(),
            variants: BTreeMap::new(),
            visibility: value.2,
        }
    }
}

impl From<(&str, Vec<String>, TypeVisibility)> for Type {
    fn from(value: (&str, Vec<String>, TypeVisibility)) -> Self {
        Self {
            ident: value.0.to_owned(),
            generics: value.1,
            properties: BTreeMap::new(),
            variants: BTreeMap::new(),
            visibility: value.2,
        }
    }
}

impl From<Pair<'_, Rule>> for Type {
    fn from(value: Pair<'_, Rule>) -> Self {
        let inner = value.into_inner();
        let mut generics: Vec<String> = Vec::new();
        let mut ident: String = String::new();
        let mut properties: BTreeMap<String, StructField> = BTreeMap::new();
        let mut variants: BTreeMap<String, Variable> = BTreeMap::new();
        let mut visibility: TypeVisibility = TypeVisibility::Private;

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
                                let mut visibility: TypeVisibility = TypeVisibility::Private;

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
                Rule::enum_block => {
                    let mut inner = pair.into_inner();
                    while let Some(pair) = inner.next() {
                        let var = Variable::from(pair.into_inner().next().unwrap());
                        variants.insert(var.ident.clone(), var);
                    }
                }
                _ => unreachable!("reached impossible rule in type processing"),
            }
        }

        Self {
            generics,
            ident,
            properties,
            variants,
            visibility,
        }
    }
}

impl From<(Pair<'_, Rule>, &Registers)> for Type {
    /// Get type **and** verify its existance in the given registries.
    fn from(value: (Pair<'_, Rule>, &Registers)) -> Self {
        let reg = value.1;
        let type_ref = Self::from(value.0);

        // check registries for type since they were supplied
        let t = reg.get_type(&type_ref.ident);

        // if t != type_ref {
        //     // this type exists, but it isn't the same type description
        //     fcompiler_general_error(CompilerError::NoSuchType, type_ref.ident.clone())
        // } else {
        // check generics
        t.check_generics(type_ref.generics.clone(), reg);
        // }

        // type exists, return
        type_ref
    }
}

impl Default for Type {
    fn default() -> Self {
        Self {
            ident: String::new(),
            generics: Vec::new(),
            properties: BTreeMap::new(),
            variants: BTreeMap::new(),
            visibility: TypeVisibility::Private,
        }
    }
}

impl ToLua for Type {
    fn transform(&self) -> String {
        if !self.variants.is_empty() {
            // enum, create with variants in table
            let mut out = format!("{}{} = {{\n", self.visibility, self.ident);

            for variant in &self.variants {
                out.push_str(&format!("{} = {},\n", variant.0, variant.1.value));
            }

            return format!("{out}\n}}\n");
        }

        format!("{}{} = {{}}\n", self.visibility, self.ident)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeAlias {
    pub ident: String,
    pub r#type: Type,
    pub visibility: TypeVisibility,
}

impl ToLua for TypeAlias {
    fn transform(&self) -> String {
        format!(
            "{}{} = {}\n",
            self.visibility, self.ident, self.r#type.ident
        )
    }
}

impl From<Pair<'_, Rule>> for TypeAlias {
    fn from(value: Pair<'_, Rule>) -> Self {
        let inner = value.into_inner();

        let mut ident: String = String::new();
        let mut r#type: Type = Type::default();
        let mut visibility: TypeVisibility = TypeVisibility::Private;

        for pair in inner {
            let rule = pair.as_rule();
            match rule {
                Rule::type_modifier => visibility = pair.into(),
                Rule::identifier => ident = pair.as_str().to_string(),
                Rule::r#type => r#type = pair.into(),
                _ => unreachable!("reached impossible rule in type alias processing"),
            }
        }

        Self {
            ident,
            r#type,
            visibility,
        }
    }
}

/// The visibility of a type (<https://www.lua.org/pil/14.2.html>).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TypeVisibility {
    Public,
    Private,
}

impl From<Pair<'_, Rule>> for TypeVisibility {
    fn from(value: Pair<Rule>) -> Self {
        match value.as_str() {
            "pub" => TypeVisibility::Public,
            "prv" => TypeVisibility::Private,
            _ => unreachable!("reached impossible type modifier value"),
        }
    }
}

impl Display for TypeVisibility {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", match self {
            Self::Public => "",
            Self::Private => "local ",
        })
    }
}

/// A call to a stored function.
pub struct FunctionCall<'a> {
    /// The identifier of the function.
    pub ident: String,
    pub arguments: Vec<Pair<'a, Rule>>,
    pub lua_out: String,
}

impl FunctionCall<'_> {
    /// Get the [`Type`] of all arguments passed during a [`FunctionCall`].
    pub fn arg_types(&self, registers: &Registers) -> Vec<Type> {
        let mut types: Vec<Type> = Vec::new();

        for arg in self.arguments.clone() {
            types.push(Type::from_parser_type(arg, registers))
        }

        types
    }
}

impl<'a> From<Pair<'a, Rule>> for FunctionCall<'a> {
    fn from(value: Pair<'a, Rule>) -> Self {
        let mut lua_out: String = String::new();
        let mut inner = value.into_inner();

        let mut ident: String = String::new();
        let mut args: String = String::new();
        let mut args_vec: Vec<Pair<'_, Rule>> = Vec::new();
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
                        // ident as argument
                        args_vec.push(pair.clone());
                        if args.is_empty() {
                            // first argument
                            args.push_str(&pair.as_str().replace(",", ""))
                        } else {
                            // nth argument
                            args.push_str(&(", ".to_string() + &pair.as_str().replace(",", "")))
                        }
                    }
                }
                Rule::block => {
                    args.push_str(&crate::process(pair.into_inner(), Registers::default()).0)
                }
                _ => {
                    args_vec.push(pair.clone());
                    if args.is_empty() {
                        // first argument
                        args.push_str(pair.as_str())
                    } else {
                        // nth argument
                        args.push_str(&(", ".to_string() + &pair.as_str().replace(",", "")))
                    }
                }
            }
        }

        if is_async {
            lua_out.push_str(&format!("select(2, coroutine.resume({ident}({args})))\n"));
        } else {
            lua_out.push_str(&format!("{ident}({args})\n"));
        }

        Self {
            ident,
            lua_out,
            arguments: args_vec,
        }
    }
}

impl ToLua for FunctionCall<'_> {
    fn transform(&self) -> String {
        self.lua_out.to_owned()
    }
}

/// A standard for loop.
///
/// <https://www.lua.org/pil/4.3.5.html>
///
/// We do not support <https://www.lua.org/pil/4.3.4.html> (numeric for) at this time.
pub struct ForLoop {
    pub idents: Vec<String>,
    pub iterator: String,
    pub block: String,
}

impl From<(Pair<'_, Rule>, &Registers)> for ForLoop {
    fn from(value: (Pair<'_, Rule>, &Registers)) -> Self {
        let regs = value.1;
        let mut inner = value.0.into_inner();

        let mut idents: Vec<String> = Vec::new();
        let mut iterator: String = String::new();
        let mut block: String = String::new();

        while let Some(pair) = inner.next() {
            let rule = pair.as_rule();
            match rule {
                Rule::identifier => idents.push(pair.as_str().to_string()),
                Rule::block => {
                    block = crate::process(pair.into_inner(), {
                        let mut regs = regs.clone();

                        for identifier in &idents {
                            regs.variables.insert(
                                identifier.clone(),
                                (identifier.clone(), Type::from(TYPE_NAME_ANY)).into(),
                            );
                        }

                        regs
                    })
                    .0
                }
                _ => iterator = pair.as_str().to_string(),
            }
        }

        Self {
            idents,
            iterator,
            block,
        }
    }
}

impl ToLua for ForLoop {
    fn transform(&self) -> String {
        format!(
            "for {} in {} do\n{}\nend\n",
            {
                let mut out = String::new();

                for (i, ident) in self.idents.iter().enumerate() {
                    if i == self.idents.len() - 1 {
                        out.push_str(&ident);
                    } else {
                        out.push_str(&format!("{ident}, "));
                    }
                }

                out
            },
            self.iterator,
            self.block
        )
    }
}

/// A standard while loop.
///
/// <https://www.lua.org/pil/4.3.2.html>
pub struct WhileLoop {
    pub condition: String,
    pub block: String,
}

impl From<(Pair<'_, Rule>, &Registers)> for WhileLoop {
    fn from(value: (Pair<'_, Rule>, &Registers)) -> Self {
        let regs = value.1;
        let mut inner = value.0.into_inner();

        let mut condition: String = String::new();
        let mut block: String = String::new();

        while let Some(pair) = inner.next() {
            let rule = pair.as_rule();
            match rule {
                Rule::block => block = crate::process(pair.into_inner(), regs.clone()).0,
                _ => condition = pair.as_str().to_string(),
            }
        }

        Self { condition, block }
    }
}

impl ToLua for WhileLoop {
    fn transform(&self) -> String {
        format!("while {} do\n{}\nend\n", self.condition, self.block)
    }
}

/// A standard conditional (if, else, else if).
///
/// <https://www.lua.org/pil/4.3.1.html>
pub struct Conditional {
    pub keyword: String,
    pub condition: String,
    pub block: String,
}

impl From<(Pair<'_, Rule>, &Registers)> for Conditional {
    fn from(value: (Pair<'_, Rule>, &Registers)) -> Self {
        let regs = value.1;

        let keyword = match value.0.as_rule() {
            Rule::conditional_else => "else",
            Rule::conditional_elseif => "elseif",
            _ => "if",
        }
        .to_string();

        let mut inner = value.0.into_inner();

        let mut condition: String = String::new();
        let mut block: String = String::new();

        while let Some(pair) = inner.next() {
            let rule = pair.as_rule();
            match rule {
                Rule::block => block = crate::process(pair.into_inner(), regs.clone()).0,
                Rule::conditional_else => {
                    if block.ends_with("end\n") {
                        // reopen block
                        block = block[..block.len() - 4].to_string();
                    }

                    block.push_str(&Conditional::from((pair, regs)).transform())
                }
                Rule::conditional_elseif => {
                    if block.ends_with("end\n") {
                        block = block[..block.len() - 4].to_string();
                    }

                    block.push_str(&Conditional::from((pair, regs)).transform())
                }
                _ => condition = pair.as_str().to_string(),
            }
        }

        Self {
            keyword,
            condition,
            block,
        }
    }
}

impl ToLua for Conditional {
    fn transform(&self) -> String {
        format!(
            "\n{} {}{}\n{}\n{}",
            self.keyword,
            self.condition,
            if self.keyword == "else" { "" } else { " then" },
            self.block,
            if !self.block.ends_with("end\n") {
                "end\n"
            } else {
                ""
            }
        )
    }
}
