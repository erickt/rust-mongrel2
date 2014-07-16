extern mod std;
extern mod zmq;
extern mod mongrel2;

fn main() {
    let ctx = match zmq::init(1) {
        Ok(ctx) => ctx,
        Err(e) => fail e.to_str(),
    };

    let conn = mongrel2::connect(ctx,
        Some(~"F0D32575-2ABB-4957-BC8B-12DAC8AFF13A"),
        ~[~"tcp://127.0.0.1:9998"],
        ~[~"tcp://127.0.0.1:9999"]);

    loop {
        let request = result::unwrap(conn.recv());
        io::println(#fmt("uuid: %s", request.uuid));
        io::println(#fmt("id: %s", request.id));
        io::println(#fmt("path: %s", request.path));

        for request.headers.each |k, vs| {
            for vs.each |v| {
                io::println(#fmt("header: %s => %s", *k, *v));
            }
        };
        io::println(#fmt("body: %s", str::from_bytes(request.body)));

        conn.reply_http(&request,
            200u,
            "OK",
            mongrel2::Headers(),
            str::to_bytes("hello world!"));
    }
}
