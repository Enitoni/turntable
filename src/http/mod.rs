use std::{env, str::FromStr, sync::Arc, thread};

use tiny_http::{Header, Response, Server, StatusCode};

use crate::audio::{AudioSystem, WaveStream};

pub fn run(audio: Arc<AudioSystem>) {
    let port: u16 = env::var("GCT_HTTP_PORT")
        .expect("GCT_HTTP_PORT was not provided")
        .parse()
        .expect("GCT_HTTP_PORT must be a number");

    let addr = format!("127.0.0.1:{}", port);
    let server = Server::http(addr).unwrap();

    println!("Running HTTP server on port {}!", port);

    for req in server.incoming_requests() {
        let audio = Arc::clone(&audio);
        let addr = req.remote_addr().to_string();

        thread::spawn(move || {
            println!("Web stream connection opened for {}", &addr);
            let stream = WaveStream::new(audio.stream());
            let mut res = Response::new(StatusCode(200), vec![], stream, None, None);

            res.add_header(
                Header::from_str(format!("Content-Type: {}", WaveStream::MIME).as_str()).unwrap(),
            );

            let _ = req.respond(res);

            println!("Web stream connection closed for {}", &addr);
        });
    }
}
