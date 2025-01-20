use parser::{Pairs, Rule};
pub mod checking;

use checking::{
    Conditional, ForLoop, Function, FunctionCall, Registers, ToLua, Type, TypeVisiblity, Variable,
    WhileLoop,
};
pub type ParserPairs<'a> = Pairs<'a, Rule>;

/// Generate a Lua output from the given parser output
pub fn process(input: ParserPairs, mut registers: Registers) -> (String, Registers) {
    let mut lua_out = String::new();

    for pair in input {
        match pair.as_rule() {
            Rule::function => {
                let function: Function = pair.into();
                lua_out.push_str(&function.transform());
                registers.functions.insert(function.name.clone(), function);
            }
            Rule::block => {
                lua_out.push_str(&process(pair.into_inner(), Registers::default()).0);
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
                                    Rule::block => {
                                        process(pair.into_inner(), Registers::default()).0
                                    }
                                    // everything else just needs to be stringified
                                    Rule::call => FunctionCall { pair }.transform(),
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
            Rule::call => {
                lua_out.push_str(&FunctionCall { pair }.transform());
            }
            Rule::r#struct => {
                let t = Type::from(pair);
                lua_out.push_str(&t.transform());
                registers.types.insert(t.ident.clone(), t);
            }
            Rule::for_loop => lua_out.push_str(&ForLoop { pair }.transform()),
            Rule::while_loop => lua_out.push_str(&WhileLoop { pair }.transform()),
            Rule::conditional => lua_out.push_str(&Conditional { pair }.transform()),
            _ => lua_out.push_str(&(pair.as_str().to_string() + "\n")),
        }
    }

    (lua_out, registers)
}
