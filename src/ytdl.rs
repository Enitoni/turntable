use youtube_dl::{YoutubeDl, YoutubeDlOutput};

pub async fn get_audio_url(source: &str) -> Option<String> {
    let output = YoutubeDl::new(source)
        .socket_timeout("15")
        .extra_arg("-f")
        .extra_arg("bestaudio")
        .run();

    output
        .ok()
        .and_then(|o| match o {
            YoutubeDlOutput::SingleVideo(video) => Some(video),
            YoutubeDlOutput::Playlist(_) => None,
        })
        .and_then(|video| {
            let format_name = video.format.as_ref();

            video.formats.and_then(|formats| {
                formats
                    .into_iter()
                    .find(|f| f.format.as_ref() == format_name)
            })
        })
        .and_then(|format| format.url)
}
