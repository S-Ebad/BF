#include "stack.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define unlikely(x) __builtin_expect(!!(x), 0)
#define ABORT(msg)                                                                                 \
  do {                                                                                             \
    fprintf(stderr, msg);                                                                          \
    abort();                                                                                       \
  } while (0)

void stack_init(Stack *stk) {
  stk->buf = malloc(STACK_INITIAL_CAP * sizeof(size_t));
  if (!stk->buf) {
    ABORT("stack: memory allocation failed");
  }
  stk->cap = STACK_INITIAL_CAP;
  stk->len = 0;
}

void stack_push(Stack *stk, size_t val) {
  if (unlikely(stk->len == stk->cap)) {
    size_t *buf = realloc(stk->buf, stk->cap * 2 * sizeof(size_t));
    if (unlikely(!buf)) {
      ABORT("stack: memory re-allocation failed");
    }

    stk->buf = buf;
    stk->cap *= 2;
  }
  stk->buf[stk->len++] = val;
}

size_t stack_pop(Stack *stk) {
  if (unlikely(stk->len == 0)) {
    fprintf(stdout, "stack: attempt to pop from empty stack");
    abort();
  }

  return stk->buf[--stk->len];
}

void stack_free(Stack *stk) {
  free(stk->buf);
  memset(stk, 0, sizeof(Stack));
}
