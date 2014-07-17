#![crate_name = "mongrel2"]
#![crate_type = "lib"]

extern crate serialize;
extern crate tnetstring;
extern crate zmq;

use std::collections::HashMap;
use std::io::{IoError, BufReader, MemWriter};
use std::str;

use serialize::json;

#[deriving(Show)]
pub enum Error {
    JsonBodyIsNotADictionary,
    InvalidSenderUuid,
    InvalidConnectionId,
    InvalidPath,
    InvalidHeaders,
    HeaderIsNotADictionary,
    HeaderValueIsNotAString,
    EmptyBody,
    InvalidBody,
    TNetStringError(tnetstring::Error),
    JsonError(json::BuilderError),
    ZmqError(zmq::Error),
    IoError(IoError),
}

pub struct Connection {
    req_addrs: Vec<String>,
    rep_addrs: Vec<String>,
    req: zmq::Socket,
    rep: zmq::Socket,
}

impl Connection {
    pub fn new(
        ctx: &mut zmq::Context,
        sender_id: Option<String>,
        req_addrs: Vec<String>,
        rep_addrs: Vec<String>
    ) -> Result<Connection, Error> {
        let mut req = match ctx.socket(zmq::PULL) {
            Ok(req) => req,
            Err(err) => { return Err(ZmqError(err)); }
        };

        for req_addr in req_addrs.iter() {
            match req.connect(req_addr.as_slice()) {
                Ok(()) => { },
                Err(err) => { return Err(ZmqError(err)); }
            }
        }

        let mut rep = match ctx.socket(zmq::PUB) {
            Ok(rep) => rep,
            Err(err) => { return Err(ZmqError(err)); }
        };

        match sender_id {
            None => { },
            Some(ref sender_id) => {
                match rep.set_identity(sender_id.as_bytes()) {
                    Ok(()) => { },
                    Err(err) => { return Err(ZmqError(err)); }
                }
            }
        }

        for rep_addr in rep_addrs.iter() {
            println!("rep: {}", *rep_addr);
            match rep.connect(rep_addr.as_slice()) {
                Ok(()) => { },
                Err(err) => { return Err(ZmqError(err)); }
            }
        }

        Ok(Connection {
            req_addrs: req_addrs,
            rep_addrs: rep_addrs,
            req: req,
            rep: rep
        })
    }

    pub fn req_addrs<'a>(&'a self) -> &'a [String] {
        self.req_addrs.as_slice()
    }

    pub fn rep_addrs<'a>(&'a self) -> &'a [String] {
        self.rep_addrs.as_slice()
    }

    pub fn recv(&mut self) -> Result<Request, Error> {
        match self.req.recv_msg(0) {
            Err(err) => Err(ZmqError(err)),
            Ok(msg) => parse(msg.as_bytes()),
        }
    }

    pub fn send(&mut self,
            uuid: &str,
            id: &[String],
            body: &[u8]) -> Result<(), Error> {
        let mut wr = MemWriter::new();
        {
            match write!(wr, "{} ", uuid) {
                Ok(()) => { }
                Err(err) => { return Err(IoError(err)); }
            }

            let id = id.connect(" ").into_bytes();
            match tnetstring::to_writer(&mut wr, &tnetstring::Str(id)) {
                Ok(()) => { }
                Err(err) => { return Err(IoError(err)); }
            }

            match write!(wr, " ") {
                Ok(()) => { }
                Err(err) => { return Err(IoError(err)); }
            }

            match wr.write(body) {
                Ok(()) => { }
                Err(err) => { return Err(IoError(err)); }
            }
        }

        println!("fee: ---\n{}---\n", str::from_utf8(wr.get_ref().as_slice()));

        let bytes = wr.unwrap();

        match self.rep.send(bytes.as_slice(), 0) {
            Ok(()) => Ok(()),
            Err(err) => Err(ZmqError(err)),
        }
    }

    pub fn reply(&mut self, req: &Request, body: &[u8]) -> Result<(), Error> {
        self.send(req.uuid.as_slice(), [req.id.clone()], body)
    }

    pub fn reply_http(&mut self,
                  req: &Request,
                  code: uint,
                  status: &str,
                  headers: &Headers,
                  body: &str) -> Result<(), Error> {
        let body_bytes = body.as_bytes();
        let mut wr = MemWriter::new();

        {
            match write!(wr, "HTTP/1.1 {} {}\r\n", code, status) {
                Ok(()) => { }
                Err(err) => { return Err(IoError(err)); }
            }

            for (key, values) in headers.iter() {
                for value in values.iter() {
                    match write!(wr, "{}: {}\r\n", *key, *value) {
                        Ok(()) => { }
                        Err(err) => { return Err(IoError(err)); }
                    }
                };
            }

            match write!(wr, "Content-Length: {}\r\n\r\n", body_bytes.len()) {
                Ok(()) => { }
                Err(err) => { return Err(IoError(err)); }
            }
        }

        let bytes = wr.unwrap();

        self.reply(req, bytes.as_slice())
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        self.req.close().unwrap();
        self.rep.close().unwrap();
    }
}

