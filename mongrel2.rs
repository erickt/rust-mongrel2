import std::json;
import std::map;
import std::map::hashmap;
import result::{ok, err, chain};
import zmq::{context, socket, error};

export connect;
export connection;
export request;

type connection_t = {
    sender_id: str,
    sub_addr: str,
    pub_addr: str,
    reqs: zmq::socket,
    resp: zmq::socket,
};

fn connect(ctx: zmq::context,
          sender_id: str,
          sub_addr: str,
          pub_addr: str) -> connection {
    let reqs =
        alt ctx.socket(zmq::PULL) {
          err(e) { fail e.to_str() }
          ok(reqs) { reqs }
        };
    reqs.connect(sub_addr);

    let resp =
        alt ctx.socket(zmq::PUB) {
          err(e) { fail e.to_str() }
          ok(resp) { resp }
        };
    resp.set_identity(sender_id);
    resp.connect(pub_addr);

    {
        sender_id: sender_id,
        sub_addr: sub_addr,
        pub_addr: pub_addr,
        reqs: reqs,
        resp: resp
    } as connection
}

iface connection {
    fn recv() -> request::t;
    fn send(uuid: str, id: [str], body: [u8]);
    fn reply(req: request::t, body: [u8]);
    fn reply_http(req: request::t,
                  body: [u8],
                  code: uint,
                  status: [u8],
                  headers: hashmap<[u8], [u8]>);
    fn term();
}

impl of connection for connection_t {
    fn recv() -> request::t {
        alt self.reqs.recv(0) {
          err(e) { fail e.to_str() }
          ok(msg) { request::parse(msg) }
        }
    }

    fn send(uuid: str, id: [str], body: [u8]) {
        let id = str::bytes(str::connect(id, " "));
        let msg = vec::connect([
            str::bytes(uuid),
            tnetstring::to_bytes(tnetstring::str(id)),
            body
        ], ' ' as u8);
        alt self.resp.send(msg, 0) {
          err(e) { fail e.to_str() }
          ok(()) { }
        };
    }

    fn reply(req: request::t, body: [u8]) {
        self.send(req.uuid, [req.id], body)
    }

    fn reply_http(req: request::t,
                  body: [u8],
                  code: uint,
                  status: [u8],
                  headers: hashmap<[u8], [u8]>) {
        let rep = [];
        rep += str::bytes(#fmt("HTTP/1.1 %u ", code));
        rep += status;
        rep += str::bytes("\r\n");
        rep += str::bytes("Content-Length: ");
        rep += str::bytes(uint::to_str(vec::len(body), 10u));

        headers.items { |key, value|
            rep += key;
            rep += str::bytes(": ");
            rep += value;
        }
        rep += str::bytes("\r\n\r\n");
        rep += body;

        self.reply(req, rep);
    }

    fn term() {
        self.reqs.close();
        self.resp.close();
    }
}

mod request {
    export t;
    export parse;

    type t = {
        uuid: str,
        id: str,
        path: str,
        headers: hashmap<str, str>,
        body: [u8],
    };

    fn parse(msg: [u8]) -> t {
        let end = vec::len(msg);
        let (start, uuid) = parse_uuid(msg, 0u, end);
        let (start, id) = parse_id(msg, start, end);
        let (start, path) = parse_path(msg, start, end);
        let (headers, body) = parse_rest(msg, start, end);

        { uuid: uuid, id: id, path: path, headers: headers, body: body }
    }

    fn parse_uuid(msg: [u8], start: uint, end: uint) -> (uint, str) {
        alt vec::position_from(msg, start, end) { |c| c == ' ' as u8 } {
            none { fail "invalid sender uuid" }
            some(i) { (i + 1u, str::from_bytes(vec::slice(msg, 0u, i))) }
        }
    }

    fn parse_id(msg: [u8], start: uint, end: uint) -> (uint, str) {
        alt vec::position_from(msg, start, end) { |c| c == ' ' as u8 } {
          none { fail "invalid connection id" }
          some(i) { (i + 1u, str::from_bytes(vec::slice(msg, start, i))) }
        }
    }

    fn parse_path(msg: [u8], start: uint, end: uint) -> (uint, str) {
        alt vec::position_from(msg, start, end) { |c| c == ' ' as u8 } {
          none { fail "invalid path" }
          some(i) { (i + 1u, str::from_bytes(vec::slice(msg, start, i))) }
        }
    }

    fn parse_rest(msg: [u8], start: uint, end: uint)
      -> (hashmap<str, str>, [u8]) {
        let rest = vec::slice(msg, start, end);

        let (headers, rest) = tnetstring::from_bytes(rest);
        let headers = alt headers {
          some(headers) { parse_headers(headers) }
          none { fail "empty headers" }
        };

        let (body, _) = tnetstring::from_bytes(rest);
        let body = alt body {
          some(body) { parse_body(body) }
          none { fail "empty body" }
        };

        (headers, body)
    }

    fn parse_headers(tns: tnetstring::t) -> hashmap<str, str> {
        let headers = map::new_str_hash();
        alt tns {
          tnetstring::map(map) {
            map.items { |key, value|
                alt value {
                  tnetstring::str(v) {
                      let key = str::from_bytes(key);
                      let v = str::from_bytes(v);
                      headers.insert(key, v);
                  }
                  _ { fail "header value is not string"; }
                }
            };
          }

          // Fall back onto json if we got a string.
          tnetstring::str(bytes) {
            alt json::from_str(str::from_bytes(bytes)) {
              err(e) { fail "invalid JSON string"; }
              ok(json::dict(map)) {
                map.items { |key, value|
                    alt value {
                      json::string(v) {
                        headers.insert(key, v);
                      }
                      _ { fail "header value is not string"; }
                    }
                }
              }
              ok(_) { fail "header is not a dictionary"; }
            }
          }

          _ { fail "invalid header"; }
        }

        headers
    }

    fn parse_body(tns: tnetstring::t) -> [u8] {
        alt tns {
          tnetstring::str(body) { body }
          _ { fail "invalid body" }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test() {
        let ctx =
            alt zmq::init(1) {
              ok(ctx) { ctx }
              err(e) { fail e.to_str() }
            };

        let connection = connect(ctx,
            "F0D32575-2ABB-4957-BC8B-12DAC8AFF13A",
            "tcp://127.0.0.1:9998",
            "tcp://127.0.0.1:9999");

        connection.term();
        ctx.term();
    }

    #[test]
    fn test_request_parse() {
        let request = request::parse(
            str::bytes("abCD-123 56 / 13:{\"foo\":\"bar\"},11:hello world,"));

        let headers = map::new_bytes_hash();
        headers.insert(str::bytes("foo"), str::bytes("bar"));

        assert request.uuid == str::bytes("abCD-123");
        assert request.id == str::bytes("56");
        assert request.headers.size() == headers.size();
        request.headers.items() { |k, v| assert v == headers.get(k); }
        assert request.body == str::bytes("hello world");
    }
}
