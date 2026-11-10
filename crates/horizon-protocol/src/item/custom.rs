//! Wire IDs for data components past vanilla's (Java mods' then native ones), installed once before networking.
use super::{Component, Stack, Template, component};
use crate::buf::{Codec, Reader};
use std::sync::OnceLock;

struct Types {
    builtin: Vec<String>,
    first: i32,
    names: Vec<String>,
}

static TYPES: OnceLock<Types> = OnceLock::new();

#[derive(Debug, PartialEq, Eq)]
pub enum InstallError {
    InvalidRange,
    DuplicateName,
    InvalidName,
    AlreadyInstalled,
}

impl std::fmt::Display for InstallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidRange => "invalid component ID range",
            Self::DuplicateName => "duplicate component name",
            Self::InvalidName => "invalid component name",
            Self::AlreadyInstalled => "component types already installed",
        })
    }
}
impl std::error::Error for InstallError {}

impl Types {
    fn new(builtin: Vec<String>, mut names: Vec<String>) -> Result<Self, InstallError> {
        let first = i32::try_from(builtin.len()).map_err(|_| InstallError::InvalidRange)?;
        if i32::try_from(names.len())
            .ok()
            .and_then(|n| first.checked_add(n))
            .is_none()
        {
            return Err(InstallError::InvalidRange);
        }
        if names.iter().any(|name| {
            horizon_core::identifier::Identifier::parse_namespaced(name)
                .is_none_or(|id| id.namespace() == "minecraft")
        }) {
            return Err(InstallError::InvalidName);
        }
        names.sort();
        if names.windows(2).any(|pair| pair[0] == pair[1])
            || names.iter().any(|name| builtin.contains(name))
        {
            return Err(InstallError::DuplicateName);
        }
        Ok(Self {
            builtin,
            first,
            names,
        })
    }

    fn name(&self, id: i32) -> Option<&str> {
        let index = usize::try_from(id.checked_sub(self.first)?).ok()?;
        self.names.get(index).map(String::as_str)
    }

    fn id(&self, name: &str) -> Option<i32> {
        let index = self.names.binary_search_by(|n| n.as_str().cmp(name)).ok()?;
        self.first.checked_add(i32::try_from(index).ok()?)
    }

    fn builtin(&self, id: i32) -> Option<&str> {
        self.builtin
            .get(usize::try_from(id).ok()?)
            .map(String::as_str)
    }
}

/// Install the merged built-in registry (vanilla's types, then Java mods' in Java's order) and sorted native names after it.
#[doc(hidden)]
pub fn install(builtin: Vec<String>, names: Vec<String>) -> Result<(), InstallError> {
    TYPES
        .set(Types::new(builtin, names)?)
        .map_err(|_| InstallError::AlreadyInstalled)
}

/// A built-in type's name, vanilla's or a Java mod's; never a native one.
pub(crate) fn builtin(id: i32) -> Option<&'static str> {
    TYPES.get()?.builtin(id)
}

/// Validate an opaque value against the same limits used by the network reader.
pub fn validate(value: &horizon_core::nbt::Tag) -> crate::Result<()> {
    if matches!(value, horizon_core::nbt::Tag::End) {
        return Err(crate::Error::new("a component value cannot be NBT End"));
    }
    crate::buf::check_network_nbt(value)?;
    // Disk NBT canonicalizes NaNs, and its reader normalizes negative zero.
    // Reject values that would change when saved and reloaded.
    let limits = horizon_core::nbt::Limits {
        max_bytes: crate::buf::NBT_QUOTA + 2,
        ..Default::default()
    };
    let named = horizon_core::nbt::NamedTag {
        name: "".into(),
        tag: value.clone(),
    };
    let saved = horizon_core::nbt::write_named(&named, limits)
        .map_err(|error| crate::Error::new(error.to_string()))?;
    let (loaded, _) = horizon_core::nbt::read_named(&saved, limits)
        .map_err(|error| crate::Error::new(error.to_string()))?;
    if loaded.tag != *value {
        return Err(crate::Error::new(
            "component value changes on save and reload",
        ));
    }
    Ok(())
}

