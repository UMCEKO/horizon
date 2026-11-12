// game-module-cfg: feature java-mods
//! The F3 entries Java mods registered (`RegisterDebugEntriesEvent`): listed and switched on native's debug screen, their lines recorded by the Java client for the entries native has enabled.

use super::*;
use horizon_client::debug_entries::{self, JavaEntry, Status, category_of_label};
use horizon_client::debug_overlay::Recorded;
use horizon_java_host::wire::In;
use std::cell::{Cell, RefCell};

thread_local! {
    /// The enabled entries' calls as the Java client's last tick recorded them.
    static RECORDED: RefCell<Vec<Recorded>> = RefCell::default();
    /// The last `DebugLines` input named entries.
    static SENT: Cell<bool> = const { Cell::new(false) };
    static TAKEN: Cell<bool> = const { Cell::new(false) };
}

pub(super) fn recorded() -> Vec<Recorded> {
    RECORDED.with_borrow(Clone::clone)
}

/// Whether this tick's `DebugLines` input is sent: while entries are wanted, and once when they stop.
pub(super) fn should_send(wanted: &[String]) -> bool {
    SENT.replace(!wanted.is_empty()) || !wanted.is_empty()
}

/// 0 absent from the profile, then `ALWAYS_ON`, `IN_OVERLAY`, `NEVER`.
fn status(byte: u8) -> Option<Option<Status>> {
    Some(match byte {
        0 => None,
        1 => Some(Status::AlwaysOn),
        2 => Some(Status::InOverlay),
        3 => Some(Status::Never),
        _ => return None,
    })
}

fn read_entries(reply: &[u8]) -> Option<Vec<JavaEntry>> {
    let mut reply = In(reply);
    let entries = (0..reply.int()?)
        .map(|_| {
            let id = reply.text()?.to_owned();
            let label = reply.text()?;
            // A mod's own category has no section of its own on native's screen yet.
            let category = category_of_label(label);
            let _sort_key = reply.float()?;
            let reduced = reply.bool()?;
            let default = status(reply.byte()?)?;
            let performance = status(reply.byte()?)?;
            Some(JavaEntry {
                id,
                category,
                reduced,
                default,
                performance,
            })
        })
        .collect::<Option<Vec<_>>>()?;
    reply.0.is_empty().then_some(entries)
}

fn read_lines(reply: &mut In) -> Option<Vec<String>> {
    (0..reply.int()?)
        .map(|_| reply.text().map(str::to_owned))
        .collect()
}

fn read_recorded(reply: &mut In) -> Option<Vec<Recorded>> {
    (0..reply.int()?)
        .map(|_| {
            let id = reply.text()?.to_owned();
            let priority = read_lines(reply)?;
            let lines = read_lines(reply)?;
            let groups = (0..reply.int()?)
                .map(|_| Some((reply.text()?.to_owned(), read_lines(reply)?)))
                .collect::<Option<Vec<_>>>()?;
            Some(Recorded {
                id,
                priority,
                lines,
                groups,
            })
        })
        .collect()
}

/// A tick status's `debugLines` block: what the wanted entries recorded.
pub(super) fn java_debug_status(reply: &mut In) -> Option<()> {
    let recorded = read_recorded(reply)?;
    RECORDED.with_borrow_mut(|held| *held = recorded);
    Some(())
}

impl Game<'_> {
    /// The mods' F3 entries, asked once of the Java client as the window takes the boot's other results.
    pub(in crate::game) fn java_debug_entries_take(&mut self) {
        if TAKEN.replace(true) {
            return;
        }
        let Some(host) = horizon_java_host::get() else {
            return;
        };
        let Some(entries) = host
            .request("client_debug_entries", &[])
            .and_then(|reply| read_entries(&reply))
        else {
            log::error!("Horizon: the Java client's F3 entries did not answer");
            return;
        };
        if entries.is_empty() {
            return;
        }
        debug_entries::set_java_entries(entries);
        self.debug_entries.java_entries_set();
    }

    /// The Java entries native has enabled now, for the Java client to record.
    pub(in crate::game) fn java_debug_wanted(&self) -> (bool, Vec<String>) {
        let reduced = self.reduced_debug_info();
        let wanted = debug_entries::java_entries()
            .iter()
            .filter(|e| self.debug_entries.enabled(&e.id, self.debug, reduced))
            .map(|e| e.id.clone())
            .collect();
        (reduced, wanted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use horizon_java_host::wire::Out;

    #[test]
    fn an_export_reads_its_entries_and_refuses_a_short_one() {
        let mut out = Out::default();
        out.int(1);
        out.text("appleskin:food_stats");
        out.text(r#"{"translate":"debug.options.category.text"}"#);
        out.float(1.0);
        out.bool(false);
        out.0.extend([2, 0]);
        let entries = read_entries(&out.0).unwrap();
        assert_eq!(
            entries,
            [JavaEntry {
                id: "appleskin:food_stats".into(),
                category: horizon_client::debug_entries::Category::Text,
                reduced: false,
                default: Some(Status::InOverlay),
                performance: None,
            }]
        );
        assert_eq!(read_entries(&out.0[..out.0.len() - 1]), None);
    }

    #[test]
    fn a_status_block_reads_each_entrys_calls_in_order() {
        let mut out = Out::default();
        out.int(1);
        out.text("horizon_java_test:probe");
        out.int(0);
        out.int(1);
        out.text("probe");
        out.int(1);
        out.text("horizon_java_test:g");
        out.int(1);
        out.text("g1");
        let recorded = read_recorded(&mut In(&out.0)).unwrap();
        assert_eq!(
            recorded,
            [Recorded {
                id: "horizon_java_test:probe".into(),
                priority: Vec::new(),
                lines: vec!["probe".into()],
                groups: vec![("horizon_java_test:g".into(), vec!["g1".into()])],
            }]
        );
    }

    #[test]
    fn the_input_is_sent_while_wanted_and_once_when_it_stops() {
        let ids = vec!["a:b".to_owned()];
        assert!(should_send(&ids));
        assert!(should_send(&ids));
        assert!(should_send(&[]));
        assert!(!should_send(&[]));
    }
}
