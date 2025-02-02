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

use rand::{Rng, distributions::Alphanumeric, thread_rng};
pub fn random() -> String {
    thread_rng()
        .sample_iter(&Alphanumeric)
        .take(16)
        .map(char::from)
        .collect()
}

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
            Rule::include => {
                let path_string = pair.into_inner().next().unwrap().as_str();
                let path: PathBuf = {
                    let inner = path_string.replace("\"", "");
                    PathBuf::new()
                        .join(registers.get_var("@@PATH_PARENT").value)
                        .join(inner)
                };

                let compiled = process_file(path);

                let compiled_regs = compiled.0;
                merge_registers!(compiled_regs + registers);

                operations.push(Operation::Ir(compiled.1));
            }
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
                    // decay: create C array decay from variable
                    // variable should be `alloca [100 * i8]`
                    "decay" => {
                        let ident = inner.next().unwrap().as_str();

                        registers
                            .variables
                            .insert(format!("{ident}.decay"), Variable {
                                prefix: String::new(),
                                label: format!("{ident}.decay"),
                                size: 100,
                                align: 16,
                                value: String::new(),
                                r#type: "ptr".to_string(),
                                key: random(),
                            });

                        operations.push(Operation::Ir(format!("%{ident}.decay = getelementptr inbounds [100 x i8], ptr %{ident}.addr, i64 0, i64 0")));
                    }
                    // awrite: write to an array variable
                    //
                    // # Example
                    // ```text
                    // awrite(ident, value, idx...)
                    // ```
                    "awrite" => {
                        // get variable
                        let ident = inner.next().unwrap().as_str().to_string();

                        let var = registers.get_var(&ident);
                        let r#type = var.r#type;

                        // get value
                        inner.next();
                        let value = inner.next().unwrap();
                        let value = match value.as_rule() {
                            _ => value.as_str().to_string(),
                        };

                        // build index pointers
                        let mut index_access_ir = String::new();

                        let mut indexes_suffix_string: String = String::new();
                        let mut last_index_variable: String = String::new();

                        while let Some(pair) = inner.next() {
                            if pair.as_rule() != Rule::call_param {
                                continue;
                            }

                            let idx = pair.as_str();
                            indexes_suffix_string.push_str(&format!(".{idx}")); // this keeps the variable naming predictable
                            last_index_variable = random();

                            index_access_ir.push_str(&format!("%arridx_{last_index_variable} = getelementptr inbounds [{idx} x {type}], ptr %{}.addr, i64 0, i64 {idx}", var.label));
                        }

                        // ...
                        operations.push(Operation::Ir(format!(
                            "{index_access_ir}\nstore {type} {value}, ptr %arridx_{last_index_variable}, align 8"
                        )));
                    }
                    // aread: read from an array
                    //
                    // # Example
                    // ```test
                    // aread(ident, idx...)
                    // ```
                    //
                    // # Returns
                    // Defines `ident.index.index...` variable.
                    "aread" => {
                        // C does array access by generating variables which
                        // use getelementptr inbounds to access fields to read and write
                        let var_ident = inner.next().unwrap().as_str();
                        let var = registers.get_var(var_ident);

                        // build index pointers
                        let mut index_access_ir = String::new();

                        let mut indexes_suffix_string: String = String::new();
                        let mut last_index_variable: String = String::new();

                        while let Some(pair) = inner.next() {
                            if pair.as_rule() != Rule::call_param {
                                continue;
                            }

                            let idx = pair.as_str();
                            indexes_suffix_string.push_str(&format!(".{idx}")); // this keeps the variable naming predictable
                            last_index_variable = random();

                            index_access_ir.push_str(&format!("%arridx_{last_index_variable} = getelementptr inbounds [{idx} x {}], ptr %{var_ident}.addr, i64 0, i64 {idx}", var.r#type));
                        }

                        // ...
                        let name = format!("{}{indexes_suffix_string}", var.label);
                        operations.push(Operation::Ir(format!(
                            "{index_access_ir}\n%{name} = load {}, ptr %arridx_{last_index_variable}, align 8",
                             var.r#type
                        )));
                        registers
                            .variables
                            .insert(name.clone(), name.as_str().into());
                    }
                    // everything user-defined
                    _ => {
                        let fun = registers.get_function(sub_function).clone();
                        operations.push(fn_call(
                            sub_function.to_string(),
                            inner,
                            &mut registers,
                            &fun,
                        ));
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
                let mut align: i32 = 4;
                let mut closed_size: bool = false;
                let mut value: String = String::new();
                let key: String = random();

                while let Some(pair) = inner.next() {
                    let rule = pair.as_rule();
                    match rule {
                        Rule::call => {
                            let mut inner = pair.into_inner();
                            let sub_function = inner.next().unwrap().as_str();

                            prefix = format!("%k_{key} = __VALUE_INSTEAD\n");
                            let fun = registers.get_function(sub_function).clone();
                            value = fn_call(sub_function.to_string(), inner, &mut registers, &fun)
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
                        Rule::pair_alignment => {
                            let mut inner = pair.into_inner();
                            align = inner.next().unwrap().as_str().parse::<i32>().unwrap();
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
                                size = std::mem::size_of_val(value.as_bytes());
                            }
                        }
                    }
                }

                registers.variables.insert(ident.clone(), Variable {
                    prefix: if prefix == "_drop" {
                        String::new()
                    } else {
                        prefix.clone()
                    },
                    label: ident.clone(),
                    size,
                    align,
                    value: value.clone(),
                    r#type: r#type.clone(),
                    key,
                });

                if prefix != "_drop" {
                    operations.push(Operation::Assign(ident.clone()));
                }

                if (r#type != "string") & (value != "void") && (prefix != "_drop") {
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
                    align: 4,
                    value: value.clone(),
                    r#type: "faraday::no_alloca".to_string(),
                    key: random(),
                });

                registers
                    .variables
                    .insert(format!("{ident}.addr"), ident.as_str().into());

                operations.push(Operation::Assign(ident.clone()));
            }
            Rule::pipe => {
                let mut inner = pair.into_inner();

                let ident = inner.next().unwrap().as_str().to_string();
                let value = inner.next().unwrap().as_str().to_string();

                // push operation
                operations.push(Operation::Pipe((ident.to_string(), value.to_string())));
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
                    Rule::call_param => {
                        let mut inner = pair.into_inner();

                        let value = inner.next().unwrap();
                        let value = match value.as_rule() {
                            Rule::identifier => {
                                format!("%{}", value.as_str().to_string())
                            }
                            _ => value.as_str().to_string(),
                        };

                        let r#type = inner.next().unwrap().as_str();
                        format!("{type} {value}")
                    }
                    _ => pair.as_str().to_string(),
                }
            }))),
            Rule::EOI => break,
            _ => icompiler_error!("reached unexpected token: {rule:?}"),
        }
    }

    if !registers.extra_header_ir.is_empty() {
        operations.push(Operation::HeadIr(registers.extra_header_ir.clone()));
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

fn fn_call<'a>(
    ident: String,
    mut inner: ParserPairs<'a>,
    regs: &mut Registers,
    fun: &Function,
) -> Operation {
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
                                    value = format!("@.s_{}_{}", var.label, var.key);
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
                        Rule::llvm_ir => {
                            value = match llvm_ir(pair.into_inner()) {
                                Operation::Ir(data) => {
                                    r#type = "void".to_string();
                                    data
                                }
                                _ => unreachable!(),
                            }
                        }
                        Rule::sized_string => {
                            // icompiler_error!("cannot process string in call arguments, please use pointer")

                            let mut inner = pair.into_inner();

                            let content = inner.next().unwrap().as_str();
                            let size = inner.next().unwrap().into_inner().next().unwrap().as_str();

                            let name = random();
                            regs.extra_header_ir.push_str(&format!(
                                "@.s_{name} = constant [{size} x i8] c\"{content}\\00\\00\", align 1\n",
                            ));
                            value = format!("@.s_{name}");
                        }
                        _ => value = pair.as_str().to_string(),
                    }
                }

                if r#type.is_empty() {
                    // pull from function
                    format!("{} {}", fun.args.get(arg_count).unwrap().0, value)
                } else {
                    // type was provided
                    if r#type != "void" {
                        format!("{type} {value}")
                    } else {
                        value
                    }
                }
            }
            Rule::int => pair.as_str().to_string(),
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

