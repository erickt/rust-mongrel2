#[link(name = "mongrel2",
       vers = "0.3",
       uuid = "f1bdda2b-0db7-42df-a40e-0decd4d56bb0")];
#[crate_type = "lib"];

extern mod extra;
extern mod zmq = "github.com/erickt/rust-zmq";
extern mod tnetstring = "github.com/erickt/rust-tnetstring";

use std::hashmap::HashMap;
use std::{cast, io, str, uint};
use extra::json;
use extra::json::ToStr;

pub struct Connection {
    sender_id: Option<~str>,
    req_addrs: @~[~str],
    rep_addrs: @~[~str],
    req: zmq::Socket,
    rep: zmq::Socket,
}

pub fn connect(
    ctx: zmq::Context,
    sender_id: Option<~str>,
    req_addrs: ~[~str],
    rep_addrs: ~[~str]
) -> Connection {
    let req = match ctx.socket(zmq::PULL) {
        Ok(req) => req,
        Err(e) => fail!(e.to_str()),
    };

    for req_addr in req_addrs.iter() {
        match req.connect(*req_addr) {
          Ok(()) => { },
          Err(e) => fail!(e.to_str()),
        }
    }

    let rep = match ctx.socket(zmq::PUB) {
        Ok(rep) => rep,
        Err(e) => fail!(e.to_str()),
    };

    match sender_id {
        None => { },
        Some(ref sender_id) => {
            match rep.set_identity(sender_id.as_bytes()) {
                Ok(()) => { },
                Err(e) => fail!(e.to_str()),
            }
        }
    }

    for rep_addr in rep_addrs.iter() {
        match rep.connect(*rep_addr) {
            Ok(()) => { },
            Err(e) => fail!(e.to_str()),
        }
    }

    Connection {
        sender_id: sender_id,
        req_addrs: @req_addrs,
        rep_addrs: @rep_addrs,
        req: req,
        rep: rep
    }
}

impl Connection {
    fn req_addrs(&self) -> @~[~str] { self.req_addrs }
    fn rep_addrs(&self) -> @~[~str] { self.rep_addrs }

    pub fn recv(&self) -> Result<Request, ~str> {
        match unsafe { self.req.recv(0) } {
            Err(e) => Err(e.to_str()),
            Ok(msg) => msg.with_bytes(|bytes| parse(bytes)),
        }
    }

    pub fn send(&self,
            uuid: &str,
            id: &[~str],
            body: &[u8]) -> Result<(), ~str> {
        let id = str_as_bytes(id.connect(" "));

        let mut msg = ~[];

        msg.push_all(uuid.as_bytes());
        msg.push(' ' as u8);
        msg.push_all(tnetstring::to_bytes(&tnetstring::Str(id)));
        msg.push(' ' as u8);
        msg.push_all(body);

        match self.rep.send(msg, 0) {
          Err(e) => Err(e.to_str()),
          Ok(()) => Ok(()),
        }
    }

    pub fn reply(&self, req: &Request, body: &[u8]) -> Result<(), ~str> {
        //self.send(req.uuid, [copy req.id], body)
        self.send(req.uuid, [req.id.clone()], body)
    }

    pub fn reply_http(&self,
                  req: &Request,
                  code: uint,
                  status: &str,
                  headers: Headers,
                  body: ~str) -> Result<(), ~str> {
        let mut rep = ~[];

        rep.push_all(str_as_bytes(format!("HTTP/1.1 {} ", code)));
        rep.push_all(status.as_bytes());
        rep.push_all("\r\n".as_bytes());
        rep.push_all("Content-Length: ".as_bytes());
        rep.push_all(str_as_bytes(uint::to_str(body.len())));
        rep.push_all("\r\n".as_bytes());

        for (key, values) in headers.iter() {
            for value in values.iter() {
                rep.push_all(str_as_bytes(*key + ": " + *value + "\r\n"));
            };
        }
        rep.push_all("\r\n".as_bytes());
        rep.push_all(str_as_bytes(body));

        self.reply(req, rep)
    }

    pub fn term (&mut self) {
        self.req.close();
        self.rep.close();
    }
}

// TODO: there is no `as_bytes' for ~str that will return ~[u8].
fn str_as_bytes(s: ~str) -> ~[u8] {
    let s = s.clone();
    let mut buf: ~[u8] = unsafe { cast::transmute(s) };
    buf.pop();
    buf
}

pub type Headers = HashMap<~str, ~[~str]>;

pub fn Headers() -> Headers {
    HashMap::new()
}

#[deriving(Clone)]
pub struct Request {
    uuid: ~str,
    id: ~str,
    path: ~str,
    headers: Headers,
    body: ~[u8],
    json_body: Option<~json::Object>,
}

impl Request {
    pub fn is_disconnect(&self) -> bool {
        do self.json_body.map_default(false) |map| {
            match map.find(&~"type") {
              Some(&json::String(ref typ)) => *typ == ~"disconnect",
              _ => false,
            }
        }
    }

    pub fn should_close(&self) -> bool {
        match self.headers.find(&~"connection") {
          None => { },
          Some(conn) => {
            if conn.len() == 1u && conn[0u] == ~"close" { return true; }
          }
        }

        match self.headers.find(&~"VERSION") {
          None => false,
          Some(version) => {
            version.len() == 1u && version[0u] == ~"HTTP/1.0"
          }
        }
    }
}

fn parse(bytes: &[u8]) -> Result<Request, ~str> {
    io::with_bytes_reader(bytes, parse_reader)
}

