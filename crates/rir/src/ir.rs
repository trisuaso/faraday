//! IR helper functions to translate the parsed content to LLVM IR.
//!
//! See `data.rs` for the actual IR generation.
use crate::{
    ParserPairs, ToIr,
    data::{Function, Operation, Registers, Variable},
    icompiler_error,
    parser::{Pair, Rule},
    random,
};

/// Get a LLVM IR type from the given [`Rule`].
pub fn rule_to_type<'a>(rule: Rule) -> &'a str {
    match rule {
        Rule::integer => "i32",
        _ => "void",
    }
}

/// Get a LLVM IR operator for [`icmp`](https://llvm.org/docs/LangRef.html#icmp-instruction) from the given [`Rule`].
pub fn rule_to_operator<'a>(rule: Rule) -> &'a str {
    match rule {
        Rule::GREATER_THAN => "sgt",
        Rule::LESS_THAN => "slt",
        Rule::GREATER_THAN_EQUAL_TO => "sge",
        Rule::LESS_THAN_EQUAL_TO => "sle",
        Rule::NOT_EQUAL => "ne",
        Rule::EQUAL => "eq",
        Rule::OR => "or",
        Rule::AND => "and",
        _ => "void",
    }
}

/// [`Operation`] generation for raw LLVM IR blocks.
pub fn llvm_ir<'a>(mut input: ParserPairs<'a>) -> Operation {
    let mut raw = input.next().unwrap().as_str().to_string();

    raw.remove(0);
    raw.remove(raw.len() - 1);

    Operation::Ir(raw)
}

/// [`Operation`] generation for function calls.
pub fn fn_call<'a>(
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

/// [`Operation`] generation for function calls.
pub fn root_function_call<'a>(
    pair: Pair<'a, Rule>,
    operations: &mut Vec<Operation>,
    registers: &mut Registers,
) {
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
        // if: compare 2 values
        "if" => {
            let mut conditional_inner = inner
                .next()
                .unwrap()
                .into_inner()
                .next()
                .unwrap()
                .into_inner()
                .next()
                .unwrap()
                .into_inner();

            let lhs = conditional_inner.next().unwrap();
            let lhs = match lhs.as_rule() {
                Rule::identifier => {
                    let r = random();
                    let var = registers.get_var(lhs.as_str());
                    operations.push(Operation::Ir(format!(
                        "%k_{r} = load i32, ptr %{}.addr, align 4",
                        var.label
                    )));
                    format!("%k_{r}")
                }
                _ => lhs.as_str().to_string(),
            };

            let op = rule_to_operator(conditional_inner.next().unwrap().as_rule());

            let rhs = conditional_inner.next().unwrap();
            let rhs = match rhs.as_rule() {
                Rule::identifier => {
                    let r = random();
                    let var = registers.get_var(rhs.as_str());
                    operations.push(Operation::Ir(format!(
                        "%k_{r} = load i32, ptr %{}.addr, align 4",
                        var.label
                    )));
                    format!("%k_{r}")
                }
                _ => rhs.as_str().to_string(),
            };

            inner.next(); // skip
            let goto = inner.next().unwrap().as_str();
            if let Some(_) = inner.next() {
                // ^ skip
                let goto_next = inner.next().unwrap().as_str();
                // has else block
                let r = random();
                operations.push(Operation::Ir(format!(
                    "%k_cmp_{r} = icmp {op} i32 {lhs}, {rhs}\nbr i1 %k_cmp_{r}, label %{goto}, label %{goto_next}"
                )));
            } else {
                // doesn't have else block
                let r = random();
                operations.push(Operation::Ir(format!(
                    "%k_cmp_{r} = icmp {op} i32 {lhs}, {rhs}\nbr i1 %k_cmp_{r}, label %{goto}"
                )));
            }
        }
        // addset: add `x` to `ident` and update its value
        "addset" => {
            let var_ident = inner.next().unwrap().as_str();
            let var = registers.get_var(var_ident);

            inner.next(); // skip
            let val = inner.next().unwrap().as_str();

            let r = random();
            operations.push(Operation::Ir(format!(
                "%k_{r}_v = load i32, ptr %{}.addr
%k_{r} = add nsw i32 %k_{r}_v, {val}
store i32 %k_{r}, ptr %{}.addr, align {}",
                var.label, var.label, var.align
            )));
        }
        // everything user-defined
        _ => {
            let fun = registers.get_function(sub_function).clone();
            operations.push(fn_call(sub_function.to_string(), inner, registers, &fun));
        }
    }
}

