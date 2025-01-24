use bindings::{TYPE_NAME_ANY, TYPE_NAME_TABLE};
use parser::{FaradayParser, Pairs, Parser, Rule};
use pathbufd::PathBufD as PathBuf;
use std::{
    fs::read_to_string,
    sync::{LazyLock, Mutex},
};

pub mod bindings;
pub mod checking;
pub mod data;

use checking::{
    CompilerError, MultipleTypeChecking, Registers, ToLua, fcompiler_general_error,
    fcompiler_general_marker, fcompiler_type_error,
};
use data::{
    Conditional, ConstantModifier, ExprCall, ExprUse, ForLoop, Function, FunctionCall, Impl, Type,
    TypeAlias, TypeVisibility, Variable, WhileLoop, use_file,
};

pub type ParserPairs<'a> = Pairs<'a, Rule>;

pub static COMPILER_MARKER: LazyLock<Mutex<String>> =
    LazyLock::new(|| Mutex::new(String::default()));

/// Generate a Lua output from the given parser output
pub fn process(input: ParserPairs, mut registers: Registers) -> (String, Registers) {
    fcompiler_marker!("{}", registers.get_var("@@FARADAY_PATH").value);
    let do_compile = registers.get_var("@@FARADAY_NO_COMPILE").value == "false";

    let mut lua_out = String::new();

    for pair in input {
        let rule = pair.as_rule();

        // marker
        let span = pair.as_span();
        let start = span.start_pos().line_col();
        let marker = format!(
            "{}:{}:{}",
            registers.get_var("@@FARADAY_PATH").value,
            start.0,
            start.1
        );

        match COMPILER_MARKER.lock() {
            Ok(mut w) => *w = marker.clone().replace("./", ""),
            Err(_) => COMPILER_MARKER.clear_poison(),
        }

        fcompiler_general_marker(rule, span.start_pos().line_col(), span.end_pos().line_col());

        // ...
        match rule {
            Rule::function => {
                let function: Function = (pair, &registers).into();

                if do_compile {
                    lua_out.push_str(&function.transform());
                }

                registers.functions.insert(function.ident.clone(), function);
            }
            Rule::block => {
                lua_out.push_str(&process(pair.into_inner(), Registers::default()).0);
            }
            Rule::pair => {
                let variable: Variable = (pair, &registers).into();

                if do_compile {
                    lua_out.push_str(&variable.transform());
                }

                registers.variables.insert(variable.ident.clone(), variable);
            }
            Rule::reassignment => {
                let mut variable: Variable = pair.clone().into();
                variable.visibility = TypeVisibility::Public; // must be public or reassignment isn't valid in lua

                if let Some(var) = registers.variables.get(&variable.ident) {
                    // check const
                    if var.constant == ConstantModifier::Constant {
                        fcompiler_general_error(
                            CompilerError::CannotAssignConst,
                            var.ident.clone(),
                        );
                    }

                    // check type
                    if (variable.r#type != var.r#type) && !variable.r#type.ident.is_empty() {
                        fcompiler_type_error(var.r#type.ident.clone(), variable.r#type.ident);
                    }
                }

                if do_compile && !variable.r#type.ident.is_empty() {
                    lua_out.push_str(&variable.transform());
                } else if variable.r#type.ident.is_empty() {
                    lua_out.push_str(pair.as_str());
                }
            }
            Rule::call => {
                let call = FunctionCall::from(pair);
                let supplied_types = call.arg_types(&registers);
                call.check_multiple(supplied_types, &registers);

                if do_compile {
                    lua_out.push_str(&call.transform());
                }
            }
            Rule::r#struct => {
                let t = Type::from(pair);

                if do_compile {
                    lua_out.push_str(&t.transform());
                }

                registers.types.insert(t.ident.clone(), t.clone());
                registers
                    .variables
                    .insert(t.ident.clone(), (t.ident.clone(), t).into());
            }
            Rule::r#enum => {
                let t = Type::from(pair);

                if do_compile {
                    lua_out.push_str(&t.transform());
                }

                registers.types.insert(t.ident.clone(), t.clone());
                registers
                    .variables
                    .insert(t.ident.clone(), (t.ident.clone(), t).into());
            }
            Rule::type_alias => {
                let t = TypeAlias::from(pair);

                if do_compile {
                    lua_out.push_str(&t.transform());
                }

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
            Rule::r#use => {
                let mut inner = pair.into_inner();

                let mut path: PathBuf = PathBuf::new();
                let mut relative_file_path: String = String::new();
                let mut ident: String = String::new();
                let mut reexport: bool = false;

                while let Some(pair) = inner.next() {
                    let rule = pair.as_rule();
                    match rule {
                        Rule::string => {
                            path = {
                                let mut inner = pair.as_str().replace("\"", "");
                                relative_file_path = inner.clone(); // before the .fd!
                                inner += ".fd";

                                PathBuf::new()
                                    .join(registers.get_var("@@FARADAY_PATH_PARENT").value)
                                    .join(inner)
                            }
                        }
                        Rule::identifier => ident = pair.as_str().to_string(),
                        Rule::type_modifier => {
                            reexport = TypeVisibility::from(pair) == TypeVisibility::Public
                        }
                        _ => unreachable!("reached impossible rule type in use processing"),
                    }
                }

                if do_compile {
                    lua_out.push_str(&format!(
                        "local {ident} = require \"{relative_file_path}\"\n"
                    ));
                }

                // register module
                registers.variables.insert(
                    ident.clone(),
                    (
                        ident.clone(),
                        (
                            TYPE_NAME_TABLE,
                            vec!["any".to_string(), "any".to_string()],
                            if reexport {
                                TypeVisibility::Public
                            } else {
                                TypeVisibility::Private
                            },
                        )
                            .into(),
                        if reexport {
                            TypeVisibility::Public
                        } else {
                            TypeVisibility::Private
                        },
                    )
                        .into(),
                );

                // process file and merge registers
                use_file(path, relative_file_path, ident, do_compile, &mut registers);
            }
            Rule::r#macro => {
                let call = FunctionCall::from(pair.into_inner().next().unwrap());

                match call.ident.as_str() {
                    "expr_use" => {
                        let _ = ExprUse::from((call, &registers));
                    }
                    "expr_call" => lua_out.push_str(&ExprCall::from(call).transform()),
                    _ => fcompiler_general_error(CompilerError::NoSuchFunction, call.ident),
                };
            }
            _ => {
                if do_compile {
                    lua_out.push_str(&(pair.as_str().to_string() + "\n"))
                }
            }
        }
    }

    (lua_out, registers)
}

