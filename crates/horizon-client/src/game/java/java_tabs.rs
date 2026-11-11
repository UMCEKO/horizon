// game-module-cfg: feature java-mods
//! The creative tabs of the Java client: built by its login, fetched once; the screen itself never calls Java.
#![allow(dead_code)]

use super::*;
use horizon_client::creative::{JavaTab, JavaTabKind, JavaTabStack, JavaTabs};
use horizon_java_host::wire::In;
use std::cell::{Cell, RefCell};
use std::sync::mpsc::{Receiver, channel};

thread_local! {
    static JOINED: Cell<bool> = const { Cell::new(false) };
    static PENDING: RefCell<Option<Receiver<Option<JavaTabs>>>> = const { RefCell::new(None) };
}

fn strings(reply: &mut In) -> Option<Vec<String>> {
    (0..reply.int()?)
        .map(|_| reply.text().map(str::to_owned))
        .collect()
}

fn indexes(reply: &mut In) -> Option<Vec<usize>> {
    (0..reply.int()?)
        .map(|_| usize::try_from(reply.int()?).ok())
        .collect()
}

fn stack(reply: &mut In) -> Option<Option<ItemStack>> {
    let tag = super::java_mods::read_tag(reply.bytes()?)?;
    Some(horizon_client::save::item_from_nbt(&tag))
}

fn kind(name: &str) -> Option<JavaTabKind> {
    Some(match name {
        "CATEGORY" => JavaTabKind::Category,
        "SEARCH" => JavaTabKind::Search,
        "INVENTORY" => JavaTabKind::Inventory,
        "HOTBAR" => JavaTabKind::Hotbar,
        _ => return None,
    })
}

fn tab(reply: &mut In) -> Option<JavaTab> {
    let id = reply.text()?.to_owned();
    let title = super::java_mods::read_tag(reply.bytes()?)?
        .get("v")?
        .clone();
    let icon = stack(reply)?.unwrap_or_else(|| ItemStack::new("minecraft:air", 1));
    let background = reply.text()?.to_owned();
    let kind = kind(reply.text()?)?;
    let top = reply.bool()?;
    let column = u32::try_from(reply.int()?).ok()?;
    let aligned_right = reply.bool()?;
    let page = u32::try_from(reply.int()?).ok();
    let can_scroll = reply.bool()?;
    let show_title = reply.bool()?;
    // A mod tab's search bar, its width and search items: ari's Tab has no mod search box yet.
    let _bar = reply.bool()?;
    let _width = reply.int()?;
    let label_color = reply.int()? as u32;
    let scroller = reply.text()?.to_owned();
    let items = indexes(reply)?;
    let _search_items = indexes(reply)?;
    Some(JavaTab {
        id,
        title,
        icon,
        background,
        kind,
        top,
        column,
        aligned_right,
        page,
        can_scroll,
        show_title,
        label_color,
        scroller,
        items,
    })
}

fn set(reply: &mut In) -> Option<Vec<JavaTab>> {
    let _pages = reply.int()?;
    (0..reply.int()?).map(|_| tab(reply)).collect()
}

fn parse(reply: &[u8]) -> Option<JavaTabs> {
    let mut reply = In(reply);
    let stacks = (0..reply.int()?)
        .map(|_| {
            let stack = stack(&mut reply)?;
            let lines = strings(&mut reply)?;
            let tags = strings(&mut reply)?;
            Some(stack.map(|stack| JavaTabStack { stack, lines, tags }))
        })
        .collect::<Option<_>>()?;
    let without_permissions = set(&mut reply)?;
    let with_permissions = set(&mut reply)?;
    // A table that listed more stacks than it counted would misread the sets without failing.
    if !reply.0.is_empty() {
        return None;
    }
    Some(JavaTabs {
        stacks,
        without_permissions,
        with_permissions,
    })
}

