// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::*;
use expect_test::expect;

#[test]
fn fixed_bits_round_trip() {
    let mut w = BitstreamWriter::new();
    w.emit_bits(1, 1);
    w.emit_bits(0xAB, 8);
    w.emit_bits(0x1234, 16);
    w.emit_bits(0xDEAD_BEEF, 32);
    w.emit_bits(0x0123_4567_89AB_CDEF, 64);
    let data = w.finish();
    let mut r = BitstreamReader::new(&data);
    expect!["1"].assert_eq(&r.read_bits(1).to_string());
    expect!["171"].assert_eq(&r.read_bits(8).to_string()); // 0xAB
    expect!["4660"].assert_eq(&r.read_bits(16).to_string()); // 0x1234
    expect!["3735928559"].assert_eq(&r.read_bits(32).to_string()); // 0xDEADBEEF
    expect!["81985529216486895"].assert_eq(&r.read_bits(64).to_string());
}

#[test]
fn vbr_round_trip_small() {
    let mut w = BitstreamWriter::new();
    w.emit_vbr(0, 6);
    w.emit_vbr(1, 6);
    w.emit_vbr(31, 6);
    let data = w.finish();
    let mut r = BitstreamReader::new(&data);
    expect!["0"].assert_eq(&r.read_vbr(6).to_string());
    expect!["1"].assert_eq(&r.read_vbr(6).to_string());
    expect!["31"].assert_eq(&r.read_vbr(6).to_string());
}

#[test]
fn vbr_round_trip_large() {
    let mut w = BitstreamWriter::new();
    w.emit_vbr(127, 6);
    w.emit_vbr(255, 6);
    w.emit_vbr(1023, 6);
    w.emit_vbr(u64::from(u32::MAX), 6);
    let data = w.finish();
    let mut r = BitstreamReader::new(&data);
    expect!["127"].assert_eq(&r.read_vbr(6).to_string());
    expect!["255"].assert_eq(&r.read_vbr(6).to_string());
    expect!["1023"].assert_eq(&r.read_vbr(6).to_string());
    expect!["4294967295"].assert_eq(&r.read_vbr(6).to_string());
}

#[test]
fn align32_at_boundary() {
    let mut w = BitstreamWriter::new();
    w.emit_bits(0xAABBCCDD, 32);
    let len_before = w.buffer.len();
    w.align32();
    expect!["4"].assert_eq(&len_before.to_string());
    expect!["4"].assert_eq(&w.buffer.len().to_string());
}

#[test]
fn align32_pads_correctly() {
    // 1 byte partial
    let mut w = BitstreamWriter::new();
    w.emit_bits(0xFF, 8);
    w.align32();
    expect!["4"].assert_eq(&w.buffer.len().to_string());

    // 3 bits partial
    let mut w = BitstreamWriter::new();
    w.emit_bits(0b101, 3);
    w.align32();
    expect!["4"].assert_eq(&w.buffer.len().to_string());

    // 2 bytes
    let mut w = BitstreamWriter::new();
    w.emit_bits(0xFFFF, 16);
    w.align32();
    expect!["4"].assert_eq(&w.buffer.len().to_string());

    // 5 bytes
    let mut w = BitstreamWriter::new();
    w.emit_bits(0xFF, 8);
    w.emit_bits(0xFFFF_FFFF, 32);
    w.align32();
    expect!["8"].assert_eq(&w.buffer.len().to_string());
}

#[test]
fn block_enter_exit_round_trip() {
    let abbrev_width = 2;
    let mut w = BitstreamWriter::new();
    w.enter_subblock(8, 4, abbrev_width);
    w.exit_block(4);
    let data = w.finish();

    let mut r = BitstreamReader::new(&data);
    let id = r.read_abbrev_id(abbrev_width);
    expect!["1"].assert_eq(&id.to_string()); // ENTER_SUBBLOCK

    let (block_id, new_abbrev, block_len) = r.enter_subblock();
    expect!["8"].assert_eq(&block_id.to_string());
    expect!["4"].assert_eq(&new_abbrev.to_string());

    // The block content should contain only the END_BLOCK + alignment
    let end_pos = r.byte_position() + block_len * 4;
    let end_id = r.read_abbrev_id(new_abbrev);
    expect!["0"].assert_eq(&end_id.to_string()); // END_BLOCK
    r.align32();
    expect!["true"].assert_eq(&(r.byte_position() == end_pos).to_string());
}

