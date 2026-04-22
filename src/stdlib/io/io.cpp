/**
 * sekirei stdlib: io
 * print, println, read_line
 */

#include "io.hpp"
#include <cstdio>
#include <string>

extern "C" {

void sk_print(const char *s) {
    fputs(s, stdout);
}

void sk_println(const char *s) {
    puts(s);
}

char *sk_read_line(void) {
    static char buf[4096];
    if (!fgets(buf, sizeof(buf), stdin)) return nullptr;
    /* 末尾の改行を除去 */
    size_t len = strlen(buf); // NOLINT
    if (len > 0 && buf[len - 1] == '\n') buf[len - 1] = '\0';
    return buf;
}

} // extern "C"
