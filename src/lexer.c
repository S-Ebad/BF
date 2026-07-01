#include "lexer.h"
#include "assert.h"

#define EOF (-1)

void lexer_init(Lexer *lexer, const char *src, size_t len) {
  assert(src != NULL);

  lexer->src = src;
  lexer->len = len;
  lexer->pos = 0;
}

bool lexer_at_end(const Lexer *lex) {
  return lex->pos >= lex->len;
}
int lexer_peek(const Lexer *lex) {
  if (lexer_at_end(lex)) {
    return EOF;
  }

  return (unsigned char)lex->src[lex->pos];
}
int lexer_next(Lexer *lex) {
  if (lexer_at_end(lex)) {
    return EOF;
  }

  return (unsigned char)lex->src[lex->pos++];
}
