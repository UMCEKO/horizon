//! The locator bar for remote players: ServerWaypointManager and
//! WaypointTransmitter's connections, for players (the transmitters vanilla has
//! by default: `waypoint_transmit_range` is 0 for every other entity).
//!
//! Vanilla updates a player's connections each time the player's position
//! changes (`Entity.setPosRaw`, then `ServerChunkCache.move`). The hub sees the
//! position once a tick, after the moves it handled, and updates from there:
//! the same packets, since a client sends one move a tick. Hiding (crouching,
//! invisibility, a carved pumpkin or a mob head worn) changes
//! `waypoint_transmit_range` and tracks or untracks the waypoint, as
//! `LivingEntity.onAttributeUpdated` does; joining or leaving a team remakes the
//! connections with the team's colour (`ServerScoreboard.updatePlayerWaypoint`).
use crate::fast_hash::FxHashMap as HashMap;
use horizon_protocol::packets::play::{
    Clientbound as C, TrackedWaypoint, WaypointId, WaypointTarget, waypoint_operation,
};

/// `Player.createAttributes`: both waypoint ranges.
pub const PLAYER_RANGE: f64 = 6.0e7;
/// `WaypointTransmitter.REALLY_FAR_DISTANCE`.
const REALLY_FAR: f32 = 332.0;
/// `EntityAzimuthConnection.update`: half a degree.
const AZIMUTH_STEP: f32 = 0.008_726_646;

/// Items that hide their wearer (`Waypoint.addHideAttribute` on the head slot).
pub const HIDING_HEADS: [&str; 8] = [
    "minecraft:carved_pumpkin",
    "minecraft:skeleton_skull",
    "minecraft:wither_skeleton_skull",
    "minecraft:player_head",
    "minecraft:zombie_head",
    "minecraft:creeper_head",
    "minecraft:dragon_head",
    "minecraft:piglin_head",
];

/// A team colour as the locator bar draws it (`Waypoint.Icon.cloneAndAssignStyle`:
/// `TeamColor.rgb()`, black lightened so it shows).
pub fn team_rgb(color: &str) -> Option<i32> {
    let rgb = match color {
        "black" => return Some(-13_619_152),
        "dark_blue" => 0x0000AA,
        "dark_green" => 0x00AA00,
        "dark_aqua" => 0x00AAAA,
        "dark_red" => 0xAA0000,
        "dark_purple" => 0xAA00AA,
        "gold" => 0xFFAA00,
        "gray" => 0xAAAAAA,
        "dark_gray" => 0x555555,
        "blue" => 0x5555FF,
        "green" => 0x55FF55,
        "aqua" => 0x55FFFF,
        "red" => 0xFF5555,
        "light_purple" => 0xFF55FF,
        "yellow" => 0xFFFF55,
        "white" => 0xFFFFFF,
        _ => return None,
    };
    Some(rgb)
}

/// Waypoint.Icon.
#[derive(Clone, Debug, PartialEq)]
pub struct Icon {
    pub style: String,
    pub color: Option<i32>,
}

impl Default for Icon {
    fn default() -> Self {
        Self {
            style: "minecraft:default".into(),
            color: None,
        }
    }
}

/// A player as the waypoint manager sees them this tick.
#[derive(Clone, Debug)]
pub struct Player {
    pub uuid: u128,
    /// The connection that receives the player's locator bar (None: the host,
    /// whose own game draws it).
    pub conn: Option<u32>,
    pub position: [f64; 3],
    pub spectator: bool,
    /// `waypoint_transmit_range`: 0 while hidden.
    pub transmit_range: f64,
    pub receive_range: f64,
    /// `Entity.firstTick`: the player has not ticked since joining.
    pub first_tick: bool,
    /// The chunk view radius (`ChunkMap.getPlayerViewDistance`).
    pub view_distance: i32,
    /// The player's team and its colour (`TeamColor.rgb`).
    pub team: Option<(String, Option<i32>)>,
    /// The icon `/waypoint modify` set.
    pub icon: Icon,
}

