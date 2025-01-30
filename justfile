test test="echo.fd" exec="luajit":
    cargo run --bin faradayc -- test_fd/{{test}} -r={{exec}}

test-lib exec="luajit":
    cd library && cargo run --bin faradayc -- src/main.fd -r={{exec}}

test-i test="hello_world.i":
    cargo run --bin faradayc -- test_i/{{test}} -r=rir

test-i-run test="hello_world.i":
    just test-i {{test}} > build/{{test}}.ll
    llc build/{{test}}.ll -o build/{{test}}.s
    clang build/{{test}}.s -o build/{{test}}.out
    ./build/{{test}}.out
