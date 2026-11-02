//! Outlines of mods' blocks whose shape depends on their block entity: the Java client in the same
//! JVM shapes them from its copy of the block entity, and the answer is cached until it may change.

use super::java_screens::live::java_shape_generation;
use super::*;
use horizon_java_host::wire::{In, Out};
use std::cell::RefCell;
use std::collections::HashMap;

const CACHE_LIMIT: usize = 4096;

type Cached = (BlockStateId, i32, Vec<[f64; 6]>);

thread_local! {
    static CACHE: RefCell<HashMap<[i32; 3], Cached>> = RefCell::default();
}

impl Game<'_> {
    /// The block-local boxes the Java client outlines this block with; None when Horizon's own facts apply.
    pub(super) fn java_outline(&self, pos: [i32; 3], state: BlockStateId) -> Option<Vec<[f64; 6]>> {
        self.remote.as_ref()?;
        let map = horizon_java_host::bridge::STATES.get()?;
        let at = state.0 as usize;
        if !map.owned.get(at).copied().unwrap_or(false)
            || !map.entity.get(at).copied().unwrap_or(false)
        {
            return None;
        }
        let host = horizon_java_host::get()?;
        let generation = java_shape_generation();
        let cached = CACHE.with_borrow(|c| {
            c.get(&pos)
                .filter(|(s, g, _)| *s == state && *g == generation)
                .map(|(_, _, boxes)| boxes.clone())
        });
        if cached.is_some() {
            return cached;
        }
        let mut out = Out::default();
        out.long(BlockPos::new(pos[0], pos[1], pos[2]).pack());
        let reply = host.request("client_shape", &out.0)?;
        let mut reply = In(&reply);
        reply.byte()?;
        let mut boxes = Vec::new();
        for _ in 0..reply.int()? {
            let mut b = [0.0; 6];
            for v in &mut b {
                *v = reply.double()?;
            }
            boxes.push(b);
        }
        CACHE.with_borrow_mut(|c| {
            if c.len() >= CACHE_LIMIT {
                c.clear();
            }
            c.insert(pos, (state, generation, boxes.clone()));
        });
        Some(boxes)
    }
}
