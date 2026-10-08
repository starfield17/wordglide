PYTHON ?= python3

.PHONY: check build clean clean-all
check:
	cargo fmt --all -- --check
	cargo clippy --locked --all-targets -- -D warnings
	cargo test --locked --all-targets
	cargo test --locked --doc
	$(PYTHON) -m unittest discover -s scripts -p 'test_*.py' -v

build:
	cargo clean
	cargo build --locked --release --bins

clean:
	cargo clean

# These directories contain disposable verification and packaging outputs.
clean-all: clean
	rm -rf -- "$(CURDIR)/artifacts" "$(CURDIR)/dist"
