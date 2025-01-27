use std::sync::{LazyLock, RwLock};

use serde::{Deserialize, Serialize};

pub static COMPILER_TEMPLATES: LazyLock<RwLock<CompilerConfig>> =
    LazyLock::new(|| RwLock::new(CompilerConfig::lua()));

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompilerConfig<'a> {
    /// An argument in a function parameters list. (not last argument)
    ///
    /// # Variables
    /// * `$param`
    pub arg: &'a str,
    /// An argument in a function parameters list. (last argument)
    ///
    /// # Variables
    /// * `$param`
    pub last_arg: &'a str,
    /// An asynchronous function.
    ///
    /// # Variables
    /// * `$visibility`
    /// * `$args`
    /// * `$body`
    /// * `$ident`
    pub async_function: &'a str,
    /// A synchronous function.
    ///
    /// # Variables
    /// * `$visibility`
    /// * `$args`
    /// * `$body`
    /// * `$ident`
    pub function: &'a str,
    /// A variable declaration.
    ///
    /// # Variables
    /// * `$visibility`
    /// * `$ident`
    /// * `$value`
    /// * `$typename`
    pub variable: &'a str,
    /// A type identifier.
    ///
    /// # Variables
    /// * `$visibility`
    /// * `$ident`
    pub r#type: &'a str,
    /// An enum definition.
    ///
    /// # Variables
    /// * `$visibility`
    /// * `$ident`
    /// * `$body`
    pub r#enum: &'a str,
    /// A enum field definition.
    ///
    /// # Variables
    /// * `$ident`
    /// * `$value`
    pub enum_field: &'a str,
    /// A type alias.
    ///
    /// # Variables
    /// * `$visibility`
    /// * `$ident`
    /// * `$value`
    pub type_alias: &'a str,
    /// [`crate::data::TypeVisibility::Public`]
    pub visibility_public: &'a str,
    /// [`crate::data::TypeVisibility::Private`]
    pub visibility_private: &'a str,
    /// [`crate::data::MutabilityModifier::Mutable`]
    pub mutability_mutable: &'a str,
    /// [`crate::data::MutabilityModifier::Constant`]
    pub mutability_constant: &'a str,
    /// Asynchronous function call.
    ///
    /// # Variables
    /// * `$ident`
    /// * `$args`
    pub async_call: &'a str,
    /// Synchronous function call.
    ///
    /// # Variables
    /// * `$ident`
    /// * `$args`
    pub call: &'a str,
    /// For loop.
    ///
    /// # Variables
    /// * `$idents`
    /// * `$iter`
    /// * `$body`
    pub r#for: &'a str,
    /// While loop.
    ///
    /// # Variables
    /// * `$condition`
    /// * `$body`
    pub r#while: &'a str,
    /// Conditional.
    ///
    /// # Variables
    /// * `$condition`
    /// * `$body`
    pub conditional: &'a str,
    /// Conditional opening. (else block)
    pub conditional_opening_else: &'a str,
    /// Conditional opening. (not else block)
    pub conditional_opening_no_else: &'a str,
    /// Conditional closing.
    pub conditional_closing: &'a str,
}

impl CompilerConfig<'_> {
    /// Lua defaults for [`CompilerConfig`]
    pub fn lua() -> Self {
        Self {
            arg: "$param, ",
            last_arg: "$param",
            async_function: "$visibility$ident = function ($args)\n   return coroutine.create(function ()\n    $body\nend)\nend\n",
            function: "$visibilityfunction $ident($args)\n    $body\nend\n",
            variable: "$visibility$ident = $value\n",
            r#type: "$visibility$ident = {}\n",
            r#enum: "$visibility$ident = {\n$body}\n",
            enum_field: "$ident = $value,\n",
            type_alias: "$visibility$ident = {}\n",
            visibility_public: "",
            visibility_private: "local ",
            mutability_mutable: "",
            mutability_constant: "",
            async_call: "select(2, coroutine.resume($ident($args)))\n",
            call: "$ident($args)",
            r#for: "for $idents in $iter do\n$body\nend\n",
            r#while: "while $condition do\n$body\nend\n",
            conditional: "\n$keyword $condition $opening\n$body\n$closing",
            conditional_opening_else: "",
            conditional_opening_no_else: " then",
            conditional_closing: "end\n",
        }
    }
}
