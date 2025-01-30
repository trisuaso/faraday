pub mod data;
pub mod macros;
pub mod parser;

use macros::icompiler_error;
use parser::{InstructionParser, Pairs, Parser, Rule};
pub type ParserPairs<'a> = Pairs<'a, Rule>;

use data::{Function, Operation, Registers, Section, ToIr, Variable};
use pathbufd::PathBufD as PathBuf;
use std::{
    fs::read_to_string,
    sync::{LazyLock, Mutex},
};

pub static COMPILER_MARKER: LazyLock<Mutex<(String, String)>> =
    LazyLock::new(|| Mutex::new((String::default(), String::default())));

pub fn process<'a>(
    input: ParserPairs<'a>,
    file_specifier: &'a str,
    mut registers: Registers,
) -> (Registers, Vec<Operation>) {
    let mut operations = Vec::new();
    for pair in input {
        let rule = pair.as_rule();

        // marker
        let span = pair.as_span();

        let start = span.start_pos().line_col();
        let end = span.end_pos().line_col();

        let marker = format!("{}:{}:{}", file_specifier, start.0, start.1);
        let marker_end = format!("{}:{}:{}", file_specifier, end.0, end.1);

        match COMPILER_MARKER.lock() {
            Ok(mut w) => {
                *w = (
                    marker.clone().replace("./", ""),
                    marker_end.clone().replace("./", ""),
                )
            }
            Err(_) => COMPILER_MARKER.clear_poison(),
        }

        // ...
        match rule {
            Rule::section => {
                let mut inner = pair.into_inner();

                let ident = inner.next().unwrap().as_str().to_string();

                let operations_ = process(
                    inner.next().unwrap().into_inner(), // block
                    file_specifier,
                    registers.clone(),
                );

                let ops_regs = operations_.0;
                merge_registers!(ops_regs + registers);

                registers.sections.insert(ident.clone(), Section {
                    ident: ident.clone(),
                    operations: operations_.1,
                });

                operations.push(Operation::Section(ident));
            }
            Rule::function => {
                let mut inner = pair.into_inner();

                let ret_type = inner.next().unwrap().as_str().to_string();
                let ident = inner.next().unwrap().as_str().to_string();
                let mut args: Vec<(String, String)> = Vec::new();

                while let Some(pair) = inner.next() {
                    match pair.as_rule() {
                        Rule::identifier => {}
                        Rule::param => {
                            let mut inner = pair.into_inner();
                            args.push((
                                // type
                                inner.next().unwrap().as_str().to_string(),
                                // name
                                format!("%{}", inner.next().unwrap().as_str()),
                            ));
                        }
                        Rule::block => {
                            let operations_ = process(
                                pair.into_inner(), // block
                                file_specifier,
                                {
                                    let mut regs = registers.clone();

                                    for var in &args {
                                        let ident = var.1.replacen("%", "", 1);
                                        regs.variables
                                            .insert(ident.to_string(), ident.as_str().into());
                                    }

                                    regs
                                },
                            );

                            let ops_regs = operations_.0;
                            merge_registers!(ops_regs + registers);

                            registers.functions.insert(ident.clone(), Function {
                                ident: ident.clone(),
                                ret_type,
                                args,
                                operations: operations_.1,
                            });

                            operations.push(Operation::Function(ident));
                            break; // we're done here
                        }
                        _ => icompiler_error!(
                            "reached unexpected rule in function: {:?}",
                            pair.as_rule()
                        ),
                    }
                }
            }
            Rule::call => {
                let mut inner = pair.into_inner();
                let sub_function = inner.next().unwrap().as_str();
                match sub_function {
                    // jump: jump to section
                    "jump" => {
                        // get section name and process section
                        let section_name = inner.next().unwrap().as_str();
                        operations.push(Operation::Jump(section_name.to_string()));
                    }
                    // everything user-defined
                    _ => {
                        operations.push(fn_call(sub_function.to_string(), inner, &registers));
                    }
                }
            }
            Rule::pair => {
                // alloc variable
                let mut inner = pair.into_inner();

                let mut prefix: String = String::new();
                let mut ident: String = String::new();
                let mut r#type: String = String::new();
                let mut size: usize = 0;
                let mut closed_size: bool = false;
                let mut value: String = String::new();

                while let Some(pair) = inner.next() {
                    let rule = pair.as_rule();
                    match rule {
                        Rule::call => {
                            let mut inner = pair.into_inner();
                            let sub_function = inner.next().unwrap().as_str();

                            prefix = format!("%1 = __VALUE_INSTEAD\n");
                            value = fn_call(sub_function.to_string(), inner, &registers)
                                .transform(&mut registers)
                                .1;

                            if !closed_size {
                                size = value.len();
                            }

                            break;
                        }
                        Rule::type_annotation => {
                            let mut inner = pair.into_inner();
                            r#type = inner.next().unwrap().as_str().to_string();
                        }
                        Rule::identifier => {
                            if ident.is_empty() {
                                ident = pair.as_str().to_string()
                            } else if pair.as_str() != "void" {
                                let var = registers.get_var(pair.as_str());

                                value = format!("%{}", var.label);

                                if !closed_size {
                                    size = value.len();
                                }
                            } else {
                                value = pair.as_str().to_string()
                            }
                        }
                        Rule::llvm_ir => match llvm_ir(pair.into_inner()) {
                            Operation::Ir(data) => {
                                value = data;

                                if !closed_size {
                                    size = value.len();
                                }
                            }
                            _ => unreachable!(),
                        },
                        Rule::int => {
                            size = pair.as_str().parse::<usize>().unwrap();
                            closed_size = true;
                        }
                        _ => {
                            value = pair.as_str().to_string();

                            if !closed_size {
                                size = value.len();
                            }
                        }
                    }
                }

                registers.variables.insert(ident.clone(), Variable {
                    prefix,
                    label: ident.clone(),
                    size,
                    value: value.clone(),
                    r#type: r#type.clone(),
                });

                operations.push(Operation::Assign(ident.clone()));

                if (r#type != "string") & (value != "void") {
                    operations.push(Operation::Pipe((ident, value)));
                }
            }
            Rule::no_alloca_pair => {
                // alloc variable
                let mut inner = pair.into_inner();

                let mut ident: String = String::new();
                let mut value: String = String::new();

                while let Some(pair) = inner.next() {
                    let rule = pair.as_rule();
                    match rule {
                        Rule::identifier => {
                            if ident.is_empty() {
                                ident = pair.as_str().to_string()
                            } else {
                                value = pair.as_str().to_string()
                            }
                        }
                        Rule::llvm_ir => match llvm_ir(pair.into_inner()) {
                            Operation::Ir(data) => {
                                value = data;
                            }
                            _ => unreachable!(),
                        },
                        _ => value = pair.as_str().to_string(),
                    }
                }

                registers.variables.insert(ident.clone(), Variable {
                    prefix: String::new(),
                    label: ident.clone(),
                    size: 0,
                    value: value.clone(),
                    r#type: "faraday::no_alloca".to_string(),
                });

                operations.push(Operation::Assign(ident.clone()));
            }
            Rule::pipe => {
                let mut inner = pair.into_inner();

                let ident = inner.next().unwrap().as_str().to_string();
                let value = inner.next().unwrap();

                // get variable
                let v = registers.get_var_mut(ident.as_str());

                // update variable
                v.value = String::with_capacity(v.size);
                for char in value.as_str().to_string().chars() {
                    v.value.push(char);
                }

                // push operation
                operations.push(Operation::Pipe((ident.to_string(), v.value.clone())));
            }
            Rule::read => {
                let ident = pair.into_inner().next().unwrap().as_str();
                operations.push(Operation::Read(ident.to_string()));
            }
            Rule::llvm_ir => operations.push(llvm_ir(pair.into_inner())),
            Rule::r#return => operations.push(Operation::Ir(format!("ret {}", {
                let pair = pair.into_inner().next().unwrap();
                match pair.as_rule() {
                    Rule::llvm_ir => match llvm_ir(pair.into_inner()) {
                        Operation::Ir(data) => data,
                        _ => unreachable!(),
                    },
                    _ => pair.as_str().to_string(),
                }
            }))),
            Rule::EOI => break,
            _ => icompiler_error!("reached unexpected token: {rule:?}"),
        }
    }

    (registers, operations)
}

