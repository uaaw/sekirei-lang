/**
 * sekirei Memory Allocator
 * GCと連携するアロケータ
 */

#include "allocator.h"
#include "../gc/gc.h"

void *sk_alloc(size_t size) {
    return sk_gc_alloc(size);
}

void sk_free(void *ptr) {
    (void)ptr;
    /* GC管理下のメモリは手動freeしない */
}
