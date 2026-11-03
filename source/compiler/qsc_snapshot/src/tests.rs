// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use capnp::message::{Builder, HeapAllocator};

use crate::{
    Budget, Error, Format, Kind, MAX_DEPTH, MAX_GRAPH_WORDS, MAX_SNAPSHOT_BYTES, MAX_WIRE_DEPTH,
    MODEL_VERSION, binary, header, payload, snapshot_capnp as wire, text,
};

fn fixture(segment_words: u32) -> binary::Message {
    let mut message = Builder::new(HeapAllocator::new().first_segment_words(segment_words));
    let mut root = message.init_root::<wire::document::Builder>();
    root.set_version(MODEL_VERSION);
    let mut store = root.init_store();
    store.set_version(1);
    store.set_qubit_count(2);
    let mut instructions = store.init_instructions(2);
    for index in 0..2 {
        let mut instruction = instructions.reborrow().get(index);
        instruction
            .reborrow()
            .init_operation()
            .set_standard(wire::StandardGate::H);
        instruction.init_qubits(1).set(0, index);
    }
    message
}

fn text_fixture() -> String {
    let mut writer = text::Writer::new(Kind::CircuitStore).expect("writer");
    writer.open("store").expect("store");
    writer.open("name").expect("field");
    writer
        .string("quoted \" ( # ; \n\u{0}\u{1f600}")
        .expect("string");
    writer.close().expect("field");
    writer.close().expect("store");
    writer.finish().expect("finish")
}

#[test]
fn typed_binary_round_trip_has_a_canonical_single_segment() {
    let bytes = binary::encode(Kind::CircuitStore, &fixture(1)).expect("encode");
    let (_, body) = payload(Kind::CircuitStore, &bytes).expect("envelope");
    let (reader, _) = binary::decode(Kind::CircuitStore, body).expect("decode");
    assert!(reader.is_canonical().expect("canonical"));
    let root = reader.get_root::<wire::document::Reader>().expect("root");
    let wire::document::Store(store) = root.which().expect("kind") else {
        panic!("store")
    };
    let store = store.expect("store");
    assert_eq!(store.get_qubit_count(), 2);
    assert_eq!(store.get_instructions().expect("instructions").len(), 2);
}

#[test]
fn different_allocations_produce_identical_canonical_storage() {
    assert_eq!(
        binary::encode(Kind::CircuitStore, &fixture(1)).expect("encode"),
        binary::encode(Kind::CircuitStore, &fixture(8192)).expect("encode"),
    );
}

#[test]
fn payload_projection_detects_a_dropped_field() {
    let first = fixture(32);
    let mut second = fixture(32);
    second
        .get_root::<wire::document::Builder>()
        .expect("root")
        .set_version(2);
    assert!(
        binary::check_projection::<wire::document::Owned>(
            first
                .get_root_as_reader::<wire::document::Reader>()
                .expect("root"),
            second
                .get_root_as_reader::<wire::document::Reader>()
                .expect("root"),
        )
        .is_err()
    );
}

#[test]
fn text_escapes_round_trip_without_reinterpreting_reader_syntax() {
    let text = text_fixture();
    let parsed = text::parse(Kind::CircuitStore, text.as_bytes()).expect("parse");
    let mut record = parsed.record("store").expect("record");
    assert_eq!(
        text::string(record.field("name").expect("field")).expect("string"),
        "quoted \" ( # ; \n\u{0}\u{1f600}"
    );
    record.finish().expect("no extra fields");
}

#[test]
fn a_duplicate_text_field_is_rejected() {
    let parsed =
        text::parse(Kind::Circuit, b"QDKCIRCT(\n(qdk 1 (circuit (x 1) (x 2)))").expect("syntax");
    assert!(parsed.record("circuit").is_err());
}

#[test]
fn an_unknown_text_field_is_rejected() {
    let parsed =
        text::parse(Kind::Circuit, b"QDKCIRCT(\n(qdk 1 (circuit (unknown 1)))").expect("syntax");
    assert!(parsed.record("circuit").expect("record").finish().is_err());
}

#[test]
fn a_missing_text_field_is_rejected() {
    let parsed = text::parse(Kind::Circuit, b"QDKCIRCT(\n(qdk 1 (circuit))").expect("syntax");
    assert!(
        parsed
            .record("circuit")
            .expect("record")
            .field("missing")
            .is_err()
    );
}