fn parse_reader(rdr: @io::Reader) -> Result<Request, ~str> {
    let uuid = match parse_uuid(rdr) {
        Ok(uuid) => uuid,
        Err(e) => return Err(e),
    };

    let id = match parse_id(rdr) {
        Ok(value) => value,
        Err(e) => return Err(e),
    };

    let path = match parse_path(rdr) {
        Ok(value) => value,
        Err(e) => return Err(e),
    };

    let headers = match parse_headers(rdr) {
        Ok(headers) => headers,
        Err(e) => return Err(e),
    };

    let body = match parse_body(rdr) {
        Ok(body) => body,
        Err(e) => return Err(e),
    };

    // Extract out the json body if we have it.
    let json_body = match headers.find(&~"METHOD") {
      None => None,
      Some(method) => {
        if method.len() == 1u && method[0u] == ~"JSON" {
            match json::from_str(str::from_bytes(body)) {
              Ok(json::Object(map)) => Some(map),
              Ok(_) => return Err(~"json body is not a dictionary"),
              Err(e) =>
                return Err(format!("invalid JSON string: {}", e.to_str())),
            }
        } else { None }
      }
    };

    Ok(Request {
        uuid: uuid,
        id: id,
        path: path,
        headers: headers,
        body: body,
        json_body: json_body
    })
}

fn read_str(rdr: @io::Reader) -> Option<~str> {
    let mut s = ~"";

    while !rdr.eof() {
        let ch = rdr.read_char();
        if ch == ' ' {
            return Some(s);
        } else {
            s.push_char(ch);
        }
    }

    None
}

fn parse_uuid(rdr: @io::Reader) -> Result<~str, ~str> {
    match read_str(rdr) {
        Some(s) => Ok(s),
        None => Err(~"invalid sender uuid"),
    }
}

fn parse_id(rdr: @io::Reader) -> Result<~str, ~str> {
    match read_str(rdr) {
        Some(s) => Ok(s),
        None => Err(~"invalid connection id"),
    }
}

fn parse_path(rdr: @io::Reader) -> Result<~str, ~str> {
    match read_str(rdr) {
        Some(s) => Ok(s),
        None => Err(~"invalid path"),
    }
}

fn parse_headers(rdr: @io::Reader) -> Result<Headers, ~str> {
    let tns = match tnetstring::from_reader(rdr) {
        None => return Err(~"empty headers"),
        Some(tns) => tns,
    };

    match tns {
        tnetstring::Map(map) => parse_tnetstring_headers(map),

        // Fall back onto json if we got a string.
        tnetstring::Str(bytes) => {
            match json::from_str(str::from_bytes(bytes)) {
                Err(e) => return Err(e.to_str()),
                Ok(json::Object(map)) => parse_json_headers(map),
                Ok(_) => Err(~"header is not a dictionary"),
            }
        }

        _ => Err(~"invalid header"),
    }
}

fn parse_tnetstring_headers(map: tnetstring::Map) -> Result<Headers, ~str> {
    let mut headers = HashMap::new();

    for (key, value) in map.iter() {
        let key = str::from_bytes(*key);
        let mut values = match headers.pop(&key) {
            Some(values) => values,
            None => ~[],
        };

        match value {
            &tnetstring::Str(ref v) => values.push(str::from_bytes(*v)),
            &tnetstring::Vec(ref vs) => {
                for v in vs.iter() {
                    match v {
                        &tnetstring::Str(ref v) =>
                            values.push(str::from_bytes(*v)),
                        _ => return Err(~"header value is not a string"),
                    }
                }
            },
            _ => return Err(~"header value is not string"),
        }

        headers.insert(key, values);
    }

    Ok(headers)
}

fn parse_json_headers(map: ~json::Object) -> Result<Headers, ~str> {
    let mut headers = HashMap::new();

    for (key, value) in map.iter() {
        let mut values = match headers.pop(key) {
            Some(values) => values,
            None => ~[],
        };

        match value {
            &json::String(ref v) => values.push(v.clone()),
            &json::List(ref vs) => {
                for v in vs.iter() {
                    match v {
                        &json::String(ref v) => values.push(v.clone()),
                        _ => return Err(~"header value is not a string"),
                    }
                }
            }
            _ => return Err(~"header value is not string"),
        }

        headers.insert(key.clone(), values);
    }

    Ok(headers)
}

fn parse_body(rdr: @io::Reader) -> Result<~[u8], ~str> {
    match tnetstring::from_reader(rdr) {
        None => Err(~"empty body"),
        Some(tns) => {
            match tns {
                tnetstring::Str(body) => Ok(body),
                _ => Err(~"invalid body"),
            }
        }
    }
}

#[test]
fn test() {
    let ctx = zmq::init(1).unwrap();

    let mut connection = connect(ctx,
        Some(~"F0D32575-2ABB-4957-BC8B-12DAC8AFF13A"),
        ~[~"tcp://127.0.0.1:9998"],
        ~[~"tcp://127.0.0.1:9999"]);

    connection.term();
    ctx.term();
}

#[test]
fn test_request_parse() {
    let request = parse(
        str::to_bytes("abCD-123 56 / 13:{\"foo\":\"bar\"},11:hello world,")
    ).unwrap();

    assert!(request.uuid == ~"abCD-123");
    assert!(request.id == ~"56");
    assert!(request.headers.len() == 1u);
    let value = match request.headers.find(&~"foo") {
        Some(header_list) => header_list[0u].clone(),
        None => ~"",
    };
    assert!(value == ~"bar");
    assert!(request.body == str::to_bytes("hello world"));
}
