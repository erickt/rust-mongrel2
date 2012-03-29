use std;
import std::map;
import std::map::hashmap;
import io::println;

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
        "F0D32575-2ABB-4957-BC8B-12DAC8AFF13A",
        "tcp://127.0.0.1:9998",
        "tcp://127.0.0.1:9999");

    while true {
        let request = conn.recv();
        println(#fmt("uuid: %s", request.uuid));
        println(#fmt("id: %s", request.id));
        println(#fmt("path: %s", request.path));

        request.headers.items {|k,v|
            println(#fmt("header: %s => %s", k, str::connect(v, " ")));
        };
        println(#fmt("body: %s", str::from_bytes(request.body)));

        conn.reply_http(request,
            200u,
            "OK",
            map::str_hash(),
            str::bytes("hello world!"));
    }

    conn.term();
    ctx.term();
}
