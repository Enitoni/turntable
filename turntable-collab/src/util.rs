use lazy_static::lazy_static;
use rand::{distributions::Alphanumeric, thread_rng, Rng};
use regex::Regex;

lazy_static! {
    pub static ref URL_SCHEME_REGEX: Regex = Regex::new(r"^(https?://)?").unwrap();
}

pub fn random_string(length: usize) -> String {
    let mut rng = thread_rng();

    std::iter::repeat(())
        .map(|_| rng.sample(Alphanumeric) as char)
        .take(length)
        .collect()
}
