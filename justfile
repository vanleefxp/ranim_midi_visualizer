set shell := ["powershell"]

install:
	cargo install --path .

run +args="ui":
	cargo fmt
	cargo run {{ args }}