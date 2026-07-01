#ifndef LEXER_H
#define LEXER_H

#include <stdbool.h>
#include <stddef.h>

typedef struct Lexer {
  const char *src;
  size_t pos;
  size_t len;
} Lexer;

void lexer_init(Lexer *lexer, const char *src, size_t len);

bool lexer_at_end(const Lexer *lex);
int lexer_peek(const Lexer *lex);
int lexer_next(Lexer *lex);

#endif
