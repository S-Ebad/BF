#include "lexer.h"
#include "parser.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

// get file size or -1 if failed
static long file_size(FILE *file) {
  if (fseek(file, 0, SEEK_END) != 0) {
    return -1;
  }

  long size = ftell(file);
  if (size < 0) {
    return -1;
  }

  if (fseek(file, 0, SEEK_SET) != 0) {
    return -1;
  }

  return size;
}

static char *read_file(const char *filename, size_t *out_len) {
  FILE *file = fopen(filename, "rb");
  if (!file) {
    return NULL;
  }

  long size = file_size(file);
  if (size == -1) {
    fclose(file);
    return NULL;
  }

  char *buf = malloc((size_t)size + 1);
  if (!buf) {
    fclose(file);
    return NULL;
  }

  size_t read = fread(buf, 1, (size_t)size, file);
  if (read != (size_t)size) {
    free(buf);
    fclose(file);

    return NULL;
  }
  fclose(file);

  buf[read] = 0;
  *out_len = read;

  return buf;
}

int main(int argc, char **argv) {
  if (argc != 2) {
    fprintf(stdout, "usage: %s <file.bf>\n", argv[0]);
    return 1;
  }

  size_t len = 0;
  char *buf = read_file(argv[1], &len);
  if (!buf) {
    perror("read_file");
    return 1;
  }

  Lexer lex;
  lexer_init(&lex, buf, len);

  Bytecode bytecode;

  if(!parse(&lex, &bytecode)) {
    return 1;
  }

  printf("Bytecode{ .ops=%p, .len=%zu }\n",
      (void *)bytecode.ops, bytecode.len);
  for (size_t i = 0; i < bytecode.len; i++) {
    Op *opcode = bytecode.ops + i;

    fprintf(stdout, "%s\n", op_to_str(opcode));
  }

  free(buf);
}