impl Player {
    fn block(&self) -> [i32; 3] {
        self.position.map(|v| v.floor() as i32)
    }
    fn chunk(&self) -> [i32; 2] {
        let [x, _, z] = self.block();
        [x >> 4, z >> 4]
    }
    /// The squared `Entity.distanceTo`, before the square root.
    fn distance_squared(&self, other: &Player) -> f64 {
        (0..3)
            .map(|k| (self.position[k] - other.position[k]).powi(2))
            .sum()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    Block([i32; 3]),
    Chunk([i32; 2]),
    Azimuth(f32),
}

#[derive(Clone, Debug)]
struct Connection {
    kind: Kind,
    icon: Icon,
}

/// What the manager remembers of a player between ticks.
struct Memo {
    position: [f64; 3],
    /// The centre of the player's ChunkTrackingView (`ChunkMap.move`).
    view_center: [i32; 2],
    transmitting: bool,
    team: Option<(String, Option<i32>)>,
}

/// One receiver's connections, by source slot, and the sources in the order they connected.
#[derive(Default)]
struct Row {
    cells: Vec<Option<Connection>>,
    order: Vec<u32>,
}

impl Row {
    fn get_mut(&mut self, source: u32) -> Option<&mut Connection> {
        self.cells.get_mut(source as usize)?.as_mut()
    }
    /// Replaces in place, or connects last.
    fn put(&mut self, source: u32, c: Connection) {
        let i = source as usize;
        if self.cells.len() <= i {
            self.cells.resize_with(i + 1, || None);
        }
        if self.cells[i].replace(c).is_none() {
            self.order.push(source);
        }
    }
    fn take(&mut self, source: u32) -> Option<Connection> {
        let c = self.cells.get_mut(source as usize)?.take()?;
        let i = self.order.iter().position(|&o| o == source)?;
        self.order.remove(i);
        Some(c)
    }
    /// Every source, in connection order, leaving the row empty.
    fn drain(&mut self) -> Vec<u32> {
        for &source in &self.order {
            self.cells[source as usize] = None;
        }
        std::mem::take(&mut self.order)
    }
}

/// ServerWaypointManager for one level; rows, flags and memos are indexed by player slot (`slot_of`).
#[derive(Default)]
pub struct Waypoints {
    slot_of: HashMap<u128, u32>,
    uuid_of: Vec<u128>,
    free: Vec<u32>,
    /// The transmitters, in the order they were tracked.
    waypoints: Vec<u32>,
    is_waypoint: Vec<bool>,
    /// The receivers, in the order they joined.
    players: Vec<u32>,
    is_player: Vec<bool>,
    /// Connections by receiver slot, then source slot.
    rows: Vec<Row>,
    memo: Vec<Option<Memo>>,
    /// The `locator_bar` game rule as last seen.
    enabled: Option<bool>,
}

/// One tick's players, and the packets for their connections.
struct Tick<'a> {
    /// The players in the level, by slot.
    pl: Vec<Option<&'a Player>>,
    /// Each present player's block and chunk, worked out once.
    blk: Vec<[i32; 3]>,
    chk: Vec<[i32; 2]>,
    uuid_of: &'a [u128],
    /// The connections of the players in other levels: one who just left this level is
    /// still told of the waypoints it loses (`removePlayer` disconnects them).
    away: &'a [(u128, u32)],
    view: Vec<Option<([i32; 2], i32)>>,
    /// Players a respawn replaced this tick: a new ServerPlayer, on its first tick.
    fresh: Vec<bool>,
    enabled: bool,
    out: Vec<(u32, C)>,
}

