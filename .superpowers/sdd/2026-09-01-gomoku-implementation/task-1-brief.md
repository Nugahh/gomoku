### Task 1: Project scaffolding

**Files:**
- Create: `Cargo.toml`
- Create: `Makefile`
- Create: `src/main.rs`
- Create: `.gitignore`

**Interfaces:**
- Consumes: nothing (first task).
- Produces: a compiling, empty binary named `gomoku` (lowercase, per Cargo convention — the Makefile copies it to `Gomoku` with the capital). All later tasks add modules under `src/` and `mod` declarations to `main.rs`.

- [ ] **Step 1: Write `Cargo.toml`**

```toml
[package]
name = "gomoku"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "gomoku"
path = "src/main.rs"

[dependencies]
macroquad = "=0.4.13"

[profile.release]
opt-level = 3
lto = true
codegen-units = 1
panic = "unwind"

[lints.clippy]
unwrap_used = "deny"
expect_used = "deny"
panic = "deny"
indexing_slicing = "deny"
```

Note: the `[lints.clippy]` table applies crate-wide by default. `ui.rs` and `main.rs` are exempt per the Global Constraints — Task 15 adds `#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used)]` at the top of those two files specifically, not crate-wide.

- [ ] **Step 2: Write `.gitignore`**

```
/target
/Gomoku
```

- [ ] **Step 3: Write a placeholder `src/main.rs`**

```rust
#![forbid(unsafe_code)]

fn main() {
    println!("gomoku: scaffolding ok");
}
```

- [ ] **Step 4: Verify it builds**

Run: `cargo build --release`
Expected: compiles with no errors, produces `target/release/gomoku`.

- [ ] **Step 5: Write the Makefile**

```make
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
```

- [ ] **Step 6: Verify the Makefile builds and does not relink**

Run: `make`
Expected: builds, produces `./Gomoku`.

Run: `make` again immediately.
Expected: prints `make: 'Gomoku' is up to date.` (or equivalent) — no `cargo build`, no `cp`. This is the no-relink property from spec §12; re-verify it after every later task that touches `src/`.

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml .gitignore src/main.rs Makefile
git commit -m "chore: project scaffolding, Makefile, empty binary"
```

---

