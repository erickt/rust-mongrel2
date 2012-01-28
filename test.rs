use std;
import std::map;
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
        let request = conn.recv();
        println(#fmt("uuid: %s", str::from_bytes(request.uuid)));
        println(#fmt("id: %s", str::from_bytes(request.id)));
        println(#fmt("path: %s", str::from_bytes(request.path)));

        request.headers.items {|k,v|
            println(#fmt("header: %s => %s",
                str::from_bytes(k),
                str::from_bytes(v)));
        };
        println(#fmt("body: %s", str::from_bytes(request.body)));

        conn.reply_http(request,
            str::bytes("hello world!"),
            200u,
            str::bytes("OK"),
            map::new_bytes_hash());
    }

    conn.term();
    ctx.term();
}
