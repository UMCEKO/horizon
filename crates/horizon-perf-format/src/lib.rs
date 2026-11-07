pub mod horizon {
    pub mod perf {
        pub mod v1 {
            include!(concat!(env!("OUT_DIR"), "/horizon.perf.v1.rs"));
        }
    }
}

pub use horizon::perf::v1::*;

pub mod counter;
pub mod gzip;
pub mod health;
pub mod writer;

pub use counter::{CounterHandle, CounterRegistry};
pub use gzip::GzipWriter;
pub use health::{HealthRecorder, HealthTier};
pub use writer::PerfWriter;
