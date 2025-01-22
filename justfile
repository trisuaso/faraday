test test="echo.fd":
    cargo run --bin faradayc -- test_fd/{{test}} -r=luajit -b