/// [`Operation`] generation for a function return.
pub fn fn_return<'a>(pair: Pair<'a, Rule>) -> String {
    let pair = pair.into_inner().next().unwrap();
    match pair.as_rule() {
        Rule::llvm_ir => match llvm_ir(pair.into_inner()) {
            Operation::Ir(data) => data,
            _ => unreachable!(),
        },
        Rule::call_param => {
            let mut inner = pair.into_inner();
            let mut r#type = "void";

            let value = inner.next().unwrap();
            let value = match value.as_rule() {
                Rule::identifier => {
                    format!("%{}", value.as_str().to_string())
                }
                _ => {
                    r#type = rule_to_type(value.as_rule());
                    value.as_str().to_string()
                }
            };

            if let Some(pair) = inner.next() {
                if pair.as_rule() == Rule::identifier {
                    // overwrite type
                    r#type = pair.as_str()
                }
            }

            format!("{type} {value}")
        }
        _ => pair.as_str().to_string(),
    }
}

/// A value.
///
/// `(value, prefix, size)`
pub struct Value(pub (String, String, usize));

impl Value {
    pub fn get<'a>(pair: Pair<'a, Rule>, key: &str, registers: &mut Registers) -> Self {
        let rule = pair.as_rule();
        match rule {
            Rule::call => {
                let mut inner = pair.into_inner();
                let sub_function = inner.next().unwrap().as_str();

                let fun = registers.get_function(sub_function).clone();
                let value = fn_call(sub_function.to_string(), inner, registers, &fun)
                    .transform(registers)
                    .1;

                let size = value.len();
                return Value((value, format!("%k_{key} = __VALUE_INSTEAD\n"), size));
            }
            Rule::identifier => {
                if pair.as_str() != "void" {
                    let var = registers.get_var(pair.as_str());
                    return Value((format!("%{}", var.label), String::new(), var.size));
                } else {
                    let value = pair.as_str();
                    return Value((value.to_string(), String::new(), value.len()));
                }
            }
            Rule::llvm_ir => match llvm_ir(pair.into_inner()) {
                Operation::Ir(data) => {
                    let size = data.len();
                    return Value((data, String::new(), size));
                }
                _ => unreachable!(),
            },
            _ => {
                let value = pair.as_str().to_string();
                let size = std::mem::size_of_val(value.as_bytes());
                return Value((value, String::new(), size));
            }
        }
    }
}

/// [`Operation`] generation for variable assignment.
pub fn var_assign(
    overwrite_ident: String,
    mut inner: ParserPairs,
    operations: &mut Vec<Operation>,
    registers: &mut Registers,
) -> String {
    // alloc variable
    let mut prefix: String = String::new();
    let mut label: String = String::new(); // stored in registers
    let mut ident: String = overwrite_ident.clone(); // written to ir
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
                let val = Value::get(pair, &key, registers).0;
                value = val.0;
                prefix = val.1;

                if !closed_size {
                    size = val.2;
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
                    let content = pair.as_str();
                    label = content.to_string();
                    ident = content.to_string()
                } else if label.is_empty() {
                    label = pair.as_str().to_string();
                } else {
                    let val = Value::get(pair, &key, registers).0;
                    value = val.0;
                    prefix = val.1;

                    if !closed_size {
                        size = val.2;
                    }
                }
            }
            Rule::llvm_ir => {
                let val = Value::get(pair, &key, registers).0;
                value = val.0;
                prefix = val.1;

                if !closed_size {
                    size = val.2;
                }
            }
            Rule::int => {
                size = pair.as_str().parse::<usize>().unwrap();
                closed_size = true;
            }
            _ => {
                let val = Value::get(pair, &key, registers).0;
                value = val.0;
                prefix = val.1;

                if !closed_size {
                    size = val.2;
                }
            }
        }
    }

    registers.variables.insert(label.clone(), Variable {
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
        operations.push(Operation::Assign(label.clone()));
    }

    if (r#type != "string") & (value != "void") && (prefix != "_drop") {
        operations.push(Operation::Pipe((label.clone(), ident, value)));
    }

    label
}

