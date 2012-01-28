use std;
import std::io::println;

use zmq;
import zmq::{context, error};

use mongrel2;
import mongrel2::connection;

fn main() {
    let ctx =
        alt zmq::init(1) {
            result::ok(ctx) { ctx }
            result::err(e) { fail e.to_str() }
        };

    let conn = mongrel2::connect(ctx,
        str::bytes("F0D32575-2ABB-4957-BC8B-12DAC8AFF13A"),
        str::bytes("tcp://127.0.0.1:9998"),
        str::bytes("tcp://127.0.0.1:9999"));

    while true {
        let x = conn.recv();
        println(#fmt("sender: %s", str::from_bytes(x.sender)));
        println(#fmt("id: %d", x.id));
        println(#fmt("path: %s", str::from_bytes(x.path)));

        x.headers.items {|k,v|
            println(#fmt("header: %s => %s",
                str::from_bytes(k),
                str::from_bytes(v)));
        };
        println(#fmt("body: %s", str::from_bytes(x.body)));
    }

    conn.term();
    ctx.term();
}
