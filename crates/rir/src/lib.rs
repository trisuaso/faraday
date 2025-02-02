pub mod data;
pub mod ir;
pub mod macros;
pub mod parser;

use ir::{Value, fn_call, fn_return, llvm_ir, rule_to_operator, var_assign, var_assign_no_alloca};
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
                    // peak: read the value of a variable into a temporary variable
                    "peak" => {
                        let var_ident = inner.next().unwrap().as_str();

                        inner.next(); // skip
                        let bind_as_name = inner.next().unwrap().as_str();

                        let var = registers.get_var(var_ident);

                        operations.push(Operation::Ir(format!(
                            "%{bind_as_name} = load {}, ptr %{}.addr, align 4",
                            var.r#type, var.label
                        )));

                        registers
                            .variables
                            .insert(bind_as_name.to_string(), bind_as_name.into());
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
                // we're going to implement this basically the same way Clang does,
                // we'll assign a variable with a default value, jump to a conditional
                // block, and then jump to either a body or ending block. If we jumped
                // to the body block, we need to jump to the increase block at the end to
                // progress the iteration. The ending block contains everything
                // that comes AFTER the loop.
                let mut loop_inner = pair.into_inner();
                // block names
                let key = random();
                let block_cond = format!("bb_cond_{key}");
                let block_body = format!("bb_body_{key}");
                let block_inc = format!("bb_inc_{key}");
                let block_end = format!("bb_end_{key}");

                // head
                let pair = loop_inner.next().unwrap();
                let mut scoped_regs = registers.clone(); // create new scope

                let var_name = format!("k_{key}");
                let var_label = var_assign(
                    var_name.clone(),
                    pair.into_inner(),
                    &mut operations,
                    &mut scoped_regs,
                );
                let var = scoped_regs.get_var(&var_label);
                operations.push(Operation::Ir(format!("br label %{block_cond}")));

                // cond
                let cond_key = random();

                let mut comparison = loop_inner.next().unwrap().into_inner();
                comparison.next(); // skip since this is just var_name

                let op = rule_to_operator(comparison.next().unwrap().as_rule());
                let value = Value::get(comparison.next().unwrap(), &cond_key, &mut scoped_regs).0;
                let prefix = value.1;
                let value = value.0;

                operations.push(Operation::Ir(format!(
                    "{block_cond}:
%{var_name}_{cond_key} = load {}, ptr %{var_name}.addr, align 4
{prefix}
%{var_name}_cmp_{cond_key} = icmp {op} {} %{var_name}_{cond_key}, {value}
br i1 %{var_name}_cmp_{cond_key}, label %{block_body}, label %{block_end}",
                    var.r#type, var.r#type
                )));

                // body
                let block = loop_inner.next().unwrap().into_inner();
                operations.push(Operation::Ir(format!("{block_body}:")));
                let res = process(block, file_specifier, scoped_regs);

                for operation in res.1 {
                    operations.push(operation);
                }

                let scoped_regs = res.0; // use updated version of scoped_regs
                registers
                    .extra_header_ir
                    .push_str(&scoped_regs.extra_header_ir); // make sure header stuff is still global

                operations.push(Operation::Ir(format!("br label %{block_inc}")));

                // inc(rease)
                let inc_key = random();
                operations.push(Operation::Ir(format!(
                    "{block_inc}:
%{var_name}_{inc_key} = load {}, ptr %{var_name}.addr, align 4
%{var_name}_inc_{inc_key} = add nsw i32 %{var_name}_{inc_key}, 1
store i32 %{var_name}_inc_{inc_key}, ptr %{var_name}.addr, align 4
br label %{block_cond}",
                    var.r#type
                )));

                // end
                operations.push(Operation::Ir(format!("{block_end}:")));
                let res = process(input, file_specifier, scoped_regs); // capture everything left in `input`

                for operation in res.1 {
                    operations.push(operation);
                }

                return (res.0, operations);
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
