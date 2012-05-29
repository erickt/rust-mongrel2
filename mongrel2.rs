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
    sub_addrs: [str],
    pub_addrs: [str],
    reqs: zmq::socket,
    resp: zmq::socket,
};

fn connect(ctx: zmq::context,
          sender_id: str,
          sub_addrs: [str],
          pub_addrs: [str]) -> connection {
    let reqs =
        alt ctx.socket(zmq::PULL) {
          err(e) { fail e.to_str() }
          ok(reqs) { reqs }
        };

    sub_addrs.iter { |sub_addr| reqs.connect(sub_addr); }

    let resp =
        alt ctx.socket(zmq::PUB) {
          err(e) { fail e.to_str() }
          ok(resp) { resp }
        };
    resp.set_identity(sender_id);

    pub_addrs.iter { |pub_addr| resp.connect(pub_addr); }

    {
        sender_id: sender_id,
        sub_addrs: sub_addrs,
        pub_addrs: pub_addrs,
        reqs: reqs,
        resp: resp
    } as connection
}

iface connection {
    fn sub_addrs() -> [str];
    fn pub_addrs() -> [str];
    fn recv() -> @request;
    fn send(uuid: str, id: [str], body: [u8]);
    fn reply(req: @request, body: [u8]);
    fn reply_http(req: @request,
                  code: uint,
                  status: str,
                  headers: hashmap<str, [str]>,
                  body: [u8]);
    fn term();
}

impl of connection for connection_t {
    fn sub_addrs() -> [str] { self.sub_addrs }
    fn pub_addrs() -> [str] { self.pub_addrs }

    fn recv() -> @request {
        alt self.reqs.recv(0) {
          err(e) { fail e.to_str() }
          ok(msg) { parse(msg) }
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

    fn reply(req: @request, body: [u8]) {
        self.send(req.uuid, [req.id], body)
    }

    fn reply_http(req: @request,
                  code: uint,
                  status: str,
                  headers: hashmap<str, [str]>,
                  body: [u8]) {
        let mut rep = [];
        rep += str::bytes(#fmt("HTTP/1.1 %u ", code));
        rep += str::bytes(status);
        rep += str::bytes("\r\n");
        rep += str::bytes("Content-Length: ");
        rep += str::bytes(uint::to_str(vec::len(body), 10u));
        rep += str::bytes("\r\n");

        for headers.each { |key, values|
            let lines = vec::map(values) { |value|
                str::bytes(key + ": " + value + "\r\n")
            };

            rep += vec::concat(lines);
        }
        rep += str::bytes("\r\n");
        rep += body;

        self.reply(req, rep);
    }

    fn term() {
        self.reqs.close();
        self.resp.close();
    }
}

type request = {
    uuid: str,
    id: str,
    path: str,
    headers: hashmap<str, [str]>,
    body: [u8],
    json_body: option<hashmap<str, json::json>>,
};

impl request for @request {
    fn is_disconnect() -> bool {
        self.json_body.map_default(false) { |map|
            alt map.find("type") {
              some(json::string(typ)) { typ == "disconnect" }
              _ { false }
            }
        }
    }

    fn should_close() -> bool {
        alt self.headers.find("connection") {
          none { }
          some(conn) { if conn == ["close"] { ret true; } }
        }

        alt self.headers.find("VERSION") {
          none { false }
          some(version) { version == ["HTTP/1.0"] }
        }
    }
}

fn parse(msg: [u8]) -> @request {
    let end = vec::len(msg);
    let (start, uuid) = parse_uuid(msg, 0u, end);
    let (start, id) = parse_id(msg, start, end);
    let (start, path) = parse_path(msg, start, end);
    let (headers, body) = parse_rest(msg, start, end);

    // Extract out the json body if we have it.
    let json_body = headers.find("METHOD").chain { |method|
        if method == ["JSON"] {
          alt json::from_str(str::from_bytes(body)) {
            ok(json::dict(map)) { some(map) }
            ok(_) { fail "json body is not a dictionary" }
            err(e) { fail #fmt("invalid JSON string: %s", e.msg); }
          }
        } else { none }
    };

    @{
        uuid: uuid,
        id: id,
        path: path,
        headers: headers,
        body: body,
        json_body: json_body
    }
}

fn parse_uuid(msg: [u8], start: uint, end: uint) -> (uint, str) {
    alt vec::position_between(msg, start, end) { |c| c == ' ' as u8 } {
        none { fail "invalid sender uuid" }
        some(i) { (i + 1u, str::from_bytes(vec::slice(msg, 0u, i))) }
    }
}

fn parse_id(msg: [u8], start: uint, end: uint) -> (uint, str) {
    alt vec::position_between(msg, start, end) { |c| c == ' ' as u8 } {
      none { fail "invalid connection id" }
      some(i) { (i + 1u, str::from_bytes(vec::slice(msg, start, i))) }
    }
}

fn parse_path(msg: [u8], start: uint, end: uint) -> (uint, str) {
    alt vec::position_between(msg, start, end) { |c| c == ' ' as u8 } {
      none { fail "invalid path" }
      some(i) { (i + 1u, str::from_bytes(vec::slice(msg, start, i))) }
    }
}

fn parse_rest(msg: [u8], start: uint, end: uint)
  -> (hashmap<str, [str]>, [u8]) {
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

fn parse_headers(tns: tnetstring::t) -> hashmap<str, [str]> {
    let headers = map::str_hash();
    alt tns {
      tnetstring::map(map) {
        for map.each { |key, value|
            let key = str::from_bytes(key);
            let values = alt headers.find(key) {
              none { [] }
              some(values) { values }
            };

            let vs = alt value {
              tnetstring::str(v) { [str::from_bytes(v)] }
              tnetstring::vec(vs) {
                vec::map(vs) { |v|
                    alt v {
                      tnetstring::str(v) { str::from_bytes(v) }
                      _ { fail "header value is not a string"; }
                    }
                }
              }
              _ { fail "header value is not string"; }
            };

            headers.insert(key, values + vs);
        }
      }

      // Fall back onto json if we got a string.
      tnetstring::str(bytes) {
        alt json::from_str(str::from_bytes(bytes)) {
          err(e) { fail "invalid JSON string"; }
          ok(json::dict(map)) {
            for map.each { |key, value|
                let values = alt headers.find(key) {
                  none { [] }
                  some(values) { values }
                };

                let vs = alt value {
                  json::string(v) { [v] }
                  json::list(vs) {
                    vec::map(vs) { |v|
                        alt v {
                          json::string(v) { v }
                          _ { fail "header value is not a string"; }
                        }
                    }
                  }
                  _ { fail "header value is not string"; }
                };

                headers.insert(key, values + vs);
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
        let request = parse(
            str::bytes("abCD-123 56 / 13:{\"foo\":\"bar\"},11:hello world,"));

        let headers = map::str_hash();
        headers.insert("foo", ["bar"]);

        assert request.uuid == "abCD-123";
        assert request.id == "56";
        assert request.headers.size() == headers.size();
        for request.headers.each { |k, v| assert v == headers.get(k); }
        assert request.body == str::bytes("hello world");
    }
}
