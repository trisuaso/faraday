i32 printn(i8* data) {
    printf(data)
    return ll"i32 0"
}

i32 main() {
    16 string str = "Hello, world!\0A"
    jump(print_string)

    <print_string> {
        printn(str)
        jump(print_number)
    }

    <print_number> {
        string prefix = "Number: %d"
        printf(prefix@ptr, 0@i32)
        return ll"i32 0"
    }
}
