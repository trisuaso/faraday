use pathbufd::PathBufD;
use rand::{Rng, distributions::Alphanumeric, thread_rng};
use std::{env::temp_dir, fs::write};

pub fn random() -> String {
    thread_rng()
        .sample_iter(&Alphanumeric)
        .take(16)
        .map(char::from)
        .collect()
}

/// Create a temporary file and return the path.
pub fn create() -> PathBufD {
    let tempdir = temp_dir();
    let path = PathBufD::from(tempdir.into()).join(random());

    if let Err(e) = write(&path, "") {
        panic!("{e}");
    }

    path
}
