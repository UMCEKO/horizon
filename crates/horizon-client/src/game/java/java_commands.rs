// game-module-cfg: feature java-mods
//! Which root commands the Java server runs, or holds Java's nodes in, as it told at the server's start.

use super::*;
use horizon_java_host::wire::In;
use std::cell::RefCell;
use std::collections::HashMap;

/// A root command Java runs, or whose tree has Java's nodes in it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum JavaRoot {
    Java,
    Mixed,
}

#[derive(Default)]
pub(super) struct JavaCommands {
    roots: HashMap<String, JavaRoot>,
    order: Vec<String>,
}

// Read by the command specs that follow (W6 SPEC-2..4).
#[allow(dead_code)]
impl JavaCommands {
    /// `commands_roots`' reply: a count, then each root's name and kind.
    pub(super) fn read(reply: &[u8]) -> Option<Self> {
        let mut input = In(reply);
        let count = usize::try_from(input.int()?).ok()?;
        let mut commands = Self::default();
        for _ in 0..count {
            let name = input.text()?.to_owned();
            let kind = match input.byte()? {
                1 => JavaRoot::Java,
                2 => JavaRoot::Mixed,
                _ => return None,
            };
            if commands.roots.insert(name.clone(), kind).is_none() {
                commands.order.push(name);
            }
        }
        Some(commands)
    }

    /// The first word of a command line, its slash already stripped.
    pub(super) fn root(&self, line: &str) -> Option<JavaRoot> {
        let word = line.split_ascii_whitespace().next()?;
        self.roots.get(word).copied()
    }

    pub(super) fn names(&self) -> &[String] {
        &self.order
    }
}

thread_local! {
    static JAVA_COMMANDS: RefCell<JavaCommands> = RefCell::default();
}

#[allow(dead_code)]
pub(super) fn java_commands<T>(f: impl FnOnce(&JavaCommands) -> T) -> T {
    JAVA_COMMANDS.with_borrow(f)
}

pub(super) fn java_commands_reset() {
    JAVA_COMMANDS.with_borrow_mut(|commands| *commands = JavaCommands::default());
}

impl Game<'_> {
    /// Asks the Java server which root commands are its own, once per server start.
    pub(super) fn java_read_commands(&mut self) {
        let table = self
            .java_request("commands_roots", &[])
            .and_then(|reply| JavaCommands::read(&reply));
        if table.is_none() {
            eprintln!("Horizon: the Java server's command roots are unknown");
        }
        JAVA_COMMANDS.with_borrow_mut(|commands| *commands = table.unwrap_or_default());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use horizon_java_host::wire::Out;

    #[test]
    fn a_reply_names_the_java_roots() {
        let mut out = Out::default();
        out.int(2);
        out.text("mek");
        out.0.push(1);
        out.text("enchant");
        out.0.push(2);
        let commands = JavaCommands::read(&out.0).unwrap();
        assert_eq!(commands.root("mek give x"), Some(JavaRoot::Java));
        assert_eq!(commands.root("enchant @s"), Some(JavaRoot::Mixed));
        assert_eq!(commands.root("give"), None);
        assert_eq!(commands.names(), ["mek", "enchant"]);
    }

    #[test]
    fn a_short_reply_is_unknown() {
        let mut out = Out::default();
        out.int(2);
        out.text("mek");
        out.0.push(1);
        assert!(JavaCommands::read(&out.0).is_none());
    }
}
