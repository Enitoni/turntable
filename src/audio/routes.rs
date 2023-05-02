use warp::{Filter, Rejection, Reply};

use crate::{server::with_state, VinylContext};

use super::Input;

fn add_input(context: VinylContext, query: String) -> impl Reply {
    match Input::parse(&query) {
        Some(input) => {
            let name = input.to_string();
            let response = format!("Added {} to the queue", name);

            context.audio.add(input);

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
        .and(with_state(context))
        .and(warp::body::json())
        .map(add_input);

    warp::path("audio").and(input)
}
