NAME   := Gomoku
TARGET := target/release/gomoku
SRCS   := $(shell find src -name '*.rs') Cargo.toml Cargo.lock

all: $(NAME)

$(NAME): $(TARGET)
	cp $(TARGET) $(NAME)

$(TARGET): $(SRCS)
	cargo build --release

clean:
	cargo clean

fclean: clean
	rm -f $(NAME)

re: fclean all

.PHONY: all clean fclean re
