use std::path::PathBuf;

fn main() {
    let proto = "proto/horizon/perf/v1/capture.proto";
    println!("cargo:rerun-if-changed={proto}");
    let fds = protox::compile([proto], ["proto"]).expect("compile proto");
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let mut config = prost_build::Config::new();
    config.out_dir(out_dir);
    config.compile_fds(fds).expect("generate prost types");
}
