pub mod horizon {
    pub mod perf {
        pub mod v1 {
            include!(concat!(env!("OUT_DIR"), "/horizon.perf.v1.rs"));
        }
    }
}

pub use horizon::perf::v1::*;

// Writer module placeholder
pub mod writer;
pub mod gzip;
pub mod counter;
pub mod health;