pub(crate) fn contains(id: i32) -> bool {
    name(id).is_some()
}

/// Check a decoded stack before converting it on an unverified connection.
#[doc(hidden)]
pub fn permitted(stack: &Stack, negotiated: bool) -> crate::Result<bool> {
    if negotiated || entries().next().is_none() {
        return Ok(true);
    }
    fn contains_custom(stack: &Stack) -> crate::Result<bool> {
        if stack.removed.iter().any(|id| contains(*id)) {
            return Ok(true);
        }
        for (id, value) in &stack.added {
            if contains(*id) {
                return Ok(true);
            }
            match value {
                Component::Stacks(stacks) => {
                    for stack in stacks {
                        if contains_custom(stack)? {
                            return Ok(true);
                        }
                    }
                }
                Component::TooltipDisplay { hidden, .. }
                    if hidden.iter().any(|id| contains(*id)) =>
                {
                    return Ok(true);
                }
                Component::Raw(bytes)
                    if matches!(
                        *id,
                        component::USE_REMAINDER
                            | component::POT_DECORATIONS
                            | component::CONTAINER
                            | component::SULFUR_CUBE_CONTENT
                    ) =>
                {
                    let mut reader = Reader::new(bytes);
                    let count = match *id {
                        component::USE_REMAINDER | component::SULFUR_CUBE_CONTENT => 1,
                        component::POT_DECORATIONS => 4,
                        _ => reader.count(256)?,
                    };
                    for _ in 0..count {
                        if (matches!(
                            *id,
                            component::USE_REMAINDER | component::SULFUR_CUBE_CONTENT
                        ) || reader.bool()?)
                            && contains_custom(&Template::read(&mut reader)?)?
                        {
                            return Ok(true);
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(false)
    }
    Ok(!contains_custom(stack)?)
}

#[doc(hidden)]
pub fn name(id: i32) -> Option<&'static str> {
    TYPES.get()?.name(id)
}

#[doc(hidden)]
pub fn id(name: &str) -> Option<i32> {
    TYPES.get()?.id(name)
}

#[doc(hidden)]
pub fn entries() -> impl Iterator<Item = (i32, &'static str)> {
    TYPES.get().into_iter().flat_map(|types| {
        types.names.iter().enumerate().filter_map(|(index, name)| {
            Some((
                types.first.checked_add(i32::try_from(index).ok()?)?,
                name.as_str(),
            ))
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn builtin(java: &[&str]) -> Vec<String> {
        let vanilla = (0..component::VANILLA_TYPES).map(|i| format!("minecraft:t{i}"));
        vanilla
            .chain(java.iter().map(|s| (*s).to_owned()))
            .collect()
    }

    #[test]
    fn a_native_named_components_id_stays_stable_after_javas() {
        let native = || vec!["sample:z".to_owned(), "sample:a".to_owned()];
        let alone = Types::new(builtin(&[]), native()).unwrap();
        assert_eq!(alone.id("sample:a"), Some(122));
        assert_eq!(alone.id("sample:z"), Some(123));
        let java = || builtin(&["gem:mode", "gem:other"]);
        let with_java = Types::new(java(), native()).unwrap();
        assert_eq!(with_java.builtin(122), Some("gem:mode"));
        assert_eq!(with_java.builtin(123), Some("gem:other"));
        assert_eq!(with_java.name(123), None);
        assert_eq!(with_java.id("sample:a"), Some(124));
        assert_eq!(with_java.id("sample:z"), Some(125));
        assert_eq!(with_java.name(125), Some("sample:z"));
        assert_eq!(with_java.builtin(124), None);
        let reordered = vec!["sample:a".to_owned(), "sample:z".to_owned()];
        let again = Types::new(java(), reordered).unwrap();
        assert_eq!(again.id("sample:z"), Some(125));
    }

    #[test]
    fn a_native_name_cannot_take_a_java_mods_type() {
        assert_eq!(
            Types::new(builtin(&["gem:mode"]), vec!["gem:mode".into()]).err(),
            Some(InstallError::DuplicateName)
        );
    }
}