#[test]
fn wrong_document_magic_is_rejected() {
    let bytes = binary::encode(Kind::CircuitStore, &fixture(32)).expect("encode");
    assert_eq!(payload(Kind::Graph, &bytes), Err(Error::Magic));
}

#[test]
fn wrong_root_kind_is_rejected() {
    assert!(binary::encode(Kind::Circuit, &fixture(32)).is_err());
}

#[test]
fn invalid_root_is_rejected() {
    let mut message = binary::Message::new_default();
    message
        .init_root::<wire::document::Builder>()
        .set_version(1);
    assert!(binary::encode(Kind::Graph, &message).is_err());
}

#[test]
fn a_pointer_to_a_missing_segment_is_rejected() {
    // One segment with one word: a far pointer to nonexistent segment 1.
    let packed = [0x10, 0x01, 0x11, 0x02, 0x01];
    assert!(binary::decode(Kind::Graph, &packed).is_err());
}

#[test]
fn improper_text_lists_are_rejected() {
    assert!(text::parse(Kind::Graph, b"QDKQHDG!(\n(qdk 1 (graph (x . y)))").is_err());
}

#[test]
fn unknown_format_is_rejected() {
    let mut bytes = header(Kind::Graph, Format::Binary);
    bytes[8] = 99;
    assert_eq!(payload(Kind::Graph, &bytes), Err(Error::Format(99)));
}

#[test]
fn unknown_flags_are_rejected() {
    let mut bytes = header(Kind::Graph, Format::Binary);
    bytes[9] = 0;
    assert_eq!(payload(Kind::Graph, &bytes), Err(Error::Flags(0)));
}

#[test]
fn unsupported_binary_version_is_rejected() {
    let mut message = fixture(32);
    message
        .get_root::<wire::document::Builder>()
        .expect("root")
        .set_version(99);
    assert_eq!(
        binary::encode(Kind::CircuitStore, &message),
        Err(Error::ModelVersion(99))
    );
}

#[test]
fn unsupported_text_version_is_rejected() {
    let parsed = text::parse(Kind::Graph, b"QDKQHDG!(\n(qdk 99 (graph))").expect("syntax");
    assert!(matches!(
        parsed.record("graph"),
        Err(Error::ModelVersion(99))
    ));
}

#[test]
fn truncated_packed_data_is_rejected() {
    let bytes = binary::encode(Kind::CircuitStore, &fixture(32)).expect("encode");
    let (_, body) = payload(Kind::CircuitStore, &bytes).expect("envelope");
    assert!(binary::decode(Kind::CircuitStore, &body[..body.len() - 1]).is_err());
}

#[test]
fn trailing_binary_data_is_rejected() {
    let mut bytes = binary::encode(Kind::CircuitStore, &fixture(32)).expect("encode");
    bytes.push(0);
    let (_, body) = payload(Kind::CircuitStore, &bytes).expect("envelope");
    assert!(binary::decode(Kind::CircuitStore, body).is_err());
}

#[test]
fn trailing_text_data_is_rejected() {
    assert!(text::parse(Kind::Graph, b"QDKQHDG!(\n(qdk 1 (graph)) (extra)").is_err());
}

#[test]
fn exactly_the_graph_encoded_limit_is_accepted_by_framing() {
    let mut bytes = header(Kind::Graph, Format::Binary);
    bytes.resize(MAX_SNAPSHOT_BYTES, 0);
    assert!(payload(Kind::Graph, &bytes).is_ok());
}

#[test]
fn one_byte_over_the_graph_limit_is_rejected_by_framing() {
    let mut bytes = header(Kind::Graph, Format::Binary);
    bytes.resize(MAX_SNAPSHOT_BYTES + 1, 0);
    assert_eq!(payload(Kind::Graph, &bytes), Err(Error::ByteLimit));
}

#[test]
fn circuits_do_not_inherit_the_graph_encoded_limit() {
    let mut bytes = header(Kind::CircuitStore, Format::Binary);
    bytes.resize(MAX_SNAPSHOT_BYTES + 1, 0);
    assert!(payload(Kind::CircuitStore, &bytes).is_ok());
}

