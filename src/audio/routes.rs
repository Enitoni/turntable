use hyper::Response;
use tokio::task::spawn_blocking;
use warp::{Filter, Rejection, Reply};

use crate::{server::with_state, VinylContext};

use super::{Input, WaveStream};

fn get_stream(context: VinylContext) -> impl Reply {
    let stream = context.audio.stream();
    let stream = WaveStream::new(stream);

    let body = hyper::Body::wrap_stream(stream);

    Response::builder()
        .status(200)
        .header("Content-Type", WaveStream::MIME)
        .body(body)
        .unwrap()
}

async fn add_input(context: VinylContext, query: String) -> impl Reply {
    match Input::parse(&query) {
        Some(input) => {
            let name = input.to_string();
            let response = format!("Added {} to the queue", name);

            let _ = spawn_blocking(move || context.audio.add(input)).await;

            warp::reply::with_status(warp::reply::json(&response), warp::http::StatusCode::OK)
        }
        None => {
            let response = "Invalid input";

            warp::reply::with_status(
                warp::reply::json(&response),
                warp::http::StatusCode::BAD_REQUEST,
            )
        }
    }
}

pub fn routes(
    context: VinylContext,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone {
    let input = warp::path("input")
        .and(warp::post())
        .and(with_state(context.clone()))
        .and(warp::body::json())
        .then(add_input);

    let stream = warp::path("stream")
        .and(warp::get())
        .and(with_state(context))
        .map(get_stream);

    warp::path("audio").and(input).or(stream)
}