impl<'a> Tick<'a> {
    fn conn(&self, receiver: u32) -> Option<u32> {
        self.pl[receiver as usize].and_then(|p| p.conn).or_else(|| {
            let uuid = self.uuid_of[receiver as usize];
            self.away
                .iter()
                .find(|(u, _)| *u == uuid)
                .map(|&(_, conn)| conn)
        })
    }
    fn send(
        &mut self,
        receiver: u32,
        source: u32,
        operation: i32,
        icon: &Icon,
        target: WaypointTarget,
    ) {
        let Some(conn) = self.conn(receiver) else {
            return;
        };
        let uuid = self.uuid_of[source as usize];
        self.out.push((conn, waypoint(uuid, operation, icon, target)));
    }
    fn connect(&mut self, receiver: u32, source: u32, c: &Connection) {
        let target = match c.kind {
            Kind::Block(p) => WaypointTarget::Block(p),
            Kind::Chunk(p) => WaypointTarget::Chunk(p),
            Kind::Azimuth(a) => WaypointTarget::Azimuth(a),
        };
        self.send(receiver, source, waypoint_operation::TRACK, &c.icon, target);
    }
    /// `ClientboundTrackedWaypointPacket.removeWaypoint`: an empty waypoint with the null icon.
    fn disconnect(&mut self, receiver: u32, source: u32) {
        self.send(
            receiver,
            source,
            waypoint_operation::UNTRACK,
            &Icon::default(),
            WaypointTarget::Empty,
        );
    }
    /// `ChunkTrackingView.isInViewDistance` of the receiver's view.
    fn chunk_visible(&self, [x, z]: [i32; 2], receiver: u32) -> bool {
        let Some(([cx, cz], r)) = self.view[receiver as usize] else {
            return false;
        };
        let dx = i64::from(((x - cx).abs() - 1).max(0));
        let dz = i64::from(((z - cz).abs() - 1).max(0));
        dx * dx + dz * dz < i64::from(r) * i64::from(r)
    }
    /// `WaypointTransmitter.doesSourceIgnoreReceiver`.
    fn ignores(source: &Player, receiver: &Player) -> bool {
        if receiver.spectator {
            return false;
        }
        if source.spectator {
            return true;
        }
        let range = source.transmit_range.min(receiver.receive_range);
        let d2 = source.distance_squared(receiver);
        // Well inside the range the rounded f32 distance cannot reach it.
        if (0.0..1.0e30).contains(&range) && d2 < (range * 0.999_999) * (range * 0.999_999) {
            return false;
        }
        f64::from(d2.sqrt() as f32) >= range
    }
    fn really_far(source: &Player, receiver: &Player) -> bool {
        let d2 = source.distance_squared(receiver);
        // The f32 distance is past 332 only above 332 (the margins swallow its rounding).
        if d2 < 331.9 * 331.9 {
            return false;
        }
        if d2 > 332.1 * 332.1 {
            return true;
        }
        d2.sqrt() as f32 > REALLY_FAR
    }
    /// `EntityAzimuthConnection`: the angle of the receiver seen from the source,
    /// `Mth.atan2` of the offset turned clockwise.
    fn azimuth(source: &Player, receiver: &Player) -> f32 {
        let d = [0, 1, 2].map(|k| receiver.position[k] - source.position[k]);
        horizon_core::geometry::trig::atan2(d[0], -d[2]) as f32
    }
    /// The source and the receiver, if both are in the level this tick.
    fn pair(&self, source: u32, receiver: u32) -> Option<(&'a Player, &'a Player)> {
        Some((self.pl[source as usize]?, self.pl[receiver as usize]?))
    }
    /// `LivingEntity.makeWaypointConnectionWith`.
    fn make(&self, (s, r): (&Player, &Player), source: u32, receiver: u32) -> Option<Connection> {
        if s.first_tick || self.fresh[source as usize] || s.uuid == r.uuid || Self::ignores(s, r) {
            return None;
        }
        // Waypoint.Icon.cloneAndAssignStyle: the icon's colour, else the team's.
        let mut icon = s.icon.clone();
        if icon.color.is_none() {
            icon.color = s.team.as_ref().and_then(|t| t.1);
        }
        let chunk = self.chk[source as usize];
        let kind = if Self::really_far(s, r) {
            Kind::Azimuth(Self::azimuth(s, r))
        } else if !self.chunk_visible(chunk, receiver) {
            Kind::Chunk(chunk)
        } else {
            Kind::Block(self.blk[source as usize])
        };
        Some(Connection { kind, icon })
    }
    /// `Connection.isBroken` for each kind.
    fn broken(
        &self,
        c: &Connection,
        (s, r): (&Player, &Player),
        source: u32,
        receiver: u32,
    ) -> bool {
        match c.kind {
            Kind::Block(last) => {
                let now = self.blk[source as usize];
                (0..3).map(|k| (now[k] - last[k]).abs()).sum::<i32>() > 1 || Self::ignores(s, r)
            }
            Kind::Chunk(last) => {
                let now = self.chk[source as usize];
                let moved = (now[0] - last[0]).abs().max((now[1] - last[1]).abs());
                moved > 1 || Self::ignores(s, r) || self.chunk_visible(last, receiver)
            }
            Kind::Azimuth(_) => {
                Self::ignores(s, r)
                    || self.chunk_visible(self.chk[source as usize], receiver)
                    || !Self::really_far(s, r)
            }
        }
    }
    /// `Connection.update`: the packet when the waypoint moved.
    fn update(&mut self, c: &mut Connection, (s, r): (&Player, &Player), source: u32) {
        let target = match &mut c.kind {
            Kind::Block(last) => {
                let now = self.blk[source as usize];
                if now == *last {
                    return;
                }
                *last = now;
                WaypointTarget::Block(now)
            }
            Kind::Chunk(last) => {
                let now = self.chk[source as usize];
                if now == *last {
                    return;
                }
                *last = now;
                WaypointTarget::Chunk(now)
            }
            Kind::Azimuth(last) => {
                let now = Self::azimuth(s, r);
                if (now - *last).abs() <= AZIMUTH_STEP {
                    return;
                }
                *last = now;
                WaypointTarget::Azimuth(now)
            }
        };
        if let Some(conn) = r.conn {
            self.out.push((
                conn,
                waypoint(s.uuid, waypoint_operation::UPDATE, &c.icon, target),
            ));
        }
    }
}

