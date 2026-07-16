pub mod bench;
pub mod cluster;
pub mod export;
pub mod model;
pub mod runtime;
pub mod storage;

pub fn install_crypto_provider() {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
}