impl Game<'_> {
    /// The Java client's login built the tabs: they are fetched once, off the window's thread.
    pub(in crate::game) fn java_creative_tabs_built(&self) {
        if JOINED.replace(true) {
            return;
        }
        let (done, reply) = channel();
        let built = std::thread::Builder::new()
            .name("Java creative tabs".into())
            .spawn(move || {
                let answer = horizon_java_host::get()
                    .and_then(|host| host.request("client_creative_tabs", &[]))
                    .and_then(|bytes| parse(&bytes));
                let _ = done.send(answer);
            });
        if built.is_ok() {
            PENDING.with_borrow_mut(|pending| *pending = Some(reply));
        }
    }

    /// The finished build, once.
    pub(in crate::game) fn java_creative_tabs_take(&mut self) {
        let Some(answer) = PENDING.with_borrow(|pending| pending.as_ref()?.try_recv().ok()) else {
            return;
        };
        PENDING.with_borrow_mut(|pending| *pending = None);
        match answer {
            Some(data) => self.creative_screen.set_java_tabs(&data, &self.pack.names),
            None => log::error!("Horizon: the Java client built no creative tabs"),
        }
    }

    /// The Java client left, so the session's tabs go with it.
    pub(in crate::game) fn java_creative_tabs_left(&mut self) {
        JOINED.set(false);
        PENDING.with_borrow_mut(|pending| *pending = None);
        self.creative_screen.clear_java_tabs(&self.pack.names);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use horizon_java_host::wire::Out;

    fn hex(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    // Captured from the Java client: a full basic energy cube's table entry, and a vanilla tab's title.
    const CUBE: &str = "000000590a00000a000a636f6d706f6e656e747304000f6d656b616e69736d3a656e6572677900000000003d090000030005636f756e74000000010800026964001a6d656b616e69736d3a62617369635f656e657267795f63756265000000000300000011426173696320456e6572677920437562650000001c486f6c64204c65667420536869667420666f722064657461696c732e00000021486f6c64205348494654202b204e20666f722061206465736372697074696f6e2e00000001000000156d656b616e69736d3a656e657267795f6375626573";
    const TITLE: &str = "0a00000a0001760800097472616e736c61746500186974656d47726f75702e6275696c64696e67426c6f636b730000";

    /// One set as `CreativeTabs.set` writes it: a category tab on page 0 and the search tab on every page.
    fn set(out: &mut Out, operator: bool) {
        out.int(2);
        out.int(2 + i32::from(operator));
        let tab = |out: &mut Out, id: &str, kind: &str, page: i32, column: i32, bar: bool| {
            out.text(id);
            out.bytes(&hex(TITLE));
            out.bytes(&hex("0a000000"));
            out.text("minecraft:textures/gui/container/creative_inventory/tab_items.png");
            out.text(kind);
            out.bool(true);
            out.int(column);
            out.bool(column >= 5);
            out.int(page);
            out.bool(true);
            out.bool(true);
            out.bool(bar);
            out.int(89);
            out.int(0xff40_4040_u32 as i32);
            out.text("minecraft:container/creative_inventory/scroller");
            for _ in 0..2 {
                out.int(1);
                out.int(0);
            }
        };
        tab(out, "minecraft:building_blocks", "CATEGORY", 0, 0, false);
        tab(out, "minecraft:search", "SEARCH", -1, 6, true);
        if operator {
            tab(out, "minecraft:op_blocks", "CATEGORY", -1, 5, false);
        }
    }

    fn reply() -> Out {
        let mut out = Out::default();
        out.int(1);
        out.0.extend(hex(CUBE));
        set(&mut out, false);
        set(&mut out, true);
        out
    }

    #[test]
    fn a_reply_from_the_java_client_reads_back() {
        let data = parse(&reply().0).expect("the reply reads");
        let cube = data.stacks[0].as_ref().unwrap();
        assert_eq!(cube.stack.item.as_ref(), "mekanism:basic_energy_cube");
        let saved = horizon_client::item_data::encode_components(&cube.stack);
        assert!(saved.is_some(), "the cube keeps its energy component");
        assert_eq!(cube.lines[0], "Basic Energy Cube");
        assert_eq!(cube.tags, ["mekanism:energy_cubes"]);
        let (without, with) = (&data.without_permissions, &data.with_permissions);
        assert_eq!((without.len(), with.len()), (2, 3));
        let search = &without[1];
        assert_eq!((search.page, search.kind), (None, JavaTabKind::Search));
        assert_eq!(without[0].page, Some(0));
        assert_eq!(without[0].label_color, 0xff40_4040);
        assert_eq!(without[0].items, [0]);
        let title = horizon_client::component::name_text(&without[0].title);
        assert_eq!(title.translation_key(), Some("itemGroup.buildingBlocks"));
    }

    #[test]
    fn a_table_that_miscounts_its_stacks_is_refused() {
        let mut out = reply();
        // One entry more than the count says, as a stack hashed twice once wrote.
        let cube = hex(CUBE);
        out.0.splice(4..4, cube);
        assert!(parse(&out.0).is_none());
    }
}