#[test]
fn nested_blocks_round_trip() {
    let outer_aw = 2;
    let inner_aw = 3;
    let leaf_aw = 4;

    let mut w = BitstreamWriter::new();
    w.enter_subblock(10, inner_aw, outer_aw);
    w.enter_subblock(20, leaf_aw, inner_aw);
    w.exit_block(leaf_aw);
    w.exit_block(inner_aw);
    let data = w.finish();

    let mut r = BitstreamReader::new(&data);
    // Outer block
    let id = r.read_abbrev_id(outer_aw);
    expect!["1"].assert_eq(&id.to_string());
    let (bid, aw, _blen) = r.enter_subblock();
    expect!["10"].assert_eq(&bid.to_string());
    expect!["3"].assert_eq(&aw.to_string());

    // Inner block
    let id = r.read_abbrev_id(aw);
    expect!["1"].assert_eq(&id.to_string());
    let (bid2, aw2, _blen2) = r.enter_subblock();
    expect!["20"].assert_eq(&bid2.to_string());
    expect!["4"].assert_eq(&aw2.to_string());

    // END inner
    let end = r.read_abbrev_id(aw2);
    expect!["0"].assert_eq(&end.to_string());
    r.align32();

    // END outer
    let end = r.read_abbrev_id(aw);
    expect!["0"].assert_eq(&end.to_string());
    r.align32();
    expect!["true"].assert_eq(&r.at_end().to_string());
}

#[test]
fn unabbrev_record_round_trip() {
    let aw = 4;
    let mut w = BitstreamWriter::new();
    w.emit_record(7, &[100, 200, 300], aw);
    let data = w.finish();

    let mut r = BitstreamReader::new(&data);
    let id = r.read_abbrev_id(aw);
    expect!["3"].assert_eq(&id.to_string()); // UNABBREV_RECORD
    let (code, vals) = r.read_unabbrev_record();
    expect!["7"].assert_eq(&code.to_string());
    expect!["[100, 200, 300]"].assert_eq(&format!("{vals:?}"));
}

#[test]
fn multiple_records_in_block() {
    let aw = 4;
    let mut w = BitstreamWriter::new();
    w.enter_subblock(5, aw, 2);
    w.emit_record(1, &[10, 20], aw);
    w.emit_record(2, &[30], aw);
    w.emit_record(3, &[], aw);
    w.exit_block(aw);
    let data = w.finish();

    let mut r = BitstreamReader::new(&data);
    // ENTER_SUBBLOCK
    let id = r.read_abbrev_id(2);
    expect!["1"].assert_eq(&id.to_string());
    let (bid, new_aw, _blen) = r.enter_subblock();
    expect!["5"].assert_eq(&bid.to_string());
    expect!["4"].assert_eq(&new_aw.to_string());

    // Record 1
    let id = r.read_abbrev_id(new_aw);
    expect!["3"].assert_eq(&id.to_string());
    let (code, vals) = r.read_unabbrev_record();
    expect!["1"].assert_eq(&code.to_string());
    expect!["[10, 20]"].assert_eq(&format!("{vals:?}"));

    // Record 2
    let id = r.read_abbrev_id(new_aw);
    expect!["3"].assert_eq(&id.to_string());
    let (code, vals) = r.read_unabbrev_record();
    expect!["2"].assert_eq(&code.to_string());
    expect!["[30]"].assert_eq(&format!("{vals:?}"));

    // Record 3
    let id = r.read_abbrev_id(new_aw);
    expect!["3"].assert_eq(&id.to_string());
    let (code, vals) = r.read_unabbrev_record();
    expect!["3"].assert_eq(&code.to_string());
    expect!["[]"].assert_eq(&format!("{vals:?}"));

    // END_BLOCK
    let id = r.read_abbrev_id(new_aw);
    expect!["0"].assert_eq(&id.to_string());
    r.align32();
    expect!["true"].assert_eq(&r.at_end().to_string());
}