fn waypoint(source: u128, operation: i32, icon: &Icon, target: WaypointTarget) -> C {
    C::TrackedWaypoint(TrackedWaypoint {
        operation,
        id: WaypointId::Uuid(source),
        style: icon.style.clone(),
        color: icon.color,
        target,
    })
}

impl Waypoints {
    /// `createConnection`.
    fn create_connection(&mut self, t: &mut Tick, receiver: u32, source: u32) {
        if receiver == source || !t.enabled {
            return;
        }
        match t
            .pair(source, receiver)
            .and_then(|pair| t.make(pair, source, receiver))
        {
            Some(c) => {
                t.connect(receiver, source, &c);
                self.rows[receiver as usize].put(source, c);
            }
            None => {
                if self.rows[receiver as usize].take(source).is_some() {
                    t.disconnect(receiver, source);
                }
            }
        }
    }
    /// The receiver's connection to the source updated in its place
    /// (`updateConnection`), or made if it has none (`createConnection`).
    fn update_or_create(&mut self, t: &mut Tick, receiver: u32, source: u32) {
        if receiver == source || !t.enabled {
            return;
        }
        let pair = t.pair(source, receiver);
        let Some(c) = self.rows[receiver as usize].get_mut(source) else {
            return self.create_connection(t, receiver, source);
        };
        // A connection to a player who isn't in the level is broken.
        if let Some(pair) = pair
            && !t.broken(c, pair, source, receiver)
        {
            t.update(c, pair, source);
            return;
        }
        match pair.and_then(|pair| t.make(pair, source, receiver)) {
            Some(new) => {
                t.connect(receiver, source, &new);
                *c = new;
            }
            None => {
                t.disconnect(receiver, source);
                self.rows[receiver as usize].take(source);
            }
        }
    }
    /// `trackWaypoint`.
    fn track(&mut self, t: &mut Tick, source: u32) {
        if !self.is_waypoint[source as usize] {
            self.is_waypoint[source as usize] = true;
            self.waypoints.push(source);
        }
        for k in 0..self.players.len() {
            self.create_connection(t, self.players[k], source);
        }
    }
    /// `updateWaypoint`.
    fn update_waypoint(&mut self, t: &mut Tick, source: u32) {
        if !self.is_waypoint[source as usize] {
            return;
        }
        // Connections change; the players don't.
        for k in 0..self.players.len() {
            self.update_or_create(t, self.players[k], source);
        }
    }
    /// `untrackWaypoint`.
    fn untrack(&mut self, t: &mut Tick, source: u32) {
        for k in 0..self.players.len() {
            let receiver = self.players[k];
            if self.rows[receiver as usize].take(source).is_some() {
                t.disconnect(receiver, source);
            }
        }
        if self.is_waypoint[source as usize] {
            self.is_waypoint[source as usize] = false;
            self.waypoints.retain(|&w| w != source);
        }
    }
    /// `addPlayer`.
    fn add_player(&mut self, t: &mut Tick, player: u32, transmitting: bool) {
        if !self.is_player[player as usize] {
            self.is_player[player as usize] = true;
            self.players.push(player);
        }
        for k in 0..self.waypoints.len() {
            self.create_connection(t, player, self.waypoints[k]);
        }
        if transmitting {
            self.track(t, player);
        }
    }
    /// `updatePlayer`.
    fn update_player(&mut self, t: &mut Tick, player: u32) {
        // Connections change; the waypoints don't.
        for k in 0..self.waypoints.len() {
            self.update_or_create(t, player, self.waypoints[k]);
        }
    }
    /// `removePlayer`.
    fn remove_player(&mut self, t: &mut Tick, player: u32) {
        for source in self.rows[player as usize].drain() {
            t.disconnect(player, source);
        }
        self.untrack(t, player);
        if self.is_player[player as usize] {
            self.is_player[player as usize] = false;
            self.players.retain(|&p| p != player);
        }
    }
    /// `breakAllConnections`.
    fn break_all(&mut self, t: &mut Tick) {
        for k in 0..self.players.len() {
            let receiver = self.players[k];
            for source in self.rows[receiver as usize].drain() {
                t.disconnect(receiver, source);
            }
        }
        // A row of a player who receives without having joined as a receiver goes silently.
        self.rows.iter_mut().for_each(|row| drop(row.drain()));
    }
    /// The player's slot, a free one if it has none.
    fn slot(&mut self, uuid: u128) -> u32 {
        if let Some(&slot) = self.slot_of.get(&uuid) {
            return slot;
        }
        let slot = self.free.pop().unwrap_or_else(|| {
            self.uuid_of.push(0);
            self.is_waypoint.push(false);
            self.is_player.push(false);
            self.rows.push(Row::default());
            self.memo.push(None);
            self.uuid_of.len() as u32 - 1
        });
        self.uuid_of[slot as usize] = uuid;
        self.slot_of.insert(uuid, slot);
        slot
    }

