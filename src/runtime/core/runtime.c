/**
 * sekirei Runtime Core
 */

#include "runtime.h"
#include "../gc/gc.h"
#include <stdio.h>
#include <stdlib.h>

void sk_runtime_init(void) {
    sk_gc_init();
}

void sk_runtime_shutdown(void) {
    sk_gc_shutdown();
}

extern void sk_user_main(void);

int sk_main_entry(void) {
    sk_runtime_init();
    sk_user_main();
    sk_runtime_shutdown();
    return 0;
}

int main(void) {
    return sk_main_entry();
}
