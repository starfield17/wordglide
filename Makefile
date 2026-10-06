PYTHON ?= python3

.PHONY: check build clean
check:
	cargo fmt --all -- --check
	cargo clippy --all-targets -- -D warnings
	cargo test --all-targets
	cargo test --doc
	$(PYTHON) -m unittest discover -s scripts -p 'test_*.py' -v

build:
	cargo build --release --bins

clean:
	cargo clean
