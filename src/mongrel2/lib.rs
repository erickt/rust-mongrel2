#![crate_name = "mongrel2"]
#![crate_type = "lib"]

extern crate serialize;
extern crate tnetstring;
extern crate zmq;

use std::collections::HashMap;
use std::io::{BufReader, MemWriter};
use std::str;
use serialize::json;

pub struct Connection {
    sender_id: Option<String>,
    req_addrs: Vec<String>,
    rep_addrs: Vec<String>,
    req: zmq::Socket,
    rep: zmq::Socket,
}

pub fn connect(
    ctx: &mut zmq::Context,
    sender_id: Option<String>,
    req_addrs: Vec<String>,
    rep_addrs: Vec<String>
) -> Connection {
    let mut req = match ctx.socket(zmq::PULL) {
        Ok(req) => req,
        Err(e) => fail!(e.to_string()),
    };

    for req_addr in req_addrs.iter() {
        match req.connect(req_addr.as_slice()) {
          Ok(()) => { },
          Err(e) => fail!(e.to_string()),
        }
    }

    let mut rep = match ctx.socket(zmq::PUB) {
        Ok(rep) => rep,
        Err(e) => fail!(e.to_string()),
    };

    match sender_id {
        None => { },
        Some(ref sender_id) => {
            match rep.set_identity(sender_id.as_bytes()) {
                Ok(()) => { },
                Err(e) => fail!(e.to_string()),
            }
        }
    }

    for rep_addr in rep_addrs.iter() {
        println!("rep: {}", *rep_addr);
        match rep.connect(rep_addr.as_slice()) {
            Ok(()) => { },
            Err(e) => fail!(e.to_string()),
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
    pub fn req_addrs<'a>(&'a self) -> &'a [String] {
        self.req_addrs.as_slice()
    }

    pub fn rep_addrs<'a>(&'a self) -> &'a [String] {
        self.rep_addrs.as_slice()
    }

    pub fn recv(&mut self) -> Result<Request, String> {
        match self.req.recv_msg(0) {
            Err(e) => Err(e.to_string()),
            Ok(msg) => msg.with_bytes(|bytes| parse(bytes)),
        }
    }

    pub fn send(&mut self,
            uuid: &str,
            id: &[String],
            body: &[u8]) -> Result<(), String> {
        let mut wr = MemWriter::new();
        {
            let wr = &mut wr as &mut Writer;

            write!(wr, "{} ", uuid);
            let id = id.connect(" ").into_bytes();
            tnetstring::to_writer(wr, &tnetstring::Str(id));
            write!(wr, " ");
            wr.write(body);
        }

        println!("fee: ---\n{}---\n", str::from_utf8(wr.get_ref().as_slice()));

        match self.rep.send(wr.unwrap().as_slice(), 0) {
          Err(e) => Err(e.to_string()),
          Ok(()) => Ok(()),
        }
    }

    pub fn reply(&mut self, req: &Request, body: &[u8]) -> Result<(), String> {
        self.send(req.uuid.as_slice(), [req.id.clone()], body)
    }

    pub fn reply_http(&mut self,
                  req: &Request,
                  code: uint,
                  status: &str,
                  headers: &Headers,
                  body: &str) -> Result<(), String> {
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

        self.reply(req, wr.unwrap().as_slice())
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        self.req.close().unwrap();
        self.rep.close().unwrap();
    }
}

type Headers = HashMap<String, Vec<String>>;

#[deriving(Clone)]
pub struct Request {
    uuid: String,
    id: String,
    path: String,
    headers: Headers,
    body: Vec<u8>,
    json_body: Option<json::Object>,
}

impl Request {
    pub fn is_disconnect(&self) -> bool {
        match self.json_body {
            None => false,
            Some(ref map) => {
                match map.find(&"type".to_string()) {
                    Some(&json::String(ref typ)) => *typ == "disconnect".to_string(),
                    _ => false,
                }
            }
        }
    }

    pub fn should_close(&self) -> bool {
        match self.headers.find(&"connection".to_string()) {
            None => { },
            Some(conn) => {
                if conn.len() == 1 && conn.get(0).as_slice() == "close" {
                    return true;
                }
            }
        }

        match self.headers.find(&"VERSION".to_string()) {
            None => false,
            Some(version) => {
                version.len() == 1u && version.get(0).as_slice() == "HTTP/1.0"
            }
        }
    }
}

fn parse(bytes: &[u8]) -> Result<Request, String> {
    let mut rdr = BufReader::new(bytes);
    parse_reader(&mut rdr)
}

