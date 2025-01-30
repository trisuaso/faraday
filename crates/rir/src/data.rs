use crate::icompiler_error;
use std::collections::HashMap;

pub trait ToIr {
    /// Convert to LLVM IR.
    ///
    /// # Returns
    /// ` (root level, scoped)`
    fn transform(&self, registers: &mut Registers) -> (String, String);
}

#[derive(Clone)]
pub struct Registers {
    pub variables: HashMap<String, Variable>,
    pub sections: HashMap<String, Section>,
    pub functions: HashMap<String, Function>,
}

macro_rules! llvm_function {
    (declare $t:ident @$name:ident($($args:expr),*) >> $into:ident) => {
        let name = stringify!($name).to_string();
        $into.insert(name.clone(), Function {
            ident: name,
            ret_type: stringify!($t).to_string(),
            args: vec![$($args,)*],
            operations: Vec::new()
        })
    }
}

impl Default for Registers {
    fn default() -> Self {
        Self {
            variables: HashMap::new(),
            sections: HashMap::new(),
            functions: {
                let mut out = HashMap::new();
                llvm_function!(declare i32 @puts(("i8*".to_string(), String::new())) >> out);
                llvm_function!(declare i32 @printf(("i8*".to_string(), String::new())) >> out);
                out
            },
        }
    }
}

impl Registers {
    pub fn get_var(&self, key: &str) -> Variable {
        if let Some(v) = self.variables.get(key) {
            v.to_owned()
        } else {
            icompiler_error!("attempted to get invalid variable: {key}")
        }
    }

    pub fn get_var_mut(&mut self, key: &str) -> &mut Variable {
        if let Some(v) = self.variables.get_mut(key) {
            v
        } else {
            icompiler_error!("attempted to get invalid variable: {key}")
        }
    }

    pub fn get_section(&self, key: &str) -> &Section {
        if let Some(s) = self.sections.get(key) {
            s
        } else {
            icompiler_error!("attempted to get invalid section: {key}")
        }
    }

    pub fn get_function(&self, key: &str) -> &Function {
        if let Some(f) = self.functions.get(key) {
            f
        } else {
            icompiler_error!("attempted to get invalid function: {key}")
        }
    }
}

macro_rules! clone_register {
    ($src:ident.$field:ident >> $new:ident) => {
        for value in $src.$field.to_owned() {
            $new.$field.insert(value.0, value.1);
        }
    };
}

macro_rules! clone_registers {
    ($src:ident; $t:ident) => {{
        let mut new = $t::default();
        clone_register!($src.variables >> new);
        clone_register!($src.sections >> new);
        clone_register!($src.functions >> new);
        new
    }};
}

#[macro_export]
macro_rules! merge_register {
    ($src:ident.$field:ident >> $dest:ident) => {{
        for value in $src.$field.to_owned() {
            $dest.$field.insert(value.0, value.1);
        }
    }};
}

#[macro_export]
macro_rules! merge_registers {
    ($src:ident + $dest:ident) => {{
        merge_register!($src.variables >> $dest);
        merge_register!($src.sections >> $dest);
        merge_register!($src.functions >> $dest);
    }};
}

/// A single function operation.
///
/// # Example
/// ```text
/// #do BitAnd(0, 1, branch_then, branch_else, branch_else_if)
/// ```
#[derive(Clone)]
pub enum Operation {
    /// Assign a variable.
    ///
    /// # Parameters
    /// * `ident`
    Assign(String),
    /// An instruction section.
    ///
    /// # Parameters
    /// * `ident`
    Section(String),
    /// A function definition.
    ///
    /// # Parameters
    /// * `ident`
    Function(String),
    /// Jump to section.
    ///
    /// # Parameters
    /// * `ident`
    Jump(String),
    /// Pipe data to variable.
    Pipe((String, String)),
    /// Call a function.
    Call((String, String)),
    /// Raw LLVM IR.
    Ir(String),
    /// Read variable memory.
    Read(String),
}