// short operation generators
fn llvm_ir<'a>(mut input: ParserPairs<'a>) -> Operation {
    let mut raw = input.next().unwrap().as_str().to_string();

    raw.remove(0);
    raw.remove(raw.len() - 1);

    Operation::Ir(raw)
}

fn fn_call<'a>(ident: String, mut inner: ParserPairs<'a>, regs: &Registers) -> Operation {
    let fun = regs.get_function(&ident);
    let mut args_string = String::new();

    let mut arg_count = 0;
    while let Some(pair) = inner.next() {
        args_string.push_str(&match pair.as_rule() {
            Rule::COMMA => ",".to_string(),
            Rule::call_param => {
                let mut inner = pair.into_inner();
                let mut value: String = String::new();
                let mut r#type: String = String::new();

                while let Some(pair) = inner.next() {
                    match pair.as_rule() {
                        Rule::identifier => {
                            if value.is_empty() {
                                // fill value first
                                // get var and make sure it isn't a string
                                let var = regs.get_var(pair.as_str());

                                if var.r#type == "string" {
                                    // return pointer to string
                                    value = format!("@.s_{}", var.label);
                                    r#type = "ptr".to_string();
                                } else {
                                    // normal variable
                                    value = format!("%{}", var.label)
                                }
                            } else {
                                // fill type
                                r#type = pair.as_str().to_string();
                            }
                        }
                        _ => value = pair.as_str().to_string(),
                    }
                }

                if r#type.is_empty() {
                    // pull from function
                    format!("{} {}", fun.args.get(arg_count).unwrap().0, value)
                } else {
                    // type was provided
                    format!("{type} {value}")
                }
            }
            Rule::int => pair.as_str().to_string(),
            Rule::string => {
                icompiler_error!("cannot process string in call arguments, please use pointer")
            }
            Rule::llvm_ir => match llvm_ir(pair.into_inner()) {
                Operation::Ir(data) => data,
                _ => unreachable!(),
            },
            _ => icompiler_error!(
                "received unexpected rule in function arguments: {:?}",
                pair.as_rule()
            ),
        });

        arg_count += 1;
    }

    Operation::Call((ident, args_string))
}

// ...
pub fn process_file<'a>(path: PathBuf) -> String {
    let file_string = match read_to_string(&path) {
        Ok(f) => f,
        Err(e) => icompiler_error!("{e}"),
    };

    let parsed = match InstructionParser::parse(parser::Rule::document, &file_string) {
        Ok(mut p) => p.next().unwrap().into_inner(),
        Err(e) => icompiler_error!("{e}"),
    };

    let mut head: String = String::new();
    let mut body: String = String::new();

    let mut operations = process(
        parsed,
        path.as_path().to_str().unwrap(),
        Registers::default(),
    );

    for operation in operations.1 {
        let (head_, body_) = operation.transform(&mut operations.0);
        head.push_str(&format!("{head_}\n"));
        body.push_str(&format!("{body_}\n"));
    }

    format!(
        "; faraday rir
declare i32 @puts(i8* nocapture) nounwind
declare i32 @printf(i8* nocapture) nounwind
{}\n{body}",
        head.trim()
    )
}
