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
