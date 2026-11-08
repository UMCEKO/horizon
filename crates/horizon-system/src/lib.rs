//! Probes for the machine the game runs on, read locally and never sent anywhere (`docs/telemetry.md`).
//! The benchmark report and the crash report describe the machine with them; [`scrub`] takes
//! the player's name, world and addresses out of text they attach by hand.
pub mod scrub;
pub mod system;