    /// One tick: players who left, joined, moved, hid or changed team, and the
    /// `locator_bar` game rule. `respawned` players are removed and added again,
    /// as `PlayerList.respawn` replaces the ServerPlayer. Returns the packets for
    /// each connection.
    pub fn tick(
        &mut self,
        players: &[Player],
        respawned: &[u128],
        locator_bar: bool,
    ) -> Vec<(u32, C)> {
        self.tick_level(players, &[], respawned, locator_bar)
    }

    /// [`Self::tick`] for one level of several: `away` are the connections of the players
    /// in the other levels, for the untracks owed to one who has just left this one.
    pub fn tick_level(
        &mut self,
        players: &[Player],
        away: &[(u128, u32)],
        respawned: &[u128],
        locator_bar: bool,
    ) -> Vec<(u32, C)> {
        let slots: Vec<u32> = players.iter().map(|p| self.slot(p.uuid)).collect();
        let n = self.uuid_of.len();
        let mut pl = vec![None; n];
        let mut blk = vec![[0; 3]; n];
        let mut chk = vec![[0; 2]; n];
        for (p, &s) in players.iter().zip(&slots) {
            pl[s as usize] = Some(p);
            blk[s as usize] = p.block();
            chk[s as usize] = p.chunk();
        }
        let mut fresh = vec![false; n];
        for uuid in respawned {
            if let Some(&s) = self.slot_of.get(uuid) {
                fresh[s as usize] = true;
            }
        }
        // The tick borrows the uuids while the manager's other fields change.
        let uuid_of = std::mem::take(&mut self.uuid_of);
        let mut t = Tick {
            pl,
            blk,
            chk,
            uuid_of: &uuid_of,
            away,
            view: vec![None; n],
            fresh,
            enabled: locator_bar,
            out: Vec::new(),
        };
        // Vanilla reads the rule when it connects; changing it breaks or remakes
        // every connection (MinecraftServer.onGameRuleChanged).
        let was = self.enabled.replace(locator_bar);
        for s in 0..n {
            if let (Some(memo), Some(p)) = (&self.memo[s], t.pl[s]) {
                t.view[s] = Some((memo.view_center, p.view_distance));
            }
        }
        // Players who left, and the ServerPlayer a respawn replaced.
        let gone: Vec<u32> = self
            .players
            .iter()
            .copied()
            .filter(|&s| t.pl[s as usize].is_none() || t.fresh[s as usize])
            .collect();
        for s in gone {
            self.remove_player(&mut t, s);
            self.memo[s as usize] = None;
        }
        if was == Some(true) && !locator_bar {
            self.break_all(&mut t);
        }
        // Players who joined (PlayerList.placeNewPlayer: addPlayer).
        for (p, &s) in players.iter().zip(&slots) {
            if self.memo[s as usize].is_some() {
                continue;
            }
            let center = t.chk[s as usize];
            t.view[s as usize] = Some((center, p.view_distance));
            let transmitting = p.transmit_range > 0.0;
            self.memo[s as usize] = Some(Memo {
                position: p.position,
                view_center: center,
                transmitting,
                team: p.team.clone(),
            });
            if p.receive_range > 0.0 {
                self.add_player(&mut t, s, transmitting);
            } else if transmitting {
                self.track(&mut t, s);
            }
        }
        if was == Some(false) && locator_bar {
            for k in 0..self.players.len() {
                self.update_player(&mut t, self.players[k]);
            }
        }
        for (p, &s) in players.iter().zip(&slots) {
            let su = s as usize;
            let memo = self.memo[su].as_ref().expect("joined above");
            // Entity.setPosRaw, then ServerChunkCache.move.
            if memo.position != p.position {
                let center = t.chk[su];
                let moved_view = memo.view_center != center;
                let transmitting = memo.transmitting;
                self.memo[su].as_mut().expect("joined above").position = p.position;
                if !p.first_tick {
                    if transmitting {
                        self.update_waypoint(&mut t, s);
                    }
                    if p.receive_range > 0.0 {
                        self.update_player(&mut t, s);
                    }
                }
                if moved_view {
                    self.memo[su].as_mut().expect("joined above").view_center = center;
                    t.view[su] = Some((center, p.view_distance));
                }
                if p.receive_range > 0.0 {
                    self.update_player(&mut t, s);
                }
            }
            // LivingEntity.onAttributeUpdated(WAYPOINT_TRANSMIT_RANGE).
            let transmitting = p.transmit_range > 0.0;
            if transmitting != self.memo[su].as_ref().expect("joined above").transmitting {
                self.memo[su].as_mut().expect("joined above").transmitting = transmitting;
                if transmitting {
                    self.track(&mut t, s);
                } else {
                    self.untrack(&mut t, s);
                }
            }
            // ServerScoreboard: a team joined, left or recoloured remakes the connections.
            if p.team != self.memo[su].as_ref().expect("joined above").team {
                self.memo[su].as_mut().expect("joined above").team = p.team.clone();
                for k in 0..self.players.len() {
                    self.create_connection(&mut t, self.players[k], s);
                }
            }
        }
        let out = t.out;
        self.uuid_of = uuid_of;
        self.free_slots(&slots);
        out
    }

