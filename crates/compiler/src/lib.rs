use parser::{Pairs, Rule};

pub mod bindings;
pub mod checking;
pub mod data;

use checking::{MultipleTypeChecking, Registers, ToLua};
use data::{
    Conditional, ForLoop, Function, FunctionCall, Impl, Type, TypeAlias, Variable, WhileLoop,
};

pub type ParserPairs<'a> = Pairs<'a, Rule>;

/// Generate a Lua output from the given parser output
pub fn process(input: ParserPairs, mut registers: Registers) -> (String, Registers) {
    let mut lua_out = String::new();

    for pair in input {
        match pair.as_rule() {
            Rule::function => {
                let function: Function = (pair, &registers).into();
                lua_out.push_str(&function.transform());
                registers.functions.insert(function.ident.clone(), function);
            }
            Rule::block => {
                lua_out.push_str(&process(pair.into_inner(), Registers::default()).0);
            }
            Rule::pair => {
                let variable: Variable = (pair, &registers).into();
                lua_out.push_str(&variable.transform());
                registers.variables.insert(variable.ident.clone(), variable);
            }
            Rule::call => {
                let call = FunctionCall::from(pair);
                let supplied_types = call.arg_types(&registers);
                call.check_multiple(supplied_types, &registers);
                lua_out.push_str(&call.transform());
            }
            Rule::r#struct => {
                let t = Type::from(pair);
                lua_out.push_str(&t.transform());
                registers.types.insert(t.ident.clone(), t.clone());
                registers
                    .variables
                    .insert(t.ident.clone(), (t.ident.clone(), t).into());
            }
            Rule::r#enum => {
                let t = Type::from(pair);
                lua_out.push_str(&t.transform());
                registers.types.insert(t.ident.clone(), t.clone());
                registers
                    .variables
                    .insert(t.ident.clone(), (t.ident.clone(), t).into());
            }
            Rule::type_alias => {
                let t = TypeAlias::from(pair);
                lua_out.push_str(&t.transform());
                let mut ty = registers.get_type(&t.r#type.ident);
                ty.generics = t.r#type.generics;
                registers.types.insert(t.ident.ident.clone(), ty.clone());
                registers
                    .variables
                    .insert(t.ident.ident.clone(), (t.ident.ident.clone(), ty).into());
            }
            Rule::for_loop => lua_out.push_str(&ForLoop::from((pair, &registers)).transform()),
            Rule::while_loop => lua_out.push_str(&WhileLoop::from((pair, &registers)).transform()),
            Rule::conditional => {
                lua_out.push_str(&Conditional::from((pair, &registers)).transform())
            }
            Rule::r#impl => {
                let i = Impl::from((pair, &registers));

                for function in &i.functions {
                    // make sure all functions get registered
                    registers
                        .functions
                        .insert(function.ident.clone(), function.clone());
                }

                lua_out.push_str(&i.transform());
            }
            _ => lua_out.push_str(&(pair.as_str().to_string() + "\n")),
        }
    }

    (lua_out, registers)
}
