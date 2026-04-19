set shell := ["powershell"]

install:
	cargo install --path .

run *args:
	cargo fmt
	cargo run {{ args }}

build:
	cargo fmt
	cargo build --release