macro_rules! publish_register {
    ($registers:ident.$sub:ident >> $lua_out:ident) => {
        let reg_name_for_label = stringify!($sub);
        let reg = &$registers.$sub;
        $lua_out.push_str(&format!("    -- faraday.registers:{reg_name_for_label}\n"));

        for (ident, item) in reg {
            if (item.visibility != $crate::data::TypeVisibility::Public)
                | ident.contains(".")
                | ident.contains(":")
                | ident.contains("[")
            {
                continue;
            }

            $lua_out.push_str(&format!("    {} = {},\n", ident, ident));
        }
    };
}

macro_rules! define {
    ($name:literal = $value:literal >> $registers:ident) => {
        $registers.variables.insert($name.to_string(), Variable {
            ident: $name.to_string(),
            r#type: TYPE_NAME_ANY.into(),
            value: $value.to_string(),
            visibility: $crate::data::TypeVisibility::Private,
            constant: $crate::data::ConstantModifier::Constant,
        });
    };

    ($name:literal = $value:ident >> $registers:ident) => {
        $registers.variables.insert($name.to_string(), Variable {
            ident: $name.to_string(),
            r#type: TYPE_NAME_ANY.into(),
            value: $value.to_string(),
            visibility: $crate::data::TypeVisibility::Private,
            constant: $crate::data::ConstantModifier::Constant,
        });
    };

    ($name:literal = ($value:expr) >> $registers:ident) => {
        $registers.variables.insert($name.to_string(), Variable {
            ident: $name.to_string(),
            r#type: TYPE_NAME_ANY.into(),
            value: $value.to_string(),
            visibility: $crate::data::TypeVisibility::Private,
            constant: $crate::data::ConstantModifier::Constant,
        });
    };
}

/// Process an individual file given its `path`.
pub fn process_file(
    path: PathBuf,
    mut registers: Registers,
    check_only: bool,
) -> (String, Registers) {
    // define some compiler variables
    define!(
        "@@FARADAY_PATH_PARENT" = (path.as_path().parent().unwrap().to_str().unwrap()) >> registers
    );

    define!("@@FARADAY_PATH" = (path.as_path().to_str().unwrap()) >> registers);
    define!("@@FARADAY_NO_COMPILE" = check_only >> registers);

    // ...
    let mut lua_out: String = String::new();

    let file_string = match read_to_string(path) {
        Ok(f) => f,
        Err(e) => fcompiler_error!("{e}"),
    };

    let parsed = match FaradayParser::parse(parser::Rule::document, &file_string) {
        Ok(mut p) => p.next().unwrap().into_inner(),
        Err(e) => fcompiler_error!("{e}"),
    };

    let compiled = process(parsed, registers);
    lua_out.push_str(&compiled.0);
    registers = compiled.1;

    // build export list
    let mut export = format!("\n-- faraday.module\nreturn {{\n");

    publish_register!(registers.types >> export);
    publish_register!(registers.functions >> export);
    publish_register!(registers.variables >> export);

    export.push_str("}");
    lua_out.push_str(&export);

    // return
    (lua_out, registers)
}
