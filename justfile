set shell := ["powershell"]

install:
	cargo install --path .

run *args:
	cargo fmt
	cargo run {{ args }}