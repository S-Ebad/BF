CC = gcc
CFLAGS = -Wall -Wextra -std=c99 -pedantic -MMD -MP -Iinclude

SRCS = src/main.c

BUILD_DIR = build
TARGET = $(BUILD_DIR)/main

OBJ = $(SRCS:src/%.c=$(BUILD_DIR)/%.o)
DEPS = $(OBJ:.o=.d)

all: $(TARGET)

$(BUILD_DIR):
	mkdir -p $(BUILD_DIR)

$(BUILD_DIR)/%.o: src/%.c | $(BUILD_DIR)
	$(CC) $(CFLAGS) -c $< -o $@

$(TARGET): $(OBJ)
	$(CC) $(OBJ) -o $(TARGET)

run: $(TARGET)
	./$(TARGET)

clean:
	rm -rf $(BUILD_DIR)

-include $(DEPS)