impl ToIr for Operation {
    fn transform(&self, registers: &mut Registers) -> (String, String) {
        use Operation::*;
        match self {
            Assign(ident) => {
                let var = registers.get_var(&ident);
                if var.r#type == "string" {
                    return (
                        format!(
                            "@.s_{ident} = constant [{} x i8] c\"{}\\00\\00\", align 1",
                            var.size,
                            {
                                let mut val = var.value.clone();
                                val.remove(0);
                                val.remove(val.len() - 1);
                                val
                            }
                        ),
                        format!(
                            "%{ident}.addr = getelementptr [{} x i8],[{} x i8]* @.s_{ident}, i64 0, i64 0",
                            var.size, var.size
                        ),
                    );
                } else if var.r#type == "faraday::no_alloca" {
                    return (String::new(), format!("%{ident} = {}", var.value));
                }

                // read: %{ident} = load {type}, ptr %p_ident, align 4
                (
                    String::new(),
                    format!(
                        "%{ident}.addr = alloca [{} x {}], align 4",
                        var.size, var.r#type
                    ),
                )
            }
            Section(ident) => registers
                .get_section(&ident)
                .transform(&mut clone_registers!(registers; Registers)),
            Function(ident) => registers
                .get_function(&ident)
                .transform(&mut clone_registers!(registers; Registers)),
            Jump(ident) => (String::new(), format!("br label %{ident}")),
            Pipe((ident, value)) => {
                let var = registers.get_var_mut(ident);
                var.value = value.to_owned();

                let mut val: String = String::new();
                (
                    if var.r#type == "string" {
                        icompiler_error!("cannot reassign string values (constant)")
                    } else {
                        if val.is_empty() {
                            val = var.value.clone();
                        }

                        String::new()
                    },
                    if !var.prefix.is_empty() {
                        // call
                        format!(
                            "{}store {} %1, ptr %{ident}.addr, align 4",
                            var.prefix, var.r#type
                        )
                        .replace("__VALUE_INSTEAD", &val)
                    } else {
                        // simple expression
                        format!("store {} {val}, ptr %{ident}.addr, align 4", var.r#type)
                    },
                )
            }
            Call((ident, args_string)) => {
                let fun = registers.get_function(&ident);
                (
                    String::new(),
                    format!("call {} @{ident}({args_string})", fun.ret_type),
                )
            }
            Ir(data) => (String::new(), data.trim().to_owned()),
            Read(ident) => {
                let var = registers.get_var_mut(ident);

                (
                    String::new(),
                    format!("%{ident} = load {}, ptr %{ident}.addr, align 4", var.r#type),
                )
            }
        }
    }
}

/// A section is a grouping of execution steps.
#[derive(Clone)]
pub struct Section {
    pub ident: String,
    pub operations: Vec<Operation>,
}

impl ToIr for Section {
    fn transform(&self, registers: &mut Registers) -> (String, String) {
        let mut root_out: String = String::new();
        let mut out: String = format!("{}:\n", self.ident);

        for op in &self.operations {
            let data = op.transform(registers);

            root_out.push_str(&format!("{}\n", data.0));
            out.push_str(&format!("    {}\n", data.1.replace("\n", "\n    ")));
        }

        (root_out, format!("{out}"))
    }
}

/// A function is another grouping of execution steps.
#[derive(Clone)]
pub struct Function {
    pub ident: String,
    pub ret_type: String,
    /// variable names are their arg index
    pub args: Vec<(String, String)>,
    pub operations: Vec<Operation>,
}

impl ToIr for Function {
    fn transform(&self, registers: &mut Registers) -> (String, String) {
        let mut parameters: String = String::new();

        for (i, (t, param)) in self.args.iter().enumerate() {
            if i == self.args.len() - 1 {
                // last
                parameters.push_str(&format!("{t} {param}"));
            } else {
                // not last
                parameters.push_str(&format!("{t} {param}, "));
            }
        }

        // ...
        let mut root_out: String = String::new();
        let mut out: String = format!(
            "define {} @\"{}\"({parameters}){{\n",
            self.ret_type, self.ident
        );

        for op in &self.operations {
            let data = op.transform(registers);

            root_out.push_str(&format!("{}\n", data.0));
            out.push_str(&format!("    {}\n", data.1.replace("\n", "\n    ")));
        }

        (root_out, format!("{out}}}"))
    }
}

/// A binding of a label to a pointer of the given size.
///
/// # Declaration
/// ```text
/// [type; size] {label} = {value}
/// ```
///
/// # Example
/// ```text
/// // variable "test" (value of 11) is of type `i32` and is 2 bytes large
/// [i32; 2] test = 11
/// ```
#[derive(Clone, Debug)]
pub struct Variable {
    pub prefix: String,
    pub label: String,
    pub size: usize,
    pub value: String,
    pub r#type: String,
}

impl From<&str> for Variable {
    fn from(value: &str) -> Self {
        Self {
            prefix: String::new(),
            label: value.to_string(),
            size: 0,
            value: value.to_string(),
            r#type: "void".to_string(),
        }
    }
}