    /// Every connection as (receiver, source, what it holds), by receiver uuid and then in connection order.
    #[cfg(test)]
    fn dump(&self) -> Vec<(u128, u128, String)> {
        let mut dump = Vec::new();
        for (r, row) in self.rows.iter().enumerate() {
            for &s in &row.order {
                let c = row.cells[s as usize].as_ref().unwrap();
                let kind = format!("{:?} {:?}", c.kind, c.icon);
                dump.push((self.uuid_of[r], self.uuid_of[s as usize], kind));
            }
        }
        dump.sort_by_key(|d| d.0);
        dump
    }

    /// Frees the slots of players gone from the level once nothing refers to them.
    fn free_slots(&mut self, present: &[u32]) {
        let mut here = vec![false; self.uuid_of.len()];
        present.iter().for_each(|&s| here[s as usize] = true);
        for s in 0..self.uuid_of.len() {
            let uuid = self.uuid_of[s];
            let held = self.slot_of.get(&uuid) == Some(&(s as u32));
            if held
                && !here[s]
                && self.memo[s].is_none()
                && !self.is_player[s]
                && !self.is_waypoint[s]
                // A row of a receiver that never joined can still hold a cell for a source that left.
                && self.rows.iter().all(|row| row.cells.get(s).is_none_or(Option::is_none))
            {
                self.slot_of.remove(&uuid);
                self.rows[s] = Row::default();
                self.free.push(s as u32);
            }
        }
    }
}

#[cfg(test)]
mod differential;
#[cfg(test)]
mod reference;

#[cfg(test)]
mod tests {
    use super::*;

    fn player(uuid: u128, conn: u32, position: [f64; 3]) -> Player {
        Player {
            uuid,
            conn: Some(conn),
            position,
            spectator: false,
            transmit_range: PLAYER_RANGE,
            receive_range: PLAYER_RANGE,
            first_tick: false,
            view_distance: 4,
            team: None,
            icon: Icon::default(),
        }
    }

