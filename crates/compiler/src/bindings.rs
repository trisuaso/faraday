use crate::data::{Function, Type, TypeVisibility};
use std::collections::BTreeMap;
use std::sync::LazyLock;

pub const TYPE_NAME_EMPTY: &str = "empty";
pub const TYPE_NAME_EMPTY_ALT: &str = "#";
pub const TYPE_NAME_ANY: &str = "any";
pub const TYPE_NAME_INT: &str = "int";
pub const TYPE_NAME_FLOAT: &str = "float";
pub const TYPE_NAME_NUMBER: &str = "number";
pub const TYPE_NAME_BOOLEAN: &str = "bool";
pub const TYPE_NAME_STRING: &str = "String";
pub const TYPE_NAME_TABLE: &str = "Table";
pub const TYPE_NAME_REF: &str = "ref";

macro_rules! import_default_type {
    ($type_name:ident >> $map:ident) => {
        $map.insert(
            $type_name.to_string(),
            ($type_name, TypeVisibility::Private).into(),
        );
    };

    ($type_name:ident($($generics:expr),+) >> $map:ident) => {
        $map.insert(
            $type_name.to_string(),
            ($type_name, vec![$($generics.to_string()),+], TypeVisibility::Private).into(),
        );
    };
}

macro_rules! lua_builtin_fn {
    ($fn_name:literal($($names:expr),+ ; $($types:expr),+) -> $return_type:ident >> $map:ident) => {
        $map.insert($fn_name.to_string(), crate::data::Function {
            ident: $fn_name.to_string(),
            arguments: $crate::data::FunctionArguments {
                keys: vec![$($names.to_string()),+],
                types: vec![$(Type::from(($types, TypeVisibility::Public))),+],
            },
            return_type: $crate::data::Type::from($return_type),
            body: String::new(),
            visibility: $crate::data::TypeVisibility::Private,
            execution: $crate::data::ExecutionType::Sync,
            association: $crate::data::AssociationType::Static
        });
    };
}

pub static TYPE_BINDINGS: LazyLock<BTreeMap<String, Type>> = LazyLock::new(|| {
    let mut map = BTreeMap::default();

    import_default_type!(TYPE_NAME_STRING >> map);
    import_default_type!(TYPE_NAME_INT >> map);
    import_default_type!(TYPE_NAME_FLOAT >> map);
    import_default_type!(TYPE_NAME_NUMBER >> map);

    import_default_type!(TYPE_NAME_EMPTY >> map);
    import_default_type!(TYPE_NAME_EMPTY_ALT >> map);
    import_default_type!(TYPE_NAME_ANY >> map);
    import_default_type!(TYPE_NAME_REF >> map);

    import_default_type!(TYPE_NAME_TABLE("K", "V") >> map);

    map
});

pub static FUNCTION_BINDINGS: LazyLock<BTreeMap<String, Function>> = LazyLock::new(|| {
    let mut map = BTreeMap::default();

    // misc
    lua_builtin_fn!("print"("message"; TYPE_NAME_STRING) -> TYPE_NAME_STRING >> map);
    lua_builtin_fn!("tonumber"("value"; "any") -> TYPE_NAME_INT >> map);
    lua_builtin_fn!("tostring"("value"; "any") -> TYPE_NAME_STRING >> map);

    // string
    lua_builtin_fn!("String.format"("value", "value"; TYPE_NAME_STRING, "any") -> TYPE_NAME_STRING >> map);

    // io
    lua_builtin_fn!("io.read"("_" ; "empty") -> TYPE_NAME_EMPTY >> map);
    lua_builtin_fn!("io.write"("message"; "string") -> TYPE_NAME_EMPTY >> map);

    // ...
    map
});
