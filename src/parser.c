#include "parser.h"
#include "stack.h"
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>

static bool is_valid_bf(int chr) {
  switch (chr) {
  case '+':
  case '-':
  case '>':
  case '<':
  case '[':
  case ']':
  case '.':
  case ',':
    return true;
  default:
    return false;
  }

  __builtin_unreachable();
}

const char *op_to_str(Op *opcode) {
  static char buf[20];

  switch (opcode->kind) {
  case OP_ADD:
    sprintf(buf, "Add %ld", opcode->data.offset);
    break;

  case OP_MOVE:
    sprintf(buf, "Move %ld", opcode->data.offset);
    break;

  case OP_OUT:
    sprintf(buf, "Output");
    break;
  case OP_IN:
    sprintf(buf, "Input");
    break;

  case OP_JZ:
    sprintf(buf, "JZ %zu", opcode->data.jump);
    break;

  case OP_JNZ:
    sprintf(buf, "JNZ %zu", opcode->data.jump);
    break;
  }

  return buf;
}

bool parse(Lexer *lex, Bytecode *out) {
  Op *ops = malloc(lex->len * sizeof(Op));
  if (!ops) {
    return false;
  }

  Stack jmps;
  stack_init(&jmps);

  size_t len = 0;

  int chr = -1;
  while ((chr = lexer_next(lex)) != EOF) {

    if (chr == ';') {
      do {
        chr = lexer_next(lex);
      } while (chr != '\n' && chr != EOF);
    }

    if (!is_valid_bf(chr)) {
      continue;
    }

    Op *opcode = &ops[len];

    switch (chr) {
    case '-':
    case '+':
      opcode->kind = OP_ADD;
      opcode->data.offset = (chr == '+') ? 1 : -1;
      break;

    case '>':
    case '<':
      opcode->kind = OP_MOVE;
      opcode->data.offset = (chr == '>') ? 1 : -1;
      break;

    case '[':
      opcode->kind = OP_JZ;
      opcode->data.jump = 0;

      stack_push(&jmps, len);
      break;
    case ']': {
      // mismatching brackets
      if (jmps.len == 0) {
        fprintf(stderr, "parse error: unmatched ']' at offset %zu\n", lex->pos);
        goto error_cleanup;
      }

      size_t open = stack_pop(&jmps);
      ops[open].data.jump = len + 1;

      opcode->kind = OP_JNZ;
      opcode->data.jump = len - open;

    } break;
    case '.':
      opcode->kind = OP_OUT;
      break;
    case ',':
      opcode->kind = OP_IN;
      break;

    default:
      fprintf(stderr, "parser: unreachable opcode");
      abort();
    }

    len++;
  }

  if (jmps.len != 0) {
    fprintf(stderr, "parse error: unmatched '[' (%zu unclosed)\n", jmps.len);
    goto error_cleanup;
  }

  Op *shrunk = realloc(ops, len * sizeof(Op));
  out->ops = shrunk ? shrunk : ops;
  out->len = len;

  stack_free(&jmps);
  return true;

error_cleanup:
  free(ops);
  stack_free(&jmps);

  return false;
}