/// [`Operation`] generation for variable assignment (no alloca).
pub fn var_assign_no_alloca(
    mut inner: ParserPairs,
    operations: &mut Vec<Operation>,
    registers: &mut Registers,
) {
    // alloc variable
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

/// [`Operation`] generation for a for loop.
pub fn for_loop<'a>(
    input: ParserPairs,
    pair: Pair<'a, Rule>,
    file_specifier: &str,
    mut operations: Vec<Operation>,
    registers: &mut Registers,
) -> (Registers, Vec<Operation>) {
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
%{var_name}_{cond_key} = load {}, ptr %{var_name}.addr, align {}
{prefix}
%{var_name}_cmp_{cond_key} = icmp {op} {} %{var_name}_{cond_key}, {value}
br i1 %{var_name}_cmp_{cond_key}, label %{block_body}, label %{block_end}",
        var.r#type, var.align, var.r#type
    )));

    // body
    let block = loop_inner.next().unwrap().into_inner();
    operations.push(Operation::Ir(format!("{block_body}:")));
    let res = crate::process(block, file_specifier, scoped_regs);

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
%{var_name}_{inc_key} = load {}, ptr %{var_name}.addr, align {}
%{var_name}_inc_{inc_key} = add nsw i32 %{var_name}_{inc_key}, 1
store i32 %{var_name}_inc_{inc_key}, ptr %{var_name}.addr, align {}
br label %{block_cond}",
        var.r#type, var.align, var.align
    )));

    // end
    operations.push(Operation::Ir(format!("{block_end}:")));
    let res = crate::process(input, file_specifier, scoped_regs); // capture everything left in `input`

    for operation in res.1 {
        match operation {
            Operation::HeadIr(_) => continue,
            _ => operations.push(operation),
        }
    }

    return (res.0, operations);
}

/// [`Operation`] generation for a while loop.
pub fn while_loop<'a>(
    input: ParserPairs,
    pair: Pair<'a, Rule>,
    file_specifier: &str,
    mut operations: Vec<Operation>,
    registers: &mut Registers,
) -> (Registers, Vec<Operation>) {
    // basically just a modified for loop
    let mut loop_inner = pair.into_inner();

    // block names
    let key = random();
    let block_cond = format!("bb_cond_{key}");
    let block_body = format!("bb_body_{key}");
    let block_end = format!("bb_end_{key}");

    // head
    let scoped_regs = registers.clone(); // create new scope
    operations.push(Operation::Ir(format!("br label %{block_cond}")));

    // cond
    operations.push(Operation::Ir(format!("{block_cond}:")));
    let mut conditional_inner = loop_inner.next().unwrap().into_inner();

    let lhs = conditional_inner.next().unwrap();
    let lhs = match lhs.as_rule() {
        Rule::identifier => {
            let r = random();
            let var = registers.get_var(lhs.as_str());
            operations.push(Operation::Ir(format!(
                "%k_{r} = load i32, ptr %{}.addr, align {}",
                var.label, var.align
            )));
            format!("%k_{r}")
        }
        _ => lhs.as_str().to_string(),
    };

    let op = rule_to_operator(conditional_inner.next().unwrap().as_rule());

    let rhs = conditional_inner.next().unwrap();
    let rhs = match rhs.as_rule() {
        Rule::identifier => {
            let r = random();
            let var = registers.get_var(rhs.as_str());
            operations.push(Operation::Ir(format!(
                "%k_{r} = load i32, ptr %{}.addr, align {}",
                var.label, var.align
            )));
            format!("%k_{r}")
        }
        _ => rhs.as_str().to_string(),
    };

    let r = random();
    operations.push(Operation::Ir(format!(
        "%k_cmp_{r} = icmp {op} i32 {lhs}, {rhs}
br i1 %k_cmp_{r}, label %{block_body}, label %{block_end}",
    )));

    // body
    let block = loop_inner.next().unwrap().into_inner();
    operations.push(Operation::Ir(format!("{block_body}:")));
    let res = crate::process(block, file_specifier, scoped_regs);

    for operation in res.1 {
        operations.push(operation);
    }

    let scoped_regs = res.0; // use updated version of scoped_regs
    registers
        .extra_header_ir
        .push_str(&scoped_regs.extra_header_ir); // make sure header stuff is still global

    operations.push(Operation::Ir(format!("br label %{block_cond}")));

    // end
    operations.push(Operation::Ir(format!("{block_end}:")));
    let res = crate::process(input, file_specifier, scoped_regs); // capture everything left in `input`

    for operation in res.1 {
        match operation {
            Operation::HeadIr(_) => continue,
            _ => operations.push(operation),
        }
    }

    return (res.0, operations);
}
