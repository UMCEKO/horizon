// game-module-cfg: feature java-mods
//! Java crossings from inside a level cascade, where no `&mut Game` exists: the level reads ari's `LevelHost` gives native listeners, nothing else.

use super::*;
use horizon_java_host::bridge::LevelOn;
use horizon_java_host::wire::Out;

/// The running cascade as Java's bridges see it inside a crossing: the level a native listener reads, and its dimension.
pub(crate) struct GameCascade<'a> {
    level: &'a dyn mod_api::LevelHost,
    dim: Dim,
}

impl GameCascade<'_> {
    /// The cascade's level when Java called on it; reads of another level aren't served.
    pub(super) fn level(&self, on: &LevelOn) -> Option<&dyn mod_api::LevelHost> {
        let named = match on {
            LevelOn::Overworld => Dim::Overworld,
            LevelOn::Nether => Dim::Nether,
            LevelOn::End => Dim::End,
            LevelOn::Named(id) => Dim::extra(id)?,
        };
        (named == self.dim).then_some(self.level)
    }
}

/// Posts `event` to Java's level listeners from inside a cascade on `level` (`post_cascade_event`): whether one canceled it.
pub(super) fn post(
    level: &dyn mod_api::LevelHost,
    dimension: LevelDimension,
    event: &'static str,
    fields: impl FnOnce(&mut Out),
) -> Option<bool> {
    let host = horizon_java_host::get()?;
    let dim = Dim::from_id(dimension.key().id());
    let mut cx = GameCascade { level, dim };
    horizon_java_host::bridge::with_cascade(&mut cx, event, || {
        super::java_mods::register_level(host, dim);
        host.call_with(
            "post_cascade_event",
            |out| {
                out.int(dim.key().slot() as i32);
                out.text(event);
                fields(out);
            },
            |reply| super::java_mods::java_read_posted(reply).map(|(canceled, _)| canceled),
        )
    })
}

/// `sides`, a bit per `Direction` ordinal: all six but `skipped`.
pub(super) fn sides_but(skipped: Option<horizon_core::geometry::Direction>) -> u8 {
    use horizon_core::geometry::Direction::{Down, East, North, South, Up, West};
    let all = 0b11_1111;
    let Some(skipped) = skipped else { return all };
    let ordinal = [Down, Up, North, South, West, East]
        .iter()
        .position(|side| *side == skipped)
        .unwrap_or(6);
    all & !(1 << ordinal)
}

#[cfg(test)]
mod tests {
    use super::sides_but;
    use horizon_core::geometry::Direction;

    #[test]
    fn the_sides_name_every_direction_but_the_skipped_one_by_ordinal() {
        assert_eq!(sides_but(None), 0b11_1111);
        assert_eq!(sides_but(Some(Direction::Down)), 0b11_1110);
        assert_eq!(sides_but(Some(Direction::Up)), 0b11_1101);
        assert_eq!(sides_but(Some(Direction::East)), 0b01_1111);
    }
}
