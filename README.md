# 🦇 Faraday

*Faraday* is an experimental, strongly-typed language which compiles to Lua source.

## Features

* Type checking
    * `any` and `empty`(/`#`) types
    * `const`(/`incon`) variables
    * Variable assignment
    * Variable reassignment
    * Function arguments
    * Function return value
    * Invalid types
* Structs
* Type aliases
* Enums
* `impl` blocks
    * `static` methods (`static fn ident(...) -> ... {...}`)
    * (optional) `assoc` methods (opposite of static, default; `assoc fn ident(...) -> ... {...}`)
* Braces (instead of `do ... end`/`then ... end`)
* Async/await (coroutine wrappers)
    * Async: `async fn ident(...) -> any {...}`
        * No need to change return type!
    * Await: `#ident(...)`
    * (optional) `sync` methods (opposite of async, default; `sync fn ident(...) -> any {...}`)
* `else if` instead of `elseif` (big feature)
* `use "..." as ...` instead of `require "..."` (with better module resolving)
* Type visibility (`pub`/`prv`)
    * `prv` is optional and is the default
* Automatic exports (anything set as `pub` is automatically)
    * This includes types, which the type checker will recognize!
* Syntax expressions (embedded functions while compiling)
    * Expressions are imported using the `expr_use` function call in a macro expression: `#[expr_use("./file_path")]`
        * The imported file should just contain a single function which has a name exactly matching the file name
        * If the file name has a period in it (that isn't the extension), it can be represented using an underscore
    * Expressions can be called using the `expr_call` function in a macro expression: `#[expr_call(file_name, ...]`
        * The `file_name` should be an identifier which exactly matches the name of the file from `expr_use` (just the file with no extension)
    * See the example [here](https://github.com/trisuaso/faraday/blob/master/test_fd/syntax_expressions/main.fd)

See the [tests](https://github.com/trisuaso/faraday/tree/master/test_fd) for some language examples!
