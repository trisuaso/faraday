pub mod data;
pub mod ir;
pub mod macros;
pub mod parser;

use ir::{fn_return, for_loop, llvm_ir, root_function_call, var_assign, var_assign_no_alloca};
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
    mut input: ParserPairs<'a>,
    file_specifier: &'a str,
    mut registers: Registers,
) -> (Registers, Vec<Operation>) {
    let mut operations = Vec::new();
    while let Some(pair) = input.next() {
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
            Rule::call => root_function_call(pair, &mut operations, &mut registers),
            Rule::pair => {
                var_assign(
                    String::new(),
                    pair.into_inner(),
                    &mut operations,
                    &mut registers,
                );
            }
            Rule::no_alloca_pair => {
                var_assign_no_alloca(pair.into_inner(), &mut operations, &mut registers)
            }
            Rule::pipe => {
                let mut inner = pair.into_inner();

                let ident = inner.next().unwrap().as_str().to_string();
                let value = inner.next().unwrap().as_str().to_string();

                // push operation
                operations.push(Operation::Pipe((
                    ident.to_string(),
                    ident.to_string(),
                    value.to_string(),
                )));
            }
            Rule::read => {
                let ident = pair.into_inner().next().unwrap().as_str();
                operations.push(Operation::Read(ident.to_string()));
            }
            Rule::llvm_ir => operations.push(llvm_ir(pair.into_inner())),
            Rule::r#return => operations.push(Operation::Ir(format!("ret {}", fn_return(pair)))),
            Rule::for_loop => {
                return for_loop(input, pair, file_specifier, operations, &mut registers);
            }
            Rule::EOI => break,
            _ => icompiler_error!("reached unexpected token: {rule:?}"),
        }
    }

    if !registers.extra_header_ir.is_empty() {
        operations.push(Operation::HeadIr(registers.extra_header_ir.clone()));
    }

    (registers, operations)
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
