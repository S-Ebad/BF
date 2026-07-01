#ifndef PARSER_H
#define PARSER_H

#include "lexer.h"
#include <stddef.h>
#include <stdint.h>

typedef enum OpKind {
  OP_ADD,  // *p += n
  OP_MOVE, // p += n
  OP_OUT,  // putchar(*p)
  OP_IN,   // *p = getchar()
  OP_JZ,   // if (*p == 0) jump to operand
  OP_JNZ,  // if (*p != 0) jump to operand
} OpKind;

typedef struct Op {
  OpKind kind;
  union {
    int64_t offset;
    size_t jump;
  } data;
} Op;

typedef struct Bytecode {
  Op *ops;
  size_t len;
} Bytecode;

const char *op_to_str(Op *opcode);
bool parse(Lexer *lex, Bytecode *out);

#endif
