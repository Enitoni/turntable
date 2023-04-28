use std::{env, str::FromStr, sync::Arc, thread};

use log::info;
use tiny_http::{Header, Response, Server, StatusCode};

use crate::audio::{AudioSystem, WaveStream};

pub mod stream;

const DEFAULT_PORT: u16 = 9050;

pub fn run(audio: Arc<AudioSystem>) {
    let port: u16 = env::var("VINYL_HTTP_PORT")
        .unwrap_or(DEFAULT_PORT.to_string())
        .parse()
        .expect("VINYL_HTTP_PORT must be a number");

    let addr = format!("127.0.0.1:{}", port);
    let server = Server::http(addr).unwrap();

    info!("Server listening on port {}", port);

    for req in server.incoming_requests() {
        let audio = Arc::clone(&audio);
        let addr = req.remote_addr().to_string();

        thread::spawn(move || {
            info!("Audio stream connection opened for {}", &addr);

            let stream = audio.stream();

            let stream = WaveStream::new(stream);
            let mut res = Response::new(StatusCode(200), vec![], stream, None, None);

            res.add_header(
                Header::from_str(format!("Content-Type: {}", WaveStream::MIME).as_str()).unwrap(),
            );

            let _ = req.respond(res);

            info!("Audio stream connection closed for {}", &addr);
        });
    }
}
