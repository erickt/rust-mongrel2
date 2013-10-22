RUSTPKG ?= rustpkg
RUST_FLAGS ?= -Z debug-info -O

all:
	$(RUSTPKG) $(RUST_FLAGS) install mongrel2

test:
	$(RUSTPKG) test mongrel2

clean:
	rm -rf bin build lib