fn parse_reader<R: Reader + Buffer>(rdr: &mut R) -> Result<Request, String> {
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
    let json_body = match headers.find(&"METHOD".to_string()) {
        None => None,
        Some(method) => {
            if method.len() == 1 && method.get(0).as_slice() == "JSON" {
                match json::from_str(str::from_utf8(body.as_slice()).unwrap()) {
                    Ok(json::Object(map)) => Some(map),
                    Ok(_) => {
                        return Err("json body is not a dictionary".to_string());
                    }
                    Err(e) => {
                        return Err(format!("invalid JSON string: {}", e.to_string()))
                    }
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

fn read_str<R: Reader + Buffer>(rdr: &mut R) -> Option<String> {
    let mut s = "".to_string();

    loop {
        match rdr.read_char() {
            Ok(' ') => {
                return Some(s);
            }
            Ok(ch) => {
                s.push_char(ch);
            }
            Err(_) => {
                return None;
            }
        }
    }
}

fn parse_uuid<R: Reader + Buffer>(rdr: &mut R) -> Result<String, String> {
    match read_str(rdr) {
        Some(s) => Ok(s),
        None => Err("invalid sender uuid".to_string()),
    }
}

fn parse_id<R: Reader + Buffer>(rdr: &mut R) -> Result<String, String> {
    match read_str(rdr) {
        Some(s) => Ok(s),
        None => Err("invalid connection id".to_string()),
    }
}

fn parse_path<R: Reader + Buffer>(rdr: &mut R) -> Result<String, String> {
    match read_str(rdr) {
        Some(s) => Ok(s),
        None => Err("invalid path".to_string()),
    }
}

fn parse_headers<R: Reader + Buffer>(rdr: &mut R) -> Result<Headers, String> {
    let tns = match tnetstring::from_reader(rdr) {
        Err(err) => {
            return Err(err.to_string());
        }
        Ok(None) => {
            return Err("empty headers".to_string());
        }
        Ok(Some(tns)) => tns,
    };

    match tns {
        tnetstring::Map(map) => parse_tnetstring_headers(map),

        // Fall back onto json if we got a string.
        tnetstring::Str(bytes) => {
            match json::from_str(str::from_utf8(bytes.as_slice()).unwrap()) {
                Err(e) => return Err(e.to_string()),
                Ok(json::Object(map)) => parse_json_headers(map),
                Ok(_) => Err("header is not a dictionary".to_string()),
            }
        }

        _ => Err("invalid header".to_string()),
    }
}

fn parse_tnetstring_headers(map: HashMap<Vec<u8>, tnetstring::TNetString>) -> Result<Headers, String> {
    let mut headers: HashMap<String, Vec<String>> = HashMap::new();

    for (key, value) in map.move_iter() {
        let key = String::from_utf8(key).unwrap();

        let mut values = match headers.pop(&key) {
            Some(values) => values,
            None => vec!(),
        };

        match value {
            tnetstring::Str(v) => {
                values.push(String::from_utf8(v).unwrap());
            }
            tnetstring::Vec(vs) => {
                for v in vs.move_iter() {
                    match v {
                        tnetstring::Str(v) => {
                            values.push(String::from_utf8(v).unwrap());
                        }
                        _ => return Err("header value is not a string".to_string()),
                    }
                }
            },
            _ => return Err("header value is not string".to_string()),
        }

        headers.insert(key, values);
    }

    Ok(headers)
}

fn parse_json_headers(map: json::Object) -> Result<Headers, String> {
    let mut headers = HashMap::new();

    for (key, value) in map.iter() {
        let mut values = match headers.pop(key) {
            Some(values) => values,
            None => vec!(),
        };

        match value {
            &json::String(ref v) => values.push(v.clone()),
            &json::List(ref vs) => {
                for v in vs.iter() {
                    match v {
                        &json::String(ref v) => values.push(v.clone()),
                        _ => return Err("header value is not a string".to_string()),
                    }
                }
            }
            _ => return Err("header value is not string".to_string()),
        }

        headers.insert(key.clone(), values);
    }

    Ok(headers)
}

fn parse_body<R: Reader + Buffer>(rdr: &mut R) -> Result<Vec<u8>, String> {
    match tnetstring::from_reader(rdr) {
        Err(err) => Err(err.to_string()),
        Ok(None) => Err("empty body".to_string()),
        Ok(Some(tns)) => {
            match tns {
                tnetstring::Str(body) => Ok(body),
                _ => Err("invalid body".to_string()),
            }
        }
    }
}
