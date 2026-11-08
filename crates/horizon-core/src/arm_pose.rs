//! `HumanoidModel.ArmPose`: how a humanoid model poses an arm, and the names an item declaration gives them.

/// `HumanoidModel.ArmPose`'s constants, the arm poses a humanoid model has.
// exhaustive: HumanoidModel.ArmPose's constants; NeoForge's enum extensions have no counterpart
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ArmPose {
    Empty,
    Item,
    Block,
    #[serde(rename = "bow_and_arrow")]
    Bow,
    CrossbowHold,
    CrossbowCharge,
    /// Raising a trident to throw it (or to launch with Riptide).
    ThrowTrident,
    /// Playing a goat horn.
    TootHorn,
    /// Looking through a spyglass (`HumanoidModel.ArmPose.SPYGLASS`).
    Spyglass,
    /// A spear held out (`HumanoidModel.ArmPose.SPEAR`), at rest or charging.
    Spear,
    /// A brush being used (`HumanoidModel.ArmPose.BRUSH`).
    Brush,
}

impl ArmPose {
    /// Both hands hold the item (`BOW_AND_ARROW`'s and the crossbow's two).
    pub fn two_handed(self) -> bool {
        matches!(self, Self::Bow | Self::CrossbowHold | Self::CrossbowCharge)
    }

    /// The other hand's pose gives way to this one.
    pub fn affects_offhand(self) -> bool {
        self.two_handed() || self == Self::ThrowTrident
    }
}
