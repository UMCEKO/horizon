#[derive(Debug, Default, Clone)]
pub struct HealthRecorder {
    // Tiers and slow ring land in F2.
}

impl HealthRecorder {
    pub fn new() -> Self {
        Self::default()
    }
}
