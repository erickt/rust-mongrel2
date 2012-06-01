rust-mongrel2 is a [Rust language](http://rust-lang.org) binding for the
[Mongrel2](http://mongrel2.org) web server.

Installation
------------

Rust's packaging system, cargo, is still pretty rough, and it doesn't
automatically install dependencies. So this means there are two ways to install
rust-mongrel2.

Install for users of rust-mongrel2:

    % cargo install zmq
    % cargo install tnetstring
    % cargo install mongrel2

Install for developers:

    % git clone https://github.com/erickt/rust-mongrel2
    % cd rust-mongrel2
    % make deps
    % make

    # If you want to run the tests and examples...
    % make test && ./test
    % make example && ./example
