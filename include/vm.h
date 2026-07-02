#ifndef VM_H
#define VM_H

#define MAX_TAPE_SIZE 30000

struct Bytecode;

// takes ownership 
void vm(const struct Bytecode *bytecode);

#endif
