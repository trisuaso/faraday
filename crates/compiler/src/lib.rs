use parser::{Pairs, Rule};
mod checking;

use checking::{Function, Registers, ToLua, Type, TypeVisiblity, Variable};
pub type ParserPairs<'a> = Pairs<'a, Rule>;

/// Generate a Lua output from the given parser output
pub fn process(input: ParserPairs) -> (String, Registers) {
    let mut lua_out = String::new();
    let mut registers = Registers::default();

    for pair in input {
        match pair.as_rule() {
            Rule::function => {
                let mut inner = pair.into_inner();

                let mut name = String::new();
                let mut keys: Vec<String> = Vec::new();
                let mut types: Vec<Type> = Vec::new();
                let mut return_type: Type = Type::default();
                let mut visibility: TypeVisiblity = TypeVisiblity::Private;

                while let Some(pair) = inner.next() {
                    let rule = pair.as_rule();
                    match rule {
                        Rule::identifier => {
                            name = pair.as_str().to_string();
                        }
                        Rule::type_modifier => {
                            visibility = pair.into();
                        }
                        Rule::typed_parameter => {
                            let mut inner = pair.into_inner();
                            // it's safe to unwrap here because the grammar REQUIRES
                            // a type definition for arguments
                            types.push(inner.next().unwrap().into());
                            keys.push(inner.next().unwrap().as_str().to_string());
                        }
                        Rule::r#type => return_type = pair.into(),
                        Rule::block => {
                            // function body, end processing here
                            let function = Function {
                                name: name.clone(),
                                arguments: checking::FunctionArguments { keys, types },
                                return_type,
                                body: process(pair.into_inner()).0,
                                visibility,
                            };

                            lua_out.push_str(&function.transform());
                            registers.functions.insert(name, function);
                            break; // functions can only have ONE block
                        }
                        _ => unreachable!("reached impossible rule in function processing"),
                    }
                }
            }
            Rule::block => {
                lua_out.push_str(&process(pair.into_inner()).0);
            }
            Rule::pair => {
                let mut inner = pair.into_inner();

                let mut name = String::new();
                let mut r#type = Type::default();
                let mut visibility: TypeVisiblity = TypeVisiblity::Private;

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
                            // function body, end processing here
                            let variable = Variable {
                                name: name.clone(),
                                r#type,
                                value: match rule {
                                    // process blocks before using as value
                                    Rule::block => process(pair.into_inner()).0,
                                    // everything else just needs to be stringified
                                    _ => pair.as_str().to_string(),
                                },
                                visibility,
                            };

                            lua_out.push_str(&variable.transform());
                            registers.variables.insert(name, variable);
                            break; // functions can only have ONE block
                        }
                    }
                }
            }
            _ => lua_out.push_str(&(pair.as_str().to_string() + " ")),
        }
    }

    (lua_out, registers)
}