#[test]
fn patch_u32_bits_updates_non_byte_aligned_field() {
    let mut w = BitstreamWriter::new();
    w.emit_bits(0b101, 3);
    let patch_position = w.bit_position();
    w.emit_bits(0, 32);
    w.emit_bits(0b11, 2);
    w.patch_u32_bits(patch_position, 0xDEAD_BEEF);

    let data = w.finish();
    let mut r = BitstreamReader::new(&data);
    expect!["5"].assert_eq(&r.read_bits(3).to_string());
    expect!["3735928559"].assert_eq(&r.read_bits(32).to_string());
    expect!["3"].assert_eq(&r.read_bits(2).to_string());
}

#[test]
fn char6_decode_table() {
    // a-z
    expect!["a"].assert_eq(&(decode_char6(0) as char).to_string());
    expect!["z"].assert_eq(&(decode_char6(25) as char).to_string());
    // A-Z
    expect!["A"].assert_eq(&(decode_char6(26) as char).to_string());
    expect!["Z"].assert_eq(&(decode_char6(51) as char).to_string());
    // 0-9
    expect!["0"].assert_eq(&(decode_char6(52) as char).to_string());
    expect!["9"].assert_eq(&(decode_char6(61) as char).to_string());
    // special
    expect!["."].assert_eq(&(decode_char6(62) as char).to_string());
    expect!["_"].assert_eq(&(decode_char6(63) as char).to_string());
}

#[test]
fn abbrev_fixed_round_trip() {
    let mut w = BitstreamWriter::new();
    // Emit a DEFINE_ABBREV definition manually:
    // 2 operands: Literal(7), Fixed(8)
    w.emit_vbr(2, 5); // num_ops = 2
    w.emit_bits(1, 1); // op1: is_literal = true
    w.emit_vbr(7, 8); // op1: value = 7
    w.emit_bits(0, 1); // op2: is_literal = false
    w.emit_bits(1, 3); // op2: encoding = Fixed
    w.emit_vbr(8, 5); // op2: width = 8
    // Emit an abbreviated record using that abbreviation:
    // code=7 (from literal), value=0xAB (Fixed 8 bits)
    w.emit_bits(0xAB, 8);
    let data = w.finish();

    let mut r = BitstreamReader::new(&data);
    r.push_block_scope(0);
    r.read_define_abbrev()
        .expect("read_define_abbrev should succeed");

    let (code, values) = r
        .read_abbreviated_record(4)
        .expect("read_abbreviated_record should succeed");
    expect!["7"].assert_eq(&code.to_string());
    expect!["[171]"].assert_eq(&format!("{values:?}")); // 0xAB = 171
    r.pop_block_scope();
}

#[test]
fn abbrev_vbr_round_trip() {
    let mut w = BitstreamWriter::new();
    // 2 operands: Literal(5), VBR(6)
    w.emit_vbr(2, 5); // num_ops = 2
    w.emit_bits(1, 1); // op1: is_literal
    w.emit_vbr(5, 8); // op1: value = 5
    w.emit_bits(0, 1); // op2: not literal
    w.emit_bits(2, 3); // op2: encoding = VBR
    w.emit_vbr(6, 5); // op2: chunk_width = 6
    // Emit record: code=5 (literal), value=12345 (VBR6)
    w.emit_vbr(12345, 6);
    let data = w.finish();

    let mut r = BitstreamReader::new(&data);
    r.push_block_scope(0);
    r.read_define_abbrev()
        .expect("read_define_abbrev should succeed");

    let (code, values) = r
        .read_abbreviated_record(4)
        .expect("read_abbreviated_record should succeed");
    expect!["5"].assert_eq(&code.to_string());
    expect!["[12345]"].assert_eq(&format!("{values:?}"));
    r.pop_block_scope();
}

#[test]
fn abbrev_array_char6_round_trip() {
    let mut w = BitstreamWriter::new();
    // 3 operands: Literal(19), Array, Char6 (element)
    // LLVM counts the Array and its element encoding as separate operands
    w.emit_vbr(3, 5); // num_ops = 3
    w.emit_bits(1, 1); // op1: is_literal
    w.emit_vbr(19, 8); // op1: value = 19 (TYPE_CODE_STRUCT_NAME)
    w.emit_bits(0, 1); // op2: not literal
    w.emit_bits(3, 3); // op2: encoding = Array
    // Array element encoding: Char6
    w.emit_bits(0, 1); // not literal
    w.emit_bits(4, 3); // encoding = Char6
    // Emit record: code=19, array of Char6 spelling "Hello"
    w.emit_vbr(5, 6); // array length = 5
    // Char6 encoding: H=33 (26+7), e=4, l=11, l=11, o=14
    w.emit_bits(33, 6); // H
    w.emit_bits(4, 6); // e
    w.emit_bits(11, 6); // l
    w.emit_bits(11, 6); // l
    w.emit_bits(14, 6); // o
    let data = w.finish();

    let mut r = BitstreamReader::new(&data);
    r.push_block_scope(0);
    r.read_define_abbrev()
        .expect("read_define_abbrev should succeed");

    let (code, values) = r
        .read_abbreviated_record(4)
        .expect("read_abbreviated_record should succeed");
    expect!["19"].assert_eq(&code.to_string());
    let s: String = values.iter().map(|&v| v as u8 as char).collect();
    expect!["Hello"].assert_eq(&s);
    r.pop_block_scope();
}

