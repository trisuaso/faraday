test test="echo.fd" exec="luajit":
    cargo run --bin faradayc -- test_fd/{{test}} -r={{exec}}

test-lib exec="luajit":
    cd library && cargo run --bin faradayc -- src/main.fd -r={{exec}}

test-rr test="hello_world.rr":
    cargo run --bin faradayc -- test_rr/{{test}} -r=rir

test-rr-run test="hello_world.rr":
    just test-rr {{test}} > build/{{test}}.ll
    llc build/{{test}}.ll -o build/{{test}}.s
    clang build/{{test}}.s -o build/{{test}}.out
    ./build/{{test}}.out
