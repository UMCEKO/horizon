//! Vanilla point-of-interest classification for saved block states.

/// A vanilla point-of-interest identity, independent of villager professions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PoiType {
    Armorer,
    Butcher,
    Cartographer,
    Cleric,
    Farmer,
    Fisherman,
    Fletcher,
    Leatherworker,
    Librarian,
    Mason,
    Shepherd,
    Toolsmith,
    Weaponsmith,
    Meeting,
    Home,
    Beehive,
    BeeNest,
    NetherPortal,
    Lodestone,
    TestInstance,
    LightningRod,
}

impl PoiType {
    /// Saved registry identifier.
    pub fn id(self) -> &'static str {
        match self {
            Self::Armorer => "minecraft:armorer",
            Self::Butcher => "minecraft:butcher",
            Self::Cartographer => "minecraft:cartographer",
            Self::Cleric => "minecraft:cleric",
            Self::Farmer => "minecraft:farmer",
            Self::Fisherman => "minecraft:fisherman",
            Self::Fletcher => "minecraft:fletcher",
            Self::Leatherworker => "minecraft:leatherworker",
            Self::Librarian => "minecraft:librarian",
            Self::Mason => "minecraft:mason",
            Self::Shepherd => "minecraft:shepherd",
            Self::Toolsmith => "minecraft:toolsmith",
            Self::Weaponsmith => "minecraft:weaponsmith",
            Self::Meeting => "minecraft:meeting",
            Self::Home => "minecraft:home",
            Self::Beehive => "minecraft:beehive",
            Self::BeeNest => "minecraft:bee_nest",
            Self::NetherPortal => "minecraft:nether_portal",
            Self::Lodestone => "minecraft:lodestone",
            Self::TestInstance => "minecraft:test_instance",
            Self::LightningRod => "minecraft:lightning_rod",
        }
    }
    /// The compact ordinal of a vanilla village job site, if this is one.
    pub fn job_index(self) -> Option<usize> {
        self.is_job().then_some(self as usize)
    }

    /// Whether this is one of the vanilla village job sites.
    pub fn is_job(self) -> bool {
        matches!(
            self,
            Self::Armorer
                | Self::Butcher
                | Self::Cartographer
                | Self::Cleric
                | Self::Farmer
                | Self::Fisherman
                | Self::Fletcher
                | Self::Leatherworker
                | Self::Librarian
                | Self::Mason
                | Self::Shepherd
                | Self::Toolsmith
                | Self::Weaponsmith
        )
    }
    /// `PoiType.maxTickets`.
    pub fn max_tickets(self) -> usize {
        match self {
            Self::Meeting => 32,
            Self::Beehive
            | Self::BeeNest
            | Self::NetherPortal
            | Self::Lodestone
            | Self::TestInstance
            | Self::LightningRod => 0,
            _ => 1,
        }
    }
    /// `PoiType.validRange` for the village POIs.
    pub fn valid_range(self) -> i32 {
        if self == Self::Meeting { 6 } else { 1 }
    }
}

/// `PoiTypes.register`: saved POI type of a block state.
pub fn saved_poi(state: &crate::chunk::BlockState) -> Option<PoiType> {
    use PoiType::*;
    let name = state.name.strip_prefix("minecraft:")?;
    Some(match name {
        "blast_furnace" => Armorer,
        "smoker" => Butcher,
        "cartography_table" => Cartographer,
        "brewing_stand" => Cleric,
        "composter" => Farmer,
        "barrel" => Fisherman,
        "fletching_table" => Fletcher,
        "cauldron" | "lava_cauldron" | "water_cauldron" | "powder_snow_cauldron" => Leatherworker,
        "lectern" => Librarian,
        "stonecutter" => Mason,
        "loom" => Shepherd,
        "smithing_table" => Toolsmith,
        "grindstone" => Weaponsmith,
        "bell" => Meeting,
        "beehive" => Beehive,
        "bee_nest" => BeeNest,
        "nether_portal" => NetherPortal,
        "lodestone" => Lodestone,
        "test_instance_block" => TestInstance,
        bed if bed.ends_with("_bed") && bed != "straw_bed" => {
            if state.properties.get("part").map(String::as_str) != Some("head") {
                return None;
            }
            Home
        }
        rod if rod.ends_with("lightning_rod") => LightningRod,
        _ => return None,
    })
}

/// Saved POI type and maximum tickets, for save and locate consumers.
pub fn poi_type(state: &crate::chunk::BlockState) -> Option<(&'static str, i32)> {
    saved_poi(state).map(|poi| (poi.id(), poi.max_tickets() as i32))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunk::BlockState;

    #[test]
    fn saved_poi_types_include_all_cauldron_states() {
        for name in [
            "cauldron",
            "water_cauldron",
            "lava_cauldron",
            "powder_snow_cauldron",
        ] {
            let state = BlockState {
                name: format!("minecraft:{name}"),
                properties: Default::default(),
            };
            assert_eq!(poi_type(&state), Some(("minecraft:leatherworker", 1)));
        }
    }
}