    /// The packets to `conn`, as (operation, target).
    fn to(out: &[(u32, C)], conn: u32) -> Vec<(i32, WaypointTarget)> {
        out.iter()
            .filter(|(c, _)| *c == conn)
            .map(|(_, p)| match p {
                C::TrackedWaypoint(w) => (w.operation, w.target),
                other => panic!("{other:?}"),
            })
            .collect()
    }

    #[test]
    fn a_new_player_shows_once_it_moves() {
        let mut w = Waypoints::default();
        let a = player(1, 10, [0.5, 100.0, 0.5]);
        let mut b = player(2, 20, [5.5, 100.0, 0.5]);
        b.first_tick = true;
        w.tick(std::slice::from_ref(&a), &[], true);
        let out = w.tick(&[a.clone(), b.clone()], &[], true);
        // B gets A at once; A sees nothing of B while B hasn't ticked.
        assert_eq!(to(&out, 20), [(0, WaypointTarget::Block([0, 100, 0]))]);
        assert!(to(&out, 10).is_empty());
        b.first_tick = false;
        assert!(w.tick(&[a.clone(), b.clone()], &[], true).is_empty());
        b.position[0] += 1.0;
        let out = w.tick(&[a.clone(), b.clone()], &[], true);
        assert_eq!(to(&out, 10), [(0, WaypointTarget::Block([6, 100, 0]))]);
        // One block: an update. Three: a new connection. Out of view: a chunk.
        b.position[0] += 1.0;
        let out = w.tick(&[a.clone(), b.clone()], &[], true);
        assert_eq!(to(&out, 10), [(2, WaypointTarget::Block([7, 100, 0]))]);
        b.position[0] += 3.0;
        let out = w.tick(&[a.clone(), b.clone()], &[], true);
        assert_eq!(to(&out, 10), [(0, WaypointTarget::Block([10, 100, 0]))]);
        b.position[0] = 100.5;
        let out = w.tick(&[a.clone(), b.clone()], &[], true);
        assert_eq!(to(&out, 10), [(0, WaypointTarget::Chunk([6, 0]))]);
        // Really far: an angle.
        b.position[0] = 1000.5;
        let out = w.tick(&[a.clone(), b.clone()], &[], true);
        assert!(matches!(
            to(&out, 10)[..],
            [(0, WaypointTarget::Azimuth(_))]
        ));
        // Hidden: untracked; the rule off: every connection broken.
        b.transmit_range = 0.0;
        let out = w.tick(&[a.clone(), b.clone()], &[], true);
        assert_eq!(to(&out, 10), [(1, WaypointTarget::Empty)]);
        let out = w.tick(&[a.clone(), b.clone()], &[], false);
        assert_eq!(to(&out, 20), [(1, WaypointTarget::Empty)]);
        assert!(to(&out, 10).is_empty());
        // Leaving untracks what was left.
        let out = w.tick(&[a.clone(), b.clone()], &[], true);
        assert!(matches!(
            to(&out, 20)[..],
            [(0, WaypointTarget::Azimuth(_))]
        ));
        let out = w.tick(&[b.clone()], &[], true);
        assert_eq!(to(&out, 20), [(1, WaypointTarget::Empty)]);
    }

    #[test]
    fn a_player_leaving_the_level_loses_its_waypoints() {
        let (a, b) = (
            player(1, 10, [0.5, 64.0, 0.5]),
            player(2, 20, [4.5, 64.0, 0.5]),
        );
        let mut w = Waypoints::default();
        w.tick(&[a.clone(), b.clone()], &[], true);
        let mut moved = b.clone();
        moved.position = [5.5, 64.0, 0.5];
        w.tick(&[a.clone(), moved], &[], true);
        // B goes to another level: A loses B, and B (reached through `away`) loses A.
        let out = w.tick_level(std::slice::from_ref(&a), &[(2, 20)], &[], true);
        assert_eq!(
            to(&out, 10),
            vec![(waypoint_operation::UNTRACK, WaypointTarget::Empty)]
        );
        assert_eq!(
            to(&out, 20),
            vec![(waypoint_operation::UNTRACK, WaypointTarget::Empty)]
        );
    }

    #[test]
    fn team_colours() {
        assert_eq!(team_rgb("red"), Some(0xFF5555));
        assert_eq!(team_rgb("black"), Some(0xFF30_3030_u32 as i32));
        assert_eq!(team_rgb("reset"), None);
    }
}
