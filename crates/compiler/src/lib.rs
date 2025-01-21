use parser::{Pairs, Rule};

pub mod checking;
pub mod data;

use checking::{Registers, ToLua};
use data::{Conditional, ForLoop, Function, FunctionCall, Type, Variable, WhileLoop};

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
                let variable: Variable = pair.into();
                lua_out.push_str(&variable.transform());
                registers.variables.insert(variable.ident.clone(), variable);
            }
            Rule::call => {
                lua_out.push_str(&FunctionCall::from(pair).transform());
            }
            Rule::r#struct => {
                let t = Type::from(pair);
                lua_out.push_str(&t.transform());
                registers.types.insert(t.ident.clone(), t);
            }
            Rule::for_loop => lua_out.push_str(&ForLoop::from(pair).transform()),
            Rule::while_loop => lua_out.push_str(&WhileLoop::from(pair).transform()),
            Rule::conditional => lua_out.push_str(&Conditional::from(pair).transform()),
            _ => lua_out.push_str(&(pair.as_str().to_string() + "\n")),
        }
    }

    (lua_out, registers)
}
