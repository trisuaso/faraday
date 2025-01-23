test test="echo.fd" exec="luajit":
    cargo run --bin faradayc -- test_fd/{{test}} -r={{exec}}
