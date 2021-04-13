all: build

clean:
	cargo clean

release: target/release/libcornerstore.so

build: target/debug/libcornerstore.so
	cargo clippy


target/debug/libcornerstore.so: src/lib.rs Cargo.toml
	cargo build --target x86_64-unknown-linux-gnu

target/release/libcornerstore.so: src/lib.rs Cargo.toml
	cargo build --release --target x86_64-unknown-linux-gnu


.PHONY: deps
deps:
	rustup target add x86_64-unknown-linux-musl