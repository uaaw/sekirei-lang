fn main() {
    // runtime (C)
    cc::Build::new()
        .files([
            "src/runtime/gc/gc.c",
            "src/runtime/memory/allocator.c",
            "src/runtime/core/runtime.c",
        ])
        .compile("sekirei_runtime");

    // stdlib (C++)
    cc::Build::new()
        .cpp(true)
        .files([
            "src/stdlib/io/io.cpp",
            "src/stdlib/math/math.cpp",
            "src/stdlib/string/string.cpp",
            "src/stdlib/collections/collections.cpp",
        ])
        .compile("sekirei_stdlib");

    // assembly (.s = GNU as形式、ccがgcc経由でアセンブル)
    cc::Build::new()
        .file("src/asm/entry.s")
        .file("src/asm/primitives.s")
        .compile("sekirei_asm");

    println!("cargo:rerun-if-changed=src/runtime");
    println!("cargo:rerun-if-changed=src/stdlib");
    println!("cargo:rerun-if-changed=src/asm");
}
