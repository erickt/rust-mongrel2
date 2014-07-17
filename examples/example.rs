extern crate std;
extern crate zmq;
extern crate mongrel2;

use std::collections::HashMap;
use std::str;

fn main() {
    let mut ctx = zmq::Context::new();

    let mut conn = mongrel2::Connection::new(&mut ctx,
        Some("F0D32575-2ABB-4957-BC8B-12DAC8AFF13A".to_string()),
        vec!("tcp://127.0.0.1:9998".to_string()),
        vec!("tcp://127.0.0.1:9999".to_string())).unwrap();

    let headers = HashMap::new();

    loop {
        let request = conn.recv().unwrap();

        println!("uuid: {}", request.uuid);
        println!("id: {}", request.id);
        println!("path: {}", request.path);

        for (k, vs) in request.headers.iter() {
            for v in vs.iter() {
                println!("header: {} => {}", k, v);
            }
        };
        println!("body: {}", str::from_utf8(request.body.as_slice()));

        conn.reply_http(
            &request,
            200u,
            "OK",
            &headers,
            b"hello world!").unwrap();
    }
}
