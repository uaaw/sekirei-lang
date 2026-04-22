/* sekirei primitives - aarch64 (AAPCS64) */

    .text

/* sk_memcpy_fast(dst: *mut u8, src: *const u8, n: usize)
 * x0 = dst, x1 = src, x2 = n
 */
    .global sk_memcpy_fast
sk_memcpy_fast:
    cbz     x2, .Lmemcpy_done
.Lmemcpy_loop:
    ldrb    w3, [x1], #1
    strb    w3, [x0], #1
    subs    x2, x2, #1
    bne     .Lmemcpy_loop
.Lmemcpy_done:
    ret

/* sk_memset_fast(dst: *mut u8, val: u8, n: usize)
 * x0 = dst, x1 = val, x2 = n
 */
    .global sk_memset_fast
sk_memset_fast:
    cbz     x2, .Lmemset_done
.Lmemset_loop:
    strb    w1, [x0], #1
    subs    x2, x2, #1
    bne     .Lmemset_loop
.Lmemset_done:
    ret

/* sk_atomic_inc(ptr: *mut i64) -> i64
 * x0 = ptr, returns new value in x0
 */
    .global sk_atomic_inc
sk_atomic_inc:
.Linc_retry:
    ldxr    x1, [x0]
    add     x1, x1, #1
    stxr    w2, x1, [x0]
    cbnz    w2, .Linc_retry
    mov     x0, x1
    ret

/* sk_atomic_dec(ptr: *mut i64) -> i64
 * x0 = ptr, returns new value in x0
 */
    .global sk_atomic_dec
sk_atomic_dec:
.Ldec_retry:
    ldxr    x1, [x0]
    sub     x1, x1, #1
    stxr    w2, x1, [x0]
    cbnz    w2, .Ldec_retry
    mov     x0, x1
    ret
