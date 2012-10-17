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

Running the tests:
    % make test && ./mongrel2

Running the example:
    # In one shell do:
    % m2sh load --db config.sqlite --config example.conf
    % m2sh start --db config.sqlite --host localhost

    # In another shell do:
    % make example && ./example

    # In a third shell do:
    % curl http://localhost:6767
