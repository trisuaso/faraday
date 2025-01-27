test test="echo.fd" exec="luajit":
    cargo run --bin faradayc -- test_fd/{{test}} -r={{exec}}

test-lib exec="luajit":
    cd library && cargo run --bin faradayc -- src/main.fd -r={{exec}}
