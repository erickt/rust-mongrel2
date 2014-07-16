extern mod mongrel2;
extern mod zmq;

use std::io::timer;
use std::hashmap::HashMap;

#[test]
fn test() {
    let mut ctx = zmq::Context::new();

    let req_address = ~"inproc://requests";
    let rep_address = ~"inproc://responses";

    /*
    let req_address = ~"tcp://127.0.0.1:9000";
    let rep_address = ~"tcp://127.0.0.1:9001";
    */

    /*
    let mut req_socket = ctx.socket(zmq::PUSH).unwrap();
    req_socket.bind(req_address).unwrap();
    */


    let mut rep_socket = ctx.socket(zmq::SUB).unwrap();
    let mut foo_socket = ctx.socket(zmq::PUB).unwrap();

    rep_socket.bind(rep_address).unwrap();
    rep_socket.set_subscribe([]).unwrap();

    foo_socket.connect(rep_address).unwrap();

    timer::sleep(1000);

    foo_socket.send_str("hey", 0).unwrap();
    foo_socket.send_str("hey", 0).unwrap();

    timer::sleep(1000);

    println!("before");

    let mut i = 0;
    loop {
        i += 1;
        foo_socket.send_str(format!("hey {}", i), 0).unwrap();
        let response = match rep_socket.recv_str(zmq::DONTWAIT) {
            Ok(response) => response,
            //Err(e) => { fail!(e.to_str()); }
            Err(e) => { println!("e: {}", e.to_str()); continue; }
        };
        println!("{}", i);
        println!("received: {}", response);
        break;
    }

    /*
    let mut connection = mongrel2::connect(&mut ctx,
        Some(~"F0D32575-2ABB-4957-BC8B-12DAC8AFF13A"),
        ~[req_address],
        ~[rep_address, ~"tcp://127.0.0.1:9002"]);


    match req_socket.send_str("abCD-123 56 / 13:{\"foo\":\"bar\"},11:hello world,", 0) {
        Ok(()) => { }
        Err(e) => { fail!(e.to_str()); }
    }

    let request = match connection.recv() {
        Ok(request) => request,
        Err(e) => { fail!(e); }
    };

    assert!(request.uuid == ~"abCD-123");
    assert!(request.id == ~"56");
    assert!(request.headers.len() == 1u);
    let value = match request.headers.find(&~"foo") {
        Some(header_list) => header_list[0u].clone(),
        None => ~"",
    };
    assert!(value == ~"bar");
    assert!(request.body == (~"hello world").into_bytes());

    let headers = HashMap::new();

    match connection.reply_http(&request, 200, "OK", &headers, "hey there") {
        Ok(()) => { }
        Err(e) => { fail!(e.to_str()); }
    }

    let response = match rep_socket.recv_str(0) {
        Ok(response) => response,
        Err(e) => { fail!(e.to_str()); }
    };

    println!("received: {}", response);
    */
}
