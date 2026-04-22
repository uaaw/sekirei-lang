/**
 * sekirei stdlib: string
 */

#include "string.hpp"
#include <cstring>
#include <cstdlib>
#include <cctype>

extern "C" {

size_t sk_str_len(const char *s) {
    return strlen(s);
}

const char *sk_str_concat(const char *a, const char *b) {
    size_t la = strlen(a), lb = strlen(b);
    char  *buf = (char *)malloc(la + lb + 1);
    memcpy(buf, a, la);
    memcpy(buf + la, b, lb + 1);
    return buf;
}

int sk_str_eq(const char *a, const char *b) {
    return strcmp(a, b) == 0;
}

const char *sk_str_trim(const char *s) {
    while (*s && isspace((unsigned char)*s)) s++;
    size_t len = strlen(s);
    char  *buf = (char *)malloc(len + 1);
    memcpy(buf, s, len + 1);
    while (len > 0 && isspace((unsigned char)buf[len - 1])) buf[--len] = '\0';
    return buf;
}

} // extern "C"
