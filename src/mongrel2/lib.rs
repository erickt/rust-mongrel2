#[link(name = "mongrel2",
       vers = "0.3",
       uuid = "f1bdda2b-0db7-42df-a40e-0decd4d56bb0")];
#[crate_type = "lib"];

extern mod extra;
extern mod zmq; // = "github.com/erickt/rust-zmq";
extern mod tnetstring; // = "github.com/erickt/rust-tnetstring";

use std::hashmap::HashMap;
use std::io::Decorator;
use std::io::mem::{BufReader, MemWriter};
use std::str;
use extra::json;

pub struct Connection {
    sender_id: Option<~str>,
    req_addrs: ~[~str],
    rep_addrs: ~[~str],
    req: zmq::Socket,
    rep: zmq::Socket,
}

pub fn connect(
    ctx: &mut zmq::Context,
    sender_id: Option<~str>,
    req_addrs: ~[~str],
    rep_addrs: ~[~str]
) -> Connection {
    let mut req = match ctx.socket(zmq::PULL) {
        Ok(req) => req,
        Err(e) => fail!(e.to_str()),
    };

    for req_addr in req_addrs.iter() {
        match req.connect(*req_addr) {
          Ok(()) => { },
          Err(e) => fail!(e.to_str()),
        }
    }

    let mut rep = match ctx.socket(zmq::PUB) {
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
        println!("rep: {}", *rep_addr);
        match rep.connect(*rep_addr) {
            Ok(()) => { },
            Err(e) => fail!(e.to_str()),
        }
    }

    rep.send_str("hey", 0);

    Connection {
        sender_id: sender_id,
        req_addrs: req_addrs,
        rep_addrs: rep_addrs,
        req: req,
        rep: rep
    }
}

impl Connection {
    pub fn req_addrs<'a>(&'a self) -> &'a [~str] {
        self.req_addrs.as_slice()
    }

    pub fn rep_addrs<'a>(&'a self) -> &'a [~str] {
        self.rep_addrs.as_slice()
    }

    pub fn recv(&mut self) -> Result<Request, ~str> {
        match self.req.recv_msg(0) {
            Err(e) => Err(e.to_str()),
            Ok(msg) => msg.with_bytes(|bytes| parse(bytes)),
        }
    }

    pub fn send(&mut self,
            uuid: &str,
            id: &[~str],
            body: &[u8]) -> Result<(), ~str> {
        let mut wr = MemWriter::new();
        {
            let wr = &mut wr as &mut Writer;

            write!(wr, "{} ", uuid);
            let id = id.connect(" ").into_bytes();
            tnetstring::to_writer(wr, &tnetstring::Str(id));
            write!(wr, " ");
            wr.write(body);
        }

        println!("fee: ---\n{}---\n", str::from_utf8(wr.inner_ref().as_slice()));

        match self.rep.send(wr.inner(), 0) {
          Err(e) => Err(e.to_str()),
          Ok(()) => Ok(()),
        }
    }

    pub fn reply(&mut self, req: &Request, body: &[u8]) -> Result<(), ~str> {
        self.send(req.uuid, [req.id.clone()], body)
    }

    pub fn reply_http(&mut self,
                  req: &Request,
                  code: uint,
                  status: &str,
                  headers: &Headers,
                  body: &str) -> Result<(), ~str> {
        let body_bytes = body.as_bytes();
        let mut wr = MemWriter::new();

        {
            let wr = &mut wr as &mut Writer;
            write!(wr, "HTTP/1.1 {} {}\r\n", code, status);
            for (key, values) in headers.iter() {
                for value in values.iter() {
                    write!(wr, "{}: {}\r\n", *key, *value);
                };
            }
            write!(wr, "Content-Length: {}\r\n\r\n", body_bytes.len());
            wr.write(body_bytes);
        }

        self.reply(req, wr.inner())
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        self.req.close();
        self.rep.close();
    }
}

type Headers = HashMap<~str, ~[~str]>;

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
        match self.json_body {
            None => false,
            Some(ref map) => {
                match map.find(&~"type") {
                    Some(&json::String(ref typ)) => *typ == ~"disconnect",
                    _ => false,
                }
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
    let mut rdr = BufReader::new(bytes);
    parse_reader(&mut rdr)
}

fn parse_reader<R: Reader + Buffer>(rdr: &mut R) -> Result<Request, ~str> {
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
            match json::from_str(str::from_utf8(body)) {
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

fn read_str<R: Reader + Buffer>(rdr: &mut R) -> Option<~str> {
    let mut s = ~"";

    loop {
        match rdr.read_char() {
            Some(' ') => {
                return Some(s);
            }
            Some(ch) => {
                s.push_char(ch);
            }
            None => {
                return None;
            }
        }
    }
}

fn parse_uuid<R: Reader + Buffer>(rdr: &mut R) -> Result<~str, ~str> {
    match read_str(rdr) {
        Some(s) => Ok(s),
        None => Err(~"invalid sender uuid"),
    }
}

fn parse_id<R: Reader + Buffer>(rdr: &mut R) -> Result<~str, ~str> {
    match read_str(rdr) {
        Some(s) => Ok(s),
        None => Err(~"invalid connection id"),
    }
}

fn parse_path<R: Reader + Buffer>(rdr: &mut R) -> Result<~str, ~str> {
    match read_str(rdr) {
        Some(s) => Ok(s),
        None => Err(~"invalid path"),
    }
}

fn parse_headers<R: Reader + Buffer>(rdr: &mut R) -> Result<Headers, ~str> {
    let tns = match tnetstring::from_reader(rdr) {
        None => return Err(~"empty headers"),
        Some(tns) => tns,
    };

    match tns {
        tnetstring::Map(map) => parse_tnetstring_headers(map),

        // Fall back onto json if we got a string.
        tnetstring::Str(bytes) => {
            match json::from_str(str::from_utf8(bytes)) {
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

    for (key, value) in map.move_iter() {
        let key = str::from_utf8_owned(key);
        let mut values = match headers.pop(&key) {
            Some(values) => values,
            None => ~[],
        };

        match value {
            tnetstring::Str(v) => values.push(str::from_utf8_owned(v)),
            tnetstring::Vec(vs) => {
                for v in vs.move_iter() {
                    match v {
                        tnetstring::Str(v) =>
                            values.push(str::from_utf8_owned(v)),
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

fn parse_body<R: Reader + Buffer>(rdr: &mut R) -> Result<~[u8], ~str> {
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
