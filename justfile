test test="echo.fd":
    cargo run --bin faradayc -- test_fd/{{test}} out.lua out.json state_out.json -r=luajit -b
