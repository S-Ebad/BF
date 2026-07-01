#ifndef STACK_H
#define STACK_H

#include <stddef.h>

typedef struct Stack {
  size_t *buf;
  size_t len;
  size_t cap;
} Stack;

#define STACK_INITIAL_CAP 64

void stack_init(Stack *stk);
void stack_push(Stack *stk, size_t val);
size_t stack_pop(Stack *stk);

void stack_free(Stack *stk);

#endif
