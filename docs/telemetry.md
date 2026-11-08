# Horizon sends nothing

Horizon's launcher and game send nothing about you or your machine to any server: no performance, crash,
bug or benchmark uploads, no "I'm playing" ping, no install ID, no telemetry endpoint. There is no consent
question because there is nothing to consent to. This is fork policy and differs from upstream.

What stays on your machine, as files you can read and share by hand:

- **Crash reports**: `crash-reports/` in the game folder ([crash reports](native-client.md)).
- **Benchmark reports**: `benchmarks/` in the game folder and in the launcher's data folder, in the
  [benchmark report format](benchmark-report.md).
- **Bug reports**: `horizon-launcher bug-report "what happened" [--log]` writes
  `bug-reports/bug-report-<time>.txt` in the game folder: your text, the machine's OS and hardware, and with
  `--log` the end of the game's log, with account names, UUIDs, world names, e-mail and IP addresses hidden.
  Read it, then attach it to your issue yourself.

The machine probes (`crates/horizon-system`) read cores, clocks, drive type, memory, power and form locally
for those reports. The crate has no network dependency; a test checks it.

The only network traffic the launcher and game make is what you ask for: downloads, sign-in, the server list's
status pings when you open it, and the servers you join.
