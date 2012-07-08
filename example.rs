use std;

import io::println;
import dvec::{dvec, extensions};
import std::map;
import std::map::hashmap;

use zmq;
import zmq::{context, to_str};

use mongrel2;
import mongrel2::connection;

fn main() {
    let ctx = alt zmq::init(1) {
      result::ok(ctx) { ctx }
      result::err(e) { fail e.to_str() }
    };

    let conn = mongrel2::connect(ctx,
        some("F0D32575-2ABB-4957-BC8B-12DAC8AFF13A"),
        ~["tcp://127.0.0.1:9998"],
        ~["tcp://127.0.0.1:9999"]);

    loop {
        let request = conn.recv().get();
        println(#fmt("uuid: %s", *request.uuid));
        println(#fmt("id: %s", *request.id));
        println(#fmt("path: %s", *request.path));

        for request.headers.each |k, vs| {
            for (*vs).each |v| {
                println(#fmt("header: %s => %s", k, *v));
            }
        };
        println(#fmt("body: %s", str::from_bytes(copy *request.body)));

        conn.reply_http(request,
            200u,
            "OK",
            map::str_hash(),
            str::bytes("hello world!"));
    }
}
