#pragma once
#include <stddef.h>
#include <stdint.h>

void  sk_gc_init(void);
void *sk_gc_alloc(size_t size);
void  sk_gc_collect(void);
void  sk_gc_shutdown(void);
