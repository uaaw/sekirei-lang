/**
 * sekirei GC (Mark-and-Sweep)
 * シンプルなmark & sweepガベージコレクタ
 */

#include "gc.h"
#include <stdlib.h>
#include <string.h>

#define HEAP_SIZE (1024 * 1024 * 8)  /* 8MB 初期ヒープ */

typedef struct GcHeader {
    size_t          size;
    uint8_t         marked;
    struct GcHeader *next;
} GcHeader;

static GcHeader *gc_head  = NULL;
static size_t    gc_bytes = 0;

void sk_gc_init(void) {
    gc_head  = NULL;
    gc_bytes = 0;
}

void *sk_gc_alloc(size_t size) {
    GcHeader *hdr = (GcHeader *)malloc(sizeof(GcHeader) + size);
    if (!hdr) return NULL;

    hdr->size   = size;
    hdr->marked = 0;
    hdr->next   = gc_head;
    gc_head     = hdr;
    gc_bytes   += size;

    return (void *)(hdr + 1);
}

static void gc_mark(GcHeader *hdr) {
    if (!hdr || hdr->marked) return;
    hdr->marked = 1;
    /* TODO: 内部ポインタをたどって再帰マーク */
}

static void gc_sweep(void) {
    GcHeader **cur = &gc_head;
    while (*cur) {
        if (!(*cur)->marked) {
            GcHeader *unreachable = *cur;
            *cur       = unreachable->next;
            gc_bytes  -= unreachable->size;
            free(unreachable);
        } else {
            (*cur)->marked = 0;
            cur = &(*cur)->next;
        }
    }
}

void sk_gc_collect(void) {
    /* TODO: ルートセットのマーク */
    gc_sweep();
}

void sk_gc_shutdown(void) {
    sk_gc_collect();
}