#[test]
fn excessive_declared_expansion_is_rejected_before_segment_allocation() {
    // Packed segment table: zero segment-count-minus-one, then limit+1 words.
    let declared = u32::try_from(MAX_GRAPH_WORDS + 1).expect("fits");
    let word = (u64::from(declared) << 32).to_le_bytes();
    let mut packed = vec![0_u8];
    for (index, byte) in word.into_iter().enumerate() {
        if byte != 0 {
            packed[0] |= 1 << index;
            packed.push(byte);
        }
    }
    assert!(binary::decode(Kind::Graph, &packed).is_err());
}

#[test]
fn a_highly_compressible_graph_message_reaches_the_expanded_limit() {
    let mut message = Builder::new(
        HeapAllocator::new().first_segment_words(u32::try_from(MAX_GRAPH_WORDS).expect("fits")),
    );
    let mut root = message.init_root::<wire::document::Builder>();
    root.set_version(1);
    // Root pointer, Document data/pointer, then GraphSnapshot's two pointers.
    root.init_graph()
        .init_digest(u32::try_from((MAX_GRAPH_WORDS - 5) * 8).expect("fits"));
    assert_eq!(
        message.get_segments_for_output()[0].len() / 8,
        MAX_GRAPH_WORDS
    );
    let mut packed = Vec::new();
    capnp::serialize_packed::write_message(&mut packed, &message).expect("pack");
    assert!(packed.len() < MAX_SNAPSHOT_BYTES);
    let (reader, _) = binary::decode(Kind::Graph, &packed).expect("exact expanded boundary");
    assert_eq!(reader.size_in_words(), MAX_GRAPH_WORDS);
}

#[test]
fn the_text_writer_preserves_extreme_real_bits() {
    let mut writer = text::Writer::new(Kind::CircuitStore).expect("writer");
    writer.open("store").expect("record");
    writer
        .field("phase", |writer| writer.real(1))
        .expect("smallest subnormal");
    writer.close().expect("record");
    let text = writer.finish().expect("finish");
    let parsed = text::parse(Kind::CircuitStore, text.as_bytes()).expect("parse");
    let mut record = parsed.record("store").expect("record");
    assert_eq!(
        text::real(record.field("phase").expect("field")).expect("real"),
        1
    );
    record.finish().expect("fields");
}

#[test]
fn item_budget_refuses_zero_size_list_amplification() {
    let mut budget = Budget::new(4);
    budget.charge(4).expect("exact");
    assert_eq!(budget.charge(1), Err(Error::ItemLimit));
}

#[test]
fn semantic_depth_boundary_is_exact() {
    crate::depth(MAX_DEPTH).expect("exact");
    assert_eq!(crate::depth(MAX_DEPTH + 1), Err(Error::DepthLimit));
}

#[test]
fn excessive_text_depth_is_rejected_before_parser_recursion() {
    let bytes = format!("QDKQHDG!(\n{}", "(".repeat(MAX_WIRE_DEPTH + 1));
    assert!(matches!(
        text::parse(Kind::Graph, bytes.as_bytes()),
        Err(Error::DepthLimit)
    ));
}

#[test]
fn reader_extensions_cannot_hide_recursion() {
    assert!(text::parse(Kind::Graph, b"QDKQHDG!(\n''''''x").is_err());
}

fn read_real(input: &str) -> u64 {
    let parsed = text::parse(
        Kind::Circuit,
        format!("QDKCIRCT(\n(qdk 1 (circuit (x {input})))").as_bytes(),
    )
    .expect("syntax");
    text::real(
        parsed
            .record("circuit")
            .expect("record")
            .field("x")
            .expect("field"),
    )
    .expect("real")
}

#[test]
fn negative_zero_real_preserves_its_sign() {
    assert_eq!(read_real("-0.0"), (-0.0_f64).to_bits());
}

#[test]
fn nan_bit_form_preserves_its_payload() {
    assert_eq!(
        read_real("(real64-bits \"fff8123456789000\")"),
        0xfff8_1234_5678_9000
    );
}

#[test]
fn the_smallest_subnormal_real_round_trips() {
    assert_eq!(read_real("5.0e-324"), 1);
}

#[test]
fn the_largest_finite_real_round_trips() {
    assert_eq!(read_real("1.7976931348623157e308"), f64::MAX.to_bits());
}

#[test]
fn an_inexact_decimal_real_keeps_its_nearest_bit_pattern() {
    assert_eq!(read_real("0.3333333333333333"), (1.0_f64 / 3.0).to_bits());
}
