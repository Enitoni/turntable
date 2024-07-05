use rand::{distributions::Alphanumeric, thread_rng, Rng};

pub fn random_string(length: usize) -> String {
    let mut rng = thread_rng();

    std::iter::repeat(())
        .map(|_| rng.sample(Alphanumeric) as char)
        .take(length)
        .collect()
}