#[test]
fn abbrev_blockinfo_propagation() {
    let mut w = BitstreamWriter::new();
    // Emit a DEFINE_ABBREV for blockinfo
    w.emit_vbr(2, 5); // num_ops = 2
    w.emit_bits(1, 1); // literal
    w.emit_vbr(42, 8); // code = 42
    w.emit_bits(0, 1); // not literal
    w.emit_bits(1, 3); // Fixed
    w.emit_vbr(16, 5); // width = 16
    let data = w.finish();

    let mut r = BitstreamReader::new(&data);
    // Store abbreviation in blockinfo for block type 8
    r.push_block_scope(0); // dummy scope for reading
    r.read_blockinfo_abbrev(8)
        .expect("read_blockinfo_abbrev should succeed");
    r.pop_block_scope();

    // Now push scope for block 8 — should inherit the abbreviation
    r.push_block_scope(8);
    // Emit a record using inherited abbreviation (id=4)
    let mut w2 = BitstreamWriter::new();
    w2.emit_bits(0x1234, 16); // Fixed(16)
    let data2 = w2.finish();

    let mut r2 = BitstreamReader::new(&data2);
    r2.blockinfo_abbrevs = r.blockinfo_abbrevs;
    r2.push_block_scope(8);
    let (code, values) = r2
        .read_abbreviated_record(4)
        .expect("read_abbreviated_record should succeed");
    expect!["42"].assert_eq(&code.to_string());
    expect!["[4660]"].assert_eq(&format!("{values:?}")); // 0x1234 = 4660
    r2.pop_block_scope();
}

#[test]
fn multiple_abbrevs_in_scope() {
    let mut w = BitstreamWriter::new();
    // Abbrev 1 (id=4): Literal(1), Fixed(8)
    w.emit_vbr(2, 5);
    w.emit_bits(1, 1);
    w.emit_vbr(1, 8);
    w.emit_bits(0, 1);
    w.emit_bits(1, 3);
    w.emit_vbr(8, 5);
    // Abbrev 2 (id=5): Literal(2), Fixed(16)
    w.emit_vbr(2, 5);
    w.emit_bits(1, 1);
    w.emit_vbr(2, 8);
    w.emit_bits(0, 1);
    w.emit_bits(1, 3);
    w.emit_vbr(16, 5);
    // Record using abbrev 1 (id=4)
    w.emit_bits(0xFF, 8);
    // Record using abbrev 2 (id=5)
    w.emit_bits(0xBEEF, 16);
    let data = w.finish();

    let mut r = BitstreamReader::new(&data);
    r.push_block_scope(0);
    r.read_define_abbrev().expect("first abbrev should succeed");
    r.read_define_abbrev()
        .expect("second abbrev should succeed");

    let (code1, vals1) = r
        .read_abbreviated_record(4)
        .expect("record 1 should succeed");
    expect!["1"].assert_eq(&code1.to_string());
    expect!["[255]"].assert_eq(&format!("{vals1:?}"));

    let (code2, vals2) = r
        .read_abbreviated_record(5)
        .expect("record 2 should succeed");
    expect!["2"].assert_eq(&code2.to_string());
    expect!["[48879]"].assert_eq(&format!("{vals2:?}")); // 0xBEEF
    r.pop_block_scope();
}

