use std::{sync::Arc, thread, time::Duration};

mod audio;
mod http;
mod ingest;
mod logging;
mod util;

fn main() {
    logging::init_logger();

    let audio = Arc::new(audio::AudioSystem::default());
    audio.start();

    let inputs: Vec<_> = [
        "https://www.youtube.com/watch?v=TSfmEij_pcQ&pp=ygUQZGVzc2VydCBtb3VudGFpbg%3D%3D",
        "https://www.youtube.com/watch?v=Al2injKg6pk&pp=ygUMY2FycmllZCBhd2F5",
        "https://www.youtube.com/watch?v=CHFjD0Pb96E&pp=ygULbmlnaHQgYmlyZHM%3D",
        "https://www.youtube.com/watch?v=81iUUn91kbE&pp=ygUJbW9vbiBteW9u",
        "https://www.youtube.com/watch?v=3V5INDi8XVs&pp=ygUQc3lua3JvIGxvc3QgaGVyZQ%3D%3D",
        "https://www.youtube.com/watch?v=dP1bwxi9pz0&pp=ygUTMnhtIGZsb2F0aW5nIGRlc2lyZQ%3D%3D",
    ]
    .into_iter()
    .filter_map(ingest::Input::parse)
    .collect();

    for input in inputs {
        audio.add(input);
    }

    thread::spawn({
        let http_audio = Arc::clone(&audio);

        move || http::run(http_audio)
    });

    // Run forever
    loop {
        let time_to_sleep = Duration::from_secs(60);
        thread::sleep(time_to_sleep);
    }
}
