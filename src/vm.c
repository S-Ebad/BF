#include "vm.h"
#include "parser.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

typedef unsigned char cell;

void vm(const struct Bytecode *bytecode) {
  static cell tape[MAX_TAPE_SIZE];
  memset(tape, 0, MAX_TAPE_SIZE);

  unsigned int pc = 0;
  for (size_t i = 0; i < bytecode->len; i++) {
    Op *op = &bytecode->ops[i];

    switch (op->kind) {
    case OP_ADD:
      tape[pc] += op->data.offset;
      break;
    case OP_MOVE:
      pc += op->data.offset;
      break;
    case OP_IN:
      tape[pc] = getchar();
      break;
    case OP_OUT:
      putchar((char)tape[pc]);
      break;
    case OP_JZ:
      if (tape[pc] == 0) {
        i += op->data.jump - 1;
      }
      break;
    case OP_JNZ:
      if (tape[pc] != 0) {
        i -= op->data.jump;
      }
      break;
    }
  }
}