#[test]
fn writer_define_abbrev_fixed_round_trip() {
    let abbrev = AbbrevDef {
        operands: vec![AbbrevOperand::Literal(7), AbbrevOperand::Fixed(8)],
    };
    let aw = 4;
    let mut w = BitstreamWriter::new();
    let id = w.emit_define_abbrev(&abbrev, aw);
    expect!["4"].assert_eq(&id.to_string());
    w.emit_abbreviated_record(id, &[0xAB], aw);
    let data = w.finish();

    let mut r = BitstreamReader::new(&data);
    r.push_block_scope(0);
    let aid = r.read_abbrev_id(aw);
    expect!["2"].assert_eq(&aid.to_string()); // DEFINE_ABBREV
    r.read_define_abbrev()
        .expect("read_define_abbrev should succeed");
    let aid2 = r.read_abbrev_id(aw);
    expect!["4"].assert_eq(&aid2.to_string());
    let (code, values) = r
        .read_abbreviated_record(4)
        .expect("read_abbreviated_record should succeed");
    expect!["7"].assert_eq(&code.to_string());
    expect!["[171]"].assert_eq(&format!("{values:?}")); // 0xAB = 171
    r.pop_block_scope();
}

#[test]
fn writer_define_abbrev_vbr_round_trip() {
    let abbrev = AbbrevDef {
        operands: vec![AbbrevOperand::Literal(5), AbbrevOperand::Vbr(6)],
    };
    let aw = 4;
    let mut w = BitstreamWriter::new();
    let id = w.emit_define_abbrev(&abbrev, aw);
    w.emit_abbreviated_record(id, &[12345], aw);
    let data = w.finish();

    let mut r = BitstreamReader::new(&data);
    r.push_block_scope(0);
    let aid = r.read_abbrev_id(aw);
    expect!["2"].assert_eq(&aid.to_string());
    r.read_define_abbrev().expect("should succeed");
    let aid2 = r.read_abbrev_id(aw);
    expect!["4"].assert_eq(&aid2.to_string());
    let (code, values) = r.read_abbreviated_record(4).expect("should succeed");
    expect!["5"].assert_eq(&code.to_string());
    expect!["[12345]"].assert_eq(&format!("{values:?}"));
    r.pop_block_scope();
}

#[test]
fn writer_define_abbrev_char6_array_round_trip() {
    // Abbreviation: Literal(19), Array(Char6) — struct name style
    let abbrev = AbbrevDef {
        operands: vec![
            AbbrevOperand::Literal(19),
            AbbrevOperand::Array(Box::new(AbbrevOperand::Char6)),
        ],
    };
    let aw = 4;
    let mut w = BitstreamWriter::new();
    let id = w.emit_define_abbrev(&abbrev, aw);
    // "Hello" as character values: H=72, e=101, l=108, l=108, o=111
    let fields: Vec<u64> = vec![5, 72, 101, 108, 108, 111]; // [count, H, e, l, l, o]
    w.emit_abbreviated_record(id, &fields, aw);
    let data = w.finish();

    let mut r = BitstreamReader::new(&data);
    r.push_block_scope(0);
    let aid = r.read_abbrev_id(aw);
    expect!["2"].assert_eq(&aid.to_string());
    r.read_define_abbrev().expect("should succeed");
    let aid2 = r.read_abbrev_id(aw);
    expect!["4"].assert_eq(&aid2.to_string());
    let (code, values) = r.read_abbreviated_record(4).expect("should succeed");
    expect!["19"].assert_eq(&code.to_string());
    let s: String = values.iter().map(|&v| v as u8 as char).collect();
    expect!["Hello"].assert_eq(&s);
    r.pop_block_scope();
}

