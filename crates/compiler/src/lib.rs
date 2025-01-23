use bindings::{TYPE_NAME_ANY, TYPE_NAME_TABLE};
use parser::{FaradayParser, Pairs, Parser, Rule};
use pathbufd::PathBufD as PathBuf;
use std::fs::{read_to_string, write};

pub mod bindings;
pub mod checking;
pub mod data;

use checking::{MultipleTypeChecking, Registers, ToLua, fcompiler_general_marker};
use data::{
    Conditional, ForLoop, Function, FunctionCall, Impl, Type, TypeAlias, TypeVisibility, Variable,
    WhileLoop,
};

pub type ParserPairs<'a> = Pairs<'a, Rule>;

macro_rules! merge_register {
    ($prefix:ident; $registers:ident.$sub:ident + $other_registers:ident.$other_sub:ident) => {
        let reg = &mut $registers.$sub;
        let other_reg = $other_registers.$other_sub;

        for item in other_reg {
            reg.insert(format!("{}.{}", $prefix, item.0), item.1);
        }
    };
}

/// Generate a Lua output from the given parser output
pub fn process(input: ParserPairs, mut registers: Registers) -> (String, Registers) {
    fcompiler_marker!("{}", registers.get_var("@@FARADAY_PATH").value);
    let do_compile = registers.get_var("@@FARADAY_NO_COMPILE").value == "false";

    let mut lua_out = String::new();

    for pair in input {
        let rule = pair.as_rule();

        let span = pair.as_span();
        fcompiler_general_marker(rule, span.start_pos().line_col(), span.end_pos().line_col());

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
                            TypeVisibility::Private,
                        )
                            .into(),
                    )
                        .into(),
                );

                // process file and merge registers
                let compiled = process_file(path.clone(), Registers::default(), !do_compile);
                let compiled_regs = compiled.1;

                merge_register!(ident; registers.types + compiled_regs.types);
                merge_register!(ident; registers.functions + compiled_regs.functions);
                merge_register!(ident; registers.variables + compiled_regs.variables);

                let output_path = PathBuf::current()
                    .join("build")
                    .join(format!("{}.lua", relative_file_path));

                let parent = output_path.as_path().parent().unwrap();

                if !parent.exists() {
                    // make sure the file's parent exists
                    std::fs::create_dir_all(parent).unwrap();
                }

                if let Err(e) = write(output_path, compiled.0) {
                    fcompiler_error!("{e}")
                }
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
        });
    };

    ($name:literal = $value:ident >> $registers:ident) => {
        $registers.variables.insert($name.to_string(), Variable {
            ident: $name.to_string(),
            r#type: TYPE_NAME_ANY.into(),
            value: $value.to_string(),
            visibility: $crate::data::TypeVisibility::Private,
        });
    };

    ($name:literal = ($value:expr) >> $registers:ident) => {
        $registers.variables.insert($name.to_string(), Variable {
            ident: $name.to_string(),
            r#type: TYPE_NAME_ANY.into(),
            value: $value.to_string(),
            visibility: $crate::data::TypeVisibility::Private,
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
