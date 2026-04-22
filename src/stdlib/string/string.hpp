#pragma once
#include <cstddef>

extern "C" {
    size_t      sk_str_len(const char *s);
    const char *sk_str_concat(const char *a, const char *b);
    int         sk_str_eq(const char *a, const char *b);
    const char *sk_str_trim(const char *s);
}