#[test]
fn writer_multiple_abbrevs_round_trip() {
    let abbrev1 = AbbrevDef {
        operands: vec![AbbrevOperand::Literal(1), AbbrevOperand::Fixed(8)],
    };
    let abbrev2 = AbbrevDef {
        operands: vec![AbbrevOperand::Literal(2), AbbrevOperand::Fixed(16)],
    };
    let aw = 4;
    let mut w = BitstreamWriter::new();
    let id1 = w.emit_define_abbrev(&abbrev1, aw);
    let id2 = w.emit_define_abbrev(&abbrev2, aw);
    expect!["4"].assert_eq(&id1.to_string());
    expect!["5"].assert_eq(&id2.to_string());
    w.emit_abbreviated_record(id1, &[0xFF], aw);
    w.emit_abbreviated_record(id2, &[0xBEEF], aw);
    let data = w.finish();

    let mut r = BitstreamReader::new(&data);
    r.push_block_scope(0);
    // Read two DEFINE_ABBREVs
    let a1 = r.read_abbrev_id(aw);
    expect!["2"].assert_eq(&a1.to_string());
    r.read_define_abbrev().expect("first abbrev");
    let a2 = r.read_abbrev_id(aw);
    expect!["2"].assert_eq(&a2.to_string());
    r.read_define_abbrev().expect("second abbrev");
    // Read two abbreviated records
    let a3 = r.read_abbrev_id(aw);
    expect!["4"].assert_eq(&a3.to_string());
    let (code1, vals1) = r.read_abbreviated_record(4).expect("record 1");
    expect!["1"].assert_eq(&code1.to_string());
    expect!["[255]"].assert_eq(&format!("{vals1:?}"));
    let a4 = r.read_abbrev_id(aw);
    expect!["5"].assert_eq(&a4.to_string());
    let (code2, vals2) = r.read_abbreviated_record(5).expect("record 2");
    expect!["2"].assert_eq(&code2.to_string());
    expect!["[48879]"].assert_eq(&format!("{vals2:?}"));
    r.pop_block_scope();
}

#[test]
fn writer_abbrev_in_block_round_trip() {
    let outer_aw = 2;
    let block_aw = 4;
    let abbrev = AbbrevDef {
        operands: vec![
            AbbrevOperand::Literal(7),
            AbbrevOperand::Fixed(8),
            AbbrevOperand::Vbr(6),
        ],
    };
    let mut w = BitstreamWriter::new();
    w.enter_subblock(8, block_aw, outer_aw);
    let id = w.emit_define_abbrev(&abbrev, block_aw);
    expect!["4"].assert_eq(&id.to_string());
    w.emit_abbreviated_record(id, &[0xAB, 999], block_aw);
    w.emit_record(3, &[42], block_aw); // unabbreviated mixed in
    w.exit_block(block_aw);
    let data = w.finish();

    let mut r = BitstreamReader::new(&data);
    // ENTER_SUBBLOCK
    let aid = r.read_abbrev_id(outer_aw);
    expect!["1"].assert_eq(&aid.to_string());
    let (bid, new_aw, _blen) = r.enter_subblock();
    expect!["8"].assert_eq(&bid.to_string());
    expect!["4"].assert_eq(&new_aw.to_string());
    r.push_block_scope(bid);
    // DEFINE_ABBREV
    let aid = r.read_abbrev_id(new_aw);
    expect!["2"].assert_eq(&aid.to_string());
    r.read_define_abbrev().expect("define abbrev");
    // Abbreviated record
    let aid = r.read_abbrev_id(new_aw);
    expect!["4"].assert_eq(&aid.to_string());
    let (code, values) = r.read_abbreviated_record(4).expect("abbrev record");
    expect!["7"].assert_eq(&code.to_string());
    expect!["[171, 999]"].assert_eq(&format!("{values:?}"));
    // Unabbreviated record
    let aid = r.read_abbrev_id(new_aw);
    expect!["3"].assert_eq(&aid.to_string());
    let (code, vals) = r.read_unabbrev_record();
    expect!["3"].assert_eq(&code.to_string());
    expect!["[42]"].assert_eq(&format!("{vals:?}"));
    // END_BLOCK
    let aid = r.read_abbrev_id(new_aw);
    expect!["0"].assert_eq(&aid.to_string());
    r.pop_block_scope();
    r.align32();
    expect!["true"].assert_eq(&r.at_end().to_string());
}

#[test]
fn writer_abbrev_ids_reset_per_block() {
    let outer_aw = 2;
    let block_aw = 4;
    let abbrev = AbbrevDef {
        operands: vec![AbbrevOperand::Literal(1), AbbrevOperand::Fixed(8)],
    };
    let mut w = BitstreamWriter::new();
    // First block: define abbrev gets id=4
    w.enter_subblock(8, block_aw, outer_aw);
    let id1 = w.emit_define_abbrev(&abbrev, block_aw);
    expect!["4"].assert_eq(&id1.to_string());
    w.exit_block(block_aw);
    // Second block: define abbrev should also get id=4 (reset)
    w.enter_subblock(9, block_aw, outer_aw);
    let id2 = w.emit_define_abbrev(&abbrev, block_aw);
    expect!["4"].assert_eq(&id2.to_string());
    w.exit_block(block_aw);
    let _data = w.finish();
}
