/* sekirei entry point - aarch64 Linux (AAPCS64) */

    .text
    .global _start
    .type   _start, %function

_start:
    /* Linux aarch64ではカーネルがSPを16バイト境界に合わせて渡す */
    /* ランタイムのmainを呼ぶ */
    bl      sk_main_entry

    /* exit syscall (aarch64: x8=93, x0=exit_code) */
    mov     x8, #93
    svc     #0
    .size   _start, . - _start
