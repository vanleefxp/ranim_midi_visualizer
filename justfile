set shell := ["powershell"]

install:
    cargo install --path . --locked

run *args:
    cargo run {{ args }}

build:
    cargo build --release

stat:
    tokei -t rust -C

fmt:
    cargo fmt --all

lint: fmt
    cargo clippy --workspace --all-targets -- -D warnings

test:
    cargo test --workspace

test-preview *args:
    just run preview "./crates/waveform-utils/src/music/tests/song_2.mid" {{ args }}

test-render *args:
    just run render "./crates/waveform-utils/src/music/tests/song_2.mid" {{ args }}