macro_rules! define {
    ($name:literal = ($value:expr) >> $registers:ident) => {
        $registers.variables.insert($name.to_string(), Variable {
            prefix: String::new(),
            label: $name.to_string(),
            r#type: "void".to_string(),
            value: $value.to_string(),
            size: 0,
            align: 0,
            key: random(),
        });
    };
}

// ...
pub fn process_file(path: PathBuf) -> (Registers, String) {
    let mut registers: Registers = Registers::default();

    // define some compiler variables
    define!("@@PATH_PARENT" = (path.as_path().parent().unwrap().to_str().unwrap()) >> registers);

    // ...
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

    let file_specifier = path.as_path().to_str().unwrap();
    let mut operations = process(parsed, file_specifier, registers);

    for operation in operations.1 {
        let (head_, body_) = operation.transform(&mut operations.0);
        head.push_str(&format!("{head_}\n"));
        body.push_str(&format!("{body_}\n"));
    }

    (
        operations.0,
        format!(
            "; begin: {file_specifier}\n{}\n{body}; end: {file_specifier}",
            head.trim()
        ),
    )
}

pub fn process_file_with_bindings(path: PathBuf) -> (Registers, String) {
    let out = process_file(path);
    (
        out.0,
        format!(
            "; faraday rir
declare i32 @puts(i8* nocapture) nounwind
declare i32 @printf(i8* nocapture) nounwind

declare i32 @strcat(i8* nocapture, i8* nocapture) nounwind
declare i32 @strcpy(i8* nocapture, i8* nocapture) nounwind

declare ptr @malloc(i32) nounwind
declare void @free(i8* nocapture) nounwind
{}",
            out.1
        ),
    )
}
