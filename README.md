# 🦇 Faraday

*Faraday* is an experimental, strongly-typed language which compiles to Lua source.

## Features

* Type checking
    * `any` and `empty` types
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

See the [tests](https://github.com/trisuaso/faraday/tree/master/test_fd) for some language examples!
