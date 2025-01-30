i32 square(i32 num) {
    res := ll"mul nsw i32 %num, %num"
    return ll"i32 %res"
}

i32 main() {
    <entry> {
        // sections are called with the "jump" expression
        jump(do_stuff)
    }

    // <do_stuff::block::0> {}
    // <do_stuff::block::1> {}

    <do_stuff> {
        // "alloc" can be used to allocate the set number of bytes to a pointer
        i32 x = 0
        // the "<|" (pipe) operator can be used to push data into the given variable
        x <| 5
        // check if 1 == 1
        // result := ll"and i1 true" // embedded llvm ir
        // if(result, do_stuff::block::0, do_stuff::block::1)

        *x
        i32 result = square(x)
        *result // we can "read" variable pointers with *

        string c_int_print = "%d"
        printf(c_int_print@ptr, result@i32)

        return ll"i32 0"
    }
}
