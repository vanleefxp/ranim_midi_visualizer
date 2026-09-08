set shell := ["powershell"]

install:
    cargo install --path . --locked

run *args:
    cargo run {{ args }} --features ui,preview

build:
    cargo build --release --features ui,preview

stat:
    tokei -t rust -C

fmt:
    cargo fmt --all

lint: fmt
    cargo clippy --workspace --all-targets -- -D warnings

test:
    cargo test --workspace

test-preview *args:
    cargo run -- preview "./crates/waveform-utils/src/music/tests/song_2.mid" {{ args }}

test-render *args:
    cargo run -- render "./crates/waveform-utils/src/music/tests/song_2.mid" {{ args }}
