use rand::Rng;
use rand::distributions::Alphanumeric;

pub fn random_string(len: usize) -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(len)
        .map(char::from)
        .collect()
}

pub fn random_port() -> u16 {
    rand::thread_rng().gen_range(1024..=62000)
}
