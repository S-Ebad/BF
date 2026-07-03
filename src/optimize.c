#include "optimize.h"
#include "parser.h"

static bool is_combinable(OpKind kind) {
  return (kind == OP_ADD || kind == OP_MOVE) != 0;
}

// TODO: Optimizations
void optimize(struct Bytecode *bytecode) {
  size_t write = 0;

  for (size_t read = 0; read < bytecode->len; read++) {
    Op op = bytecode->ops[read];

    if (write > 0 && is_combinable(op.kind) && bytecode->ops[write - 1].kind == op.kind) {
      bytecode->ops[write - 1].data.offset += op.data.offset;

      if (bytecode->ops[write - 1].data.offset == 0) {
        write--;
      }
      continue;
    }

    bytecode->ops[write++] = op;
  }

  bytecode->len = write;
}
