//! Bytecode helpers of the entity facts: instructions, exits, local slots, and the vanilla static initialisers.

use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;

use crate::class::{ClassError, ClassInfo, Parse, Pool, op_length, params, parse_class, u2};

pub const RETURN: u8 = 0xb1;
pub const GOTO: u8 = 0xa7;
pub const IFEQ: u8 = 0x99;
pub const IFNE: u8 = 0x9a;
pub const IF_ACMPNE: u8 = 0xa6;
pub const GETSTATIC: u8 = 0xb2;
pub const PUTSTATIC: u8 = 0xb3;
pub const INVOKEVIRTUAL: u8 = 0xb6;
pub const INVOKEINTERFACE: u8 = 0xb9;
pub const INSTANCEOF: u8 = 0xc1;
pub const CHECKCAST: u8 = 0xc0;
pub const DUP: u8 = 0x59;
pub const POP: u8 = 0x57;
pub const LDC: u8 = 0x12;
pub const LDC_W: u8 = 0x13;

/// One instruction: offset, opcode and the operand as Python computes it per opcode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Instruction {
    pub offset: usize,
    pub op: u8,
    pub operand: Option<i64>,
}

fn byte(code: &[u8], p: usize) -> Result<u8, ClassError> {
    code.get(p)
        .copied()
        .ok_or_else(|| ClassError(format!("read past the end at {p}")))
}

/// Port of mod_entity_facts.instructions.
pub fn instructions(code: &[u8]) -> Result<Vec<Instruction>, ClassError> {
    let mut out = Vec::new();
    let mut p = 0;
    while p < code.len() {
        let op = code[p];
        let at = i64::try_from(p).unwrap_or(i64::MAX);
        let operand = if op == LDC {
            Some(i64::from(byte(code, p + 1)?))
        } else if matches!(op, 0x13 | 0x14 | 0xbb | 0xbd | 0xc0 | 0xc1)
            || (0xb2..=0xba).contains(&op)
        {
            Some(i64::from(u2(code, p + 1)?))
        } else if (0x99..=0xa8).contains(&op) || matches!(op, 0xc6 | 0xc7) {
            Some(at + i64::from(u2(code, p + 1)? as i16))
        } else if matches!(op, 0xc8 | 0xc9) {
            let raw = (u32::from(u2(code, p + 1)?) << 16) | u32::from(u2(code, p + 3)?);
            Some(at + i64::from(raw as i32))
        } else if (0x15..=0x19).contains(&op) || (0x36..=0x3a).contains(&op) {
            Some(i64::from(byte(code, p + 1)?))
        } else if (0x1a..=0x2d).contains(&op) {
            Some(i64::from((op - 0x1a) % 4))
        } else if (0x3b..=0x4e).contains(&op) {
            Some(i64::from((op - 0x3b) % 4))
        } else if op == 0xc4 {
            Some(i64::from(u2(code, p + 2)?))
        } else {
            None
        };
        out.push(Instruction {
            offset: p,
            op,
            operand,
        });
        p += op_length(code, p)?;
    }
    Ok(out)
}

/// Port of mod_entity_facts.is_exit.
pub fn is_exit(by_offset: &HashMap<usize, Instruction>, target: i64) -> bool {
    let at = |t: i64| usize::try_from(t).ok().and_then(|t| by_offset.get(&t));
    let mut ins = at(target);
    if let Some(i) = ins
        && i.op == GOTO
    {
        ins = i.operand.and_then(at);
    }
    ins.is_some_and(|i| i.op == RETURN)
}

/// Port of mod_entity_facts.is_aload.
pub fn is_aload(op: u8) -> bool {
    op == 0x19 || (0x2a..=0x2d).contains(&op)
}

/// Port of mod_entity_facts.is_astore.
pub fn is_astore(op: u8) -> bool {
    op == 0x3a || (0x4b..=0x4e).contains(&op)
}

/// Port of mod_entity_facts.slot_of_last.
pub fn slot_of_last(desc: &str, is_static: bool) -> i64 {
    let mut slot = if is_static { 0 } else { 1 };
    let all = params(desc);
    for t in all.iter().take(all.len().saturating_sub(1)) {
        slot += if t == "J" || t == "D" { 2 } else { 1 };
    }
    slot
}

/// Port of mod_entity_facts.read_library_class.
pub fn read_library_class(jars: &[PathBuf], name: &str) -> Result<Option<Vec<u8>>, String> {
    let entry = format!("{name}.class");
    for jar in jars {
        let file = File::open(jar).map_err(|e| format!("{}: {e}", jar.display()))?;
        let mut zip = zip::ZipArchive::new(file).map_err(|e| format!("{}: {e}", jar.display()))?;
        if let Ok(mut found) = zip.by_name(&entry) {
            let mut data = Vec::new();
            found
                .read_to_end(&mut data)
                .map_err(|e| format!("{}!{entry}: {e}", jar.display()))?;
            return Ok(Some(data));
        }
    }
    Ok(None)
}

/// Port of mod_entity_facts.clinit.
pub fn clinit(data: &[u8]) -> Result<(Pool, Vec<Instruction>), ClassError> {
    let info: ClassInfo = parse_class(
        data,
        Parse {
            members: true,
            annotations: false,
            code: true,
        },
    )?;
    let code = info
        .methods
        .iter()
        .find(|m| m.name == "<clinit>" && m.code.is_some())
        .and_then(|m| m.code.as_deref());
    let ins = match code {
        Some(code) => instructions(code)?,
        None => Vec::new(),
    };
    Ok((info.pool, ins))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instructions_carry_locals_and_absolute_branch_targets() {
        let code = [0x2a, 0x4c, 0x99, 0x00, 0x04, 0xb1, 0xb1];
        let got: Vec<_> = instructions(&code)
            .unwrap()
            .iter()
            .map(|i| (i.offset, i.op, i.operand))
            .collect();
        assert_eq!(
            got,
            [
                (0, 0x2a, Some(0)),
                (1, 0x4c, Some(1)),
                (2, 0x99, Some(6)),
                (5, 0xb1, None),
                (6, 0xb1, None)
            ]
        );
    }

    #[test]
    fn a_goto_to_a_return_is_an_exit() {
        let by_offset: HashMap<usize, Instruction> = instructions(&[0xa7, 0x00, 0x03, 0xb1])
            .unwrap()
            .into_iter()
            .map(|i| (i.offset, i))
            .collect();
        assert!(is_exit(&by_offset, 0));
        assert!(!is_exit(&by_offset, 99));
    }

    #[test]
    fn the_event_slot_follows_wide_parameters() {
        assert_eq!(slot_of_last("(JLa/B;)V", true), 2);
        assert_eq!(slot_of_last("(La/B;)V", false), 1);
    }
}