pub type Headers = HashMap<String, Vec<String>>;

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
                if conn.len() == 1 && conn[0].as_slice() == "close" {
                    return true;
                }
            }
        }

        match self.headers.find(&"VERSION".to_string()) {
            None => false,
            Some(version) => {
                version.len() == 1u && version[0].as_slice() == "HTTP/1.0"
            }
        }
    }
}

fn parse(bytes: &[u8]) -> Result<Request, Error> {
    let mut rdr = BufReader::new(bytes);

    let uuid = match parse_uuid(&mut rdr) {
        Ok(uuid) => uuid,
        Err(e) => return Err(e),
    };

    let id = match parse_id(&mut rdr) {
        Ok(value) => value,
        Err(e) => return Err(e),
    };

    let path = match parse_path(&mut rdr) {
        Ok(value) => value,
        Err(e) => return Err(e),
    };

    let headers = match parse_headers(&mut rdr) {
        Ok(headers) => headers,
        Err(e) => return Err(e),
    };

    let body = match parse_body(&mut rdr) {
        Ok(body) => body,
        Err(e) => return Err(e),
    };

    // Extract out the json body if we have it.
    let json_body = match headers.find(&"METHOD".to_string()) {
        None => None,
        Some(method) => {
            if method.len() == 1 && method[0].as_slice() == "JSON" {
                match json::from_str(str::from_utf8(body.as_slice()).unwrap()) {
                    Ok(json::Object(map)) => Some(map),
                    Ok(_) => {
                        return Err(JsonBodyIsNotADictionary);
                    }
                    Err(err) => {
                        return Err(JsonError(err));
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

fn parse_uuid<R: Reader + Buffer>(rdr: &mut R) -> Result<String, Error> {
    match read_str(rdr) {
        Some(s) => Ok(s),
        None => Err(InvalidSenderUuid),
    }
}

fn parse_id<R: Reader + Buffer>(rdr: &mut R) -> Result<String, Error> {
    match read_str(rdr) {
        Some(s) => Ok(s),
        None => Err(InvalidConnectionId),
    }
}

fn parse_path<R: Reader + Buffer>(rdr: &mut R) -> Result<String, Error> {
    match read_str(rdr) {
        Some(s) => Ok(s),
        None => Err(InvalidPath),
    }
}

fn parse_headers<R: Reader + Buffer>(rdr: &mut R) -> Result<Headers, Error> {
    let tns = match tnetstring::from_reader(rdr) {
        Err(err) => {
            return Err(TNetStringError(err));
        }
        Ok(None) => {
            return Err(InvalidHeaders);
        }
        Ok(Some(tns)) => tns,
    };

    match tns {
        tnetstring::Map(map) => parse_tnetstring_headers(map),

        // Fall back onto json if we got a string.
        tnetstring::Str(bytes) => {
            match json::from_str(str::from_utf8(bytes.as_slice()).unwrap()) {
                Err(err) => Err(JsonError(err)),
                Ok(json::Object(map)) => parse_json_headers(map),
                Ok(_) => Err(HeaderIsNotADictionary),
            }
        }

        _ => Err(InvalidHeaders),
    }
}

fn parse_tnetstring_headers(map: HashMap<Vec<u8>, tnetstring::TNetString>) -> Result<Headers, Error> {
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
                        _ => { return Err(HeaderValueIsNotAString); }
                    }
                }
            },
            _ => { return Err(HeaderValueIsNotAString); }
        }

        headers.insert(key, values);
    }

    Ok(headers)
}

fn parse_json_headers(map: json::Object) -> Result<Headers, Error> {
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
                        _ => { return Err(HeaderValueIsNotAString); }
                    }
                }
            }
            _ => { return Err(HeaderValueIsNotAString); }
        }

        headers.insert(key.clone(), values);
    }

    Ok(headers)
}

fn parse_body<R: Reader + Buffer>(rdr: &mut R) -> Result<Vec<u8>, Error> {
    match tnetstring::from_reader(rdr) {
        Err(err) => Err(TNetStringError(err)),
        Ok(None) => Err(EmptyBody),
        Ok(Some(tns)) => {
            match tns {
                tnetstring::Str(body) => Ok(body),
                _ => Err(InvalidBody),
            }
        }
    }
}
