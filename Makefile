all:
	rustc mongrel2.rc

test:
	rustc --test mongrel2.rc

example: all
	rustc -L . example.rs

deps:
	cargo install -g zmq
	cargo install -g tnetstring

clean:
	rm -rf mongrel2 example *.so *.dylib *.dSYM
