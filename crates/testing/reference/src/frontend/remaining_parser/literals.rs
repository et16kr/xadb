//! Authored full arrays and exact diagnostics, independent of product code.
use super::{
    Case, Comparison, DataType, Direction, Kind, NameObservation, NullPlacement, Nullability,
    ScalarObservation, TypeLength, bad, good, node,
};

fn case_0000() -> Case {
    good(
        "select/options/0/0",
        "structural",
        b"SELECT a FROM t",
        vec![
            node(Kind::Select, [0, 15], Some(1), None),
            node(Kind::Projection, [7, 8], Some(2), Some(4)),
            node(Kind::ProjectionItem, [7, 8], Some(3), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [7, 8],
                }),
                [7, 8],
                None,
                None,
            ),
            node(Kind::Target, [14, 15], Some(5), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [14, 15],
                }),
                [14, 15],
                None,
                None,
            ),
        ],
        0,
    )
}

fn case_0001() -> Case {
    good(
        "select/options/0/1",
        "structural",
        b"SELECT a FROM t WHERE a=0",
        vec![
            node(Kind::Select, [0, 25], Some(1), None),
            node(Kind::Projection, [7, 8], Some(2), Some(4)),
            node(Kind::ProjectionItem, [7, 8], Some(3), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [7, 8],
                }),
                [7, 8],
                None,
                None,
            ),
            node(Kind::Target, [14, 15], Some(5), Some(6)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [14, 15],
                }),
                [14, 15],
                None,
                None,
            ),
            node(Kind::Where, [16, 25], Some(7), None),
            node(Kind::Or, [22, 25], Some(8), None),
            node(Kind::And, [22, 25], Some(9), None),
            node(Kind::Compare(Comparison::Eq), [22, 25], Some(10), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [22, 23],
                }),
                [22, 23],
                None,
                Some(11),
            ),
            node(
                Kind::Scalar(ScalarObservation::Integer {
                    sign: "None",
                    digits: [24, 25],
                }),
                [24, 25],
                None,
                None,
            ),
        ],
        0,
    )
}

fn case_0002() -> Case {
    good(
        "select/options/0/2",
        "structural",
        b"SELECT a FROM t ORDER BY b",
        vec![
            node(Kind::Select, [0, 26], Some(1), None),
            node(Kind::Projection, [7, 8], Some(2), Some(4)),
            node(Kind::ProjectionItem, [7, 8], Some(3), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [7, 8],
                }),
                [7, 8],
                None,
                None,
            ),
            node(Kind::Target, [14, 15], Some(5), Some(6)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [14, 15],
                }),
                [14, 15],
                None,
                None,
            ),
            node(Kind::OrderBy, [16, 26], Some(7), None),
            node(
                Kind::OrderTerm {
                    direction: None,
                    nulls: None,
                },
                [25, 26],
                Some(8),
                None,
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"B".to_vec(),
                    quoted: false,
                    span: [25, 26],
                }),
                [25, 26],
                None,
                None,
            ),
        ],
        0,
    )
}

fn case_0003() -> Case {
    good(
        "select/options/0/3",
        "structural",
        b"SELECT a FROM t WHERE a=0 ORDER BY b",
        vec![
            node(Kind::Select, [0, 36], Some(1), None),
            node(Kind::Projection, [7, 8], Some(2), Some(4)),
            node(Kind::ProjectionItem, [7, 8], Some(3), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [7, 8],
                }),
                [7, 8],
                None,
                None,
            ),
            node(Kind::Target, [14, 15], Some(5), Some(6)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [14, 15],
                }),
                [14, 15],
                None,
                None,
            ),
            node(Kind::Where, [16, 25], Some(7), Some(12)),
            node(Kind::Or, [22, 25], Some(8), None),
            node(Kind::And, [22, 25], Some(9), None),
            node(Kind::Compare(Comparison::Eq), [22, 25], Some(10), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [22, 23],
                }),
                [22, 23],
                None,
                Some(11),
            ),
            node(
                Kind::Scalar(ScalarObservation::Integer {
                    sign: "None",
                    digits: [24, 25],
                }),
                [24, 25],
                None,
                None,
            ),
            node(Kind::OrderBy, [26, 36], Some(13), None),
            node(
                Kind::OrderTerm {
                    direction: None,
                    nulls: None,
                },
                [35, 36],
                Some(14),
                None,
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"B".to_vec(),
                    quoted: false,
                    span: [35, 36],
                }),
                [35, 36],
                None,
                None,
            ),
        ],
        0,
    )
}

fn case_0004() -> Case {
    good(
        "select/options/0/4",
        "structural",
        b"SELECT a FROM t LIMIT ?",
        vec![
            node(Kind::Select, [0, 23], Some(1), None),
            node(Kind::Projection, [7, 8], Some(2), Some(4)),
            node(Kind::ProjectionItem, [7, 8], Some(3), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [7, 8],
                }),
                [7, 8],
                None,
                None,
            ),
            node(Kind::Target, [14, 15], Some(5), Some(6)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [14, 15],
                }),
                [14, 15],
                None,
                None,
            ),
            node(Kind::Limit, [16, 23], Some(7), None),
            node(
                Kind::Scalar(ScalarObservation::Parameter { ordinal: 1 }),
                [22, 23],
                None,
                None,
            ),
        ],
        1,
    )
}

fn case_0005() -> Case {
    good(
        "select/options/0/5",
        "structural",
        b"SELECT a FROM t WHERE a=0 LIMIT ?",
        vec![
            node(Kind::Select, [0, 33], Some(1), None),
            node(Kind::Projection, [7, 8], Some(2), Some(4)),
            node(Kind::ProjectionItem, [7, 8], Some(3), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [7, 8],
                }),
                [7, 8],
                None,
                None,
            ),
            node(Kind::Target, [14, 15], Some(5), Some(6)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [14, 15],
                }),
                [14, 15],
                None,
                None,
            ),
            node(Kind::Where, [16, 25], Some(7), Some(12)),
            node(Kind::Or, [22, 25], Some(8), None),
            node(Kind::And, [22, 25], Some(9), None),
            node(Kind::Compare(Comparison::Eq), [22, 25], Some(10), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [22, 23],
                }),
                [22, 23],
                None,
                Some(11),
            ),
            node(
                Kind::Scalar(ScalarObservation::Integer {
                    sign: "None",
                    digits: [24, 25],
                }),
                [24, 25],
                None,
                None,
            ),
            node(Kind::Limit, [26, 33], Some(13), None),
            node(
                Kind::Scalar(ScalarObservation::Parameter { ordinal: 1 }),
                [32, 33],
                None,
                None,
            ),
        ],
        1,
    )
}

fn case_0006() -> Case {
    good(
        "select/options/0/6",
        "structural",
        b"SELECT a FROM t ORDER BY b LIMIT ?",
        vec![
            node(Kind::Select, [0, 34], Some(1), None),
            node(Kind::Projection, [7, 8], Some(2), Some(4)),
            node(Kind::ProjectionItem, [7, 8], Some(3), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [7, 8],
                }),
                [7, 8],
                None,
                None,
            ),
            node(Kind::Target, [14, 15], Some(5), Some(6)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [14, 15],
                }),
                [14, 15],
                None,
                None,
            ),
            node(Kind::OrderBy, [16, 26], Some(7), Some(9)),
            node(
                Kind::OrderTerm {
                    direction: None,
                    nulls: None,
                },
                [25, 26],
                Some(8),
                None,
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"B".to_vec(),
                    quoted: false,
                    span: [25, 26],
                }),
                [25, 26],
                None,
                None,
            ),
            node(Kind::Limit, [27, 34], Some(10), None),
            node(
                Kind::Scalar(ScalarObservation::Parameter { ordinal: 1 }),
                [33, 34],
                None,
                None,
            ),
        ],
        1,
    )
}

fn case_0007() -> Case {
    good(
        "select/options/0/7",
        "structural",
        b"SELECT a FROM t WHERE a=0 ORDER BY b LIMIT ?",
        vec![
            node(Kind::Select, [0, 44], Some(1), None),
            node(Kind::Projection, [7, 8], Some(2), Some(4)),
            node(Kind::ProjectionItem, [7, 8], Some(3), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [7, 8],
                }),
                [7, 8],
                None,
                None,
            ),
            node(Kind::Target, [14, 15], Some(5), Some(6)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [14, 15],
                }),
                [14, 15],
                None,
                None,
            ),
            node(Kind::Where, [16, 25], Some(7), Some(12)),
            node(Kind::Or, [22, 25], Some(8), None),
            node(Kind::And, [22, 25], Some(9), None),
            node(Kind::Compare(Comparison::Eq), [22, 25], Some(10), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [22, 23],
                }),
                [22, 23],
                None,
                Some(11),
            ),
            node(
                Kind::Scalar(ScalarObservation::Integer {
                    sign: "None",
                    digits: [24, 25],
                }),
                [24, 25],
                None,
                None,
            ),
            node(Kind::OrderBy, [26, 36], Some(13), Some(15)),
            node(
                Kind::OrderTerm {
                    direction: None,
                    nulls: None,
                },
                [35, 36],
                Some(14),
                None,
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"B".to_vec(),
                    quoted: false,
                    span: [35, 36],
                }),
                [35, 36],
                None,
                None,
            ),
            node(Kind::Limit, [37, 44], Some(16), None),
            node(
                Kind::Scalar(ScalarObservation::Parameter { ordinal: 1 }),
                [43, 44],
                None,
                None,
            ),
        ],
        1,
    )
}

fn case_0008() -> Case {
    good(
        "select/options/1/0",
        "structural",
        b"SELECT * FROM t",
        vec![
            node(Kind::Select, [0, 15], Some(1), None),
            node(Kind::Projection, [7, 8], Some(2), Some(3)),
            node(Kind::Wildcard, [7, 8], None, None),
            node(Kind::Target, [14, 15], Some(4), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [14, 15],
                }),
                [14, 15],
                None,
                None,
            ),
        ],
        0,
    )
}

fn case_0009() -> Case {
    good(
        "select/options/1/1",
        "structural",
        b"SELECT * FROM t WHERE a=0",
        vec![
            node(Kind::Select, [0, 25], Some(1), None),
            node(Kind::Projection, [7, 8], Some(2), Some(3)),
            node(Kind::Wildcard, [7, 8], None, None),
            node(Kind::Target, [14, 15], Some(4), Some(5)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [14, 15],
                }),
                [14, 15],
                None,
                None,
            ),
            node(Kind::Where, [16, 25], Some(6), None),
            node(Kind::Or, [22, 25], Some(7), None),
            node(Kind::And, [22, 25], Some(8), None),
            node(Kind::Compare(Comparison::Eq), [22, 25], Some(9), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [22, 23],
                }),
                [22, 23],
                None,
                Some(10),
            ),
            node(
                Kind::Scalar(ScalarObservation::Integer {
                    sign: "None",
                    digits: [24, 25],
                }),
                [24, 25],
                None,
                None,
            ),
        ],
        0,
    )
}

fn case_0010() -> Case {
    good(
        "select/options/1/2",
        "structural",
        b"SELECT * FROM t ORDER BY b",
        vec![
            node(Kind::Select, [0, 26], Some(1), None),
            node(Kind::Projection, [7, 8], Some(2), Some(3)),
            node(Kind::Wildcard, [7, 8], None, None),
            node(Kind::Target, [14, 15], Some(4), Some(5)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [14, 15],
                }),
                [14, 15],
                None,
                None,
            ),
            node(Kind::OrderBy, [16, 26], Some(6), None),
            node(
                Kind::OrderTerm {
                    direction: None,
                    nulls: None,
                },
                [25, 26],
                Some(7),
                None,
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"B".to_vec(),
                    quoted: false,
                    span: [25, 26],
                }),
                [25, 26],
                None,
                None,
            ),
        ],
        0,
    )
}

fn case_0011() -> Case {
    good(
        "select/options/1/3",
        "structural",
        b"SELECT * FROM t WHERE a=0 ORDER BY b",
        vec![
            node(Kind::Select, [0, 36], Some(1), None),
            node(Kind::Projection, [7, 8], Some(2), Some(3)),
            node(Kind::Wildcard, [7, 8], None, None),
            node(Kind::Target, [14, 15], Some(4), Some(5)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [14, 15],
                }),
                [14, 15],
                None,
                None,
            ),
            node(Kind::Where, [16, 25], Some(6), Some(11)),
            node(Kind::Or, [22, 25], Some(7), None),
            node(Kind::And, [22, 25], Some(8), None),
            node(Kind::Compare(Comparison::Eq), [22, 25], Some(9), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [22, 23],
                }),
                [22, 23],
                None,
                Some(10),
            ),
            node(
                Kind::Scalar(ScalarObservation::Integer {
                    sign: "None",
                    digits: [24, 25],
                }),
                [24, 25],
                None,
                None,
            ),
            node(Kind::OrderBy, [26, 36], Some(12), None),
            node(
                Kind::OrderTerm {
                    direction: None,
                    nulls: None,
                },
                [35, 36],
                Some(13),
                None,
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"B".to_vec(),
                    quoted: false,
                    span: [35, 36],
                }),
                [35, 36],
                None,
                None,
            ),
        ],
        0,
    )
}

fn case_0012() -> Case {
    good(
        "select/options/1/4",
        "structural",
        b"SELECT * FROM t LIMIT ?",
        vec![
            node(Kind::Select, [0, 23], Some(1), None),
            node(Kind::Projection, [7, 8], Some(2), Some(3)),
            node(Kind::Wildcard, [7, 8], None, None),
            node(Kind::Target, [14, 15], Some(4), Some(5)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [14, 15],
                }),
                [14, 15],
                None,
                None,
            ),
            node(Kind::Limit, [16, 23], Some(6), None),
            node(
                Kind::Scalar(ScalarObservation::Parameter { ordinal: 1 }),
                [22, 23],
                None,
                None,
            ),
        ],
        1,
    )
}

fn case_0013() -> Case {
    good(
        "select/options/1/5",
        "structural",
        b"SELECT * FROM t WHERE a=0 LIMIT ?",
        vec![
            node(Kind::Select, [0, 33], Some(1), None),
            node(Kind::Projection, [7, 8], Some(2), Some(3)),
            node(Kind::Wildcard, [7, 8], None, None),
            node(Kind::Target, [14, 15], Some(4), Some(5)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [14, 15],
                }),
                [14, 15],
                None,
                None,
            ),
            node(Kind::Where, [16, 25], Some(6), Some(11)),
            node(Kind::Or, [22, 25], Some(7), None),
            node(Kind::And, [22, 25], Some(8), None),
            node(Kind::Compare(Comparison::Eq), [22, 25], Some(9), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [22, 23],
                }),
                [22, 23],
                None,
                Some(10),
            ),
            node(
                Kind::Scalar(ScalarObservation::Integer {
                    sign: "None",
                    digits: [24, 25],
                }),
                [24, 25],
                None,
                None,
            ),
            node(Kind::Limit, [26, 33], Some(12), None),
            node(
                Kind::Scalar(ScalarObservation::Parameter { ordinal: 1 }),
                [32, 33],
                None,
                None,
            ),
        ],
        1,
    )
}

fn case_0014() -> Case {
    good(
        "select/options/1/6",
        "structural",
        b"SELECT * FROM t ORDER BY b LIMIT ?",
        vec![
            node(Kind::Select, [0, 34], Some(1), None),
            node(Kind::Projection, [7, 8], Some(2), Some(3)),
            node(Kind::Wildcard, [7, 8], None, None),
            node(Kind::Target, [14, 15], Some(4), Some(5)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [14, 15],
                }),
                [14, 15],
                None,
                None,
            ),
            node(Kind::OrderBy, [16, 26], Some(6), Some(8)),
            node(
                Kind::OrderTerm {
                    direction: None,
                    nulls: None,
                },
                [25, 26],
                Some(7),
                None,
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"B".to_vec(),
                    quoted: false,
                    span: [25, 26],
                }),
                [25, 26],
                None,
                None,
            ),
            node(Kind::Limit, [27, 34], Some(9), None),
            node(
                Kind::Scalar(ScalarObservation::Parameter { ordinal: 1 }),
                [33, 34],
                None,
                None,
            ),
        ],
        1,
    )
}

fn case_0015() -> Case {
    good(
        "select/options/1/7",
        "structural",
        b"SELECT * FROM t WHERE a=0 ORDER BY b LIMIT ?",
        vec![
            node(Kind::Select, [0, 44], Some(1), None),
            node(Kind::Projection, [7, 8], Some(2), Some(3)),
            node(Kind::Wildcard, [7, 8], None, None),
            node(Kind::Target, [14, 15], Some(4), Some(5)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [14, 15],
                }),
                [14, 15],
                None,
                None,
            ),
            node(Kind::Where, [16, 25], Some(6), Some(11)),
            node(Kind::Or, [22, 25], Some(7), None),
            node(Kind::And, [22, 25], Some(8), None),
            node(Kind::Compare(Comparison::Eq), [22, 25], Some(9), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [22, 23],
                }),
                [22, 23],
                None,
                Some(10),
            ),
            node(
                Kind::Scalar(ScalarObservation::Integer {
                    sign: "None",
                    digits: [24, 25],
                }),
                [24, 25],
                None,
                None,
            ),
            node(Kind::OrderBy, [26, 36], Some(12), Some(14)),
            node(
                Kind::OrderTerm {
                    direction: None,
                    nulls: None,
                },
                [35, 36],
                Some(13),
                None,
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"B".to_vec(),
                    quoted: false,
                    span: [35, 36],
                }),
                [35, 36],
                None,
                None,
            ),
            node(Kind::Limit, [37, 44], Some(15), None),
            node(
                Kind::Scalar(ScalarObservation::Parameter { ordinal: 1 }),
                [43, 44],
                None,
                None,
            ),
        ],
        1,
    )
}

fn case_0016() -> Case {
    good(
        "select/order/None/None",
        "structural",
        b"SELECT a AS x FROM t ORDER BY b",
        vec![
            node(Kind::Select, [0, 31], Some(1), None),
            node(Kind::Projection, [7, 13], Some(2), Some(5)),
            node(Kind::ProjectionItem, [7, 13], Some(3), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [7, 8],
                }),
                [7, 8],
                None,
                Some(4),
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"X".to_vec(),
                    quoted: false,
                    span: [12, 13],
                }),
                [12, 13],
                None,
                None,
            ),
            node(Kind::Target, [19, 20], Some(6), Some(7)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [19, 20],
                }),
                [19, 20],
                None,
                None,
            ),
            node(Kind::OrderBy, [21, 31], Some(8), None),
            node(
                Kind::OrderTerm {
                    direction: None,
                    nulls: None,
                },
                [30, 31],
                Some(9),
                None,
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"B".to_vec(),
                    quoted: false,
                    span: [30, 31],
                }),
                [30, 31],
                None,
                None,
            ),
        ],
        0,
    )
}

fn case_0017() -> Case {
    good(
        "select/order/None/First",
        "structural",
        b"SELECT a AS x FROM t ORDER BY b NULLS FIRST",
        vec![
            node(Kind::Select, [0, 43], Some(1), None),
            node(Kind::Projection, [7, 13], Some(2), Some(5)),
            node(Kind::ProjectionItem, [7, 13], Some(3), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [7, 8],
                }),
                [7, 8],
                None,
                Some(4),
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"X".to_vec(),
                    quoted: false,
                    span: [12, 13],
                }),
                [12, 13],
                None,
                None,
            ),
            node(Kind::Target, [19, 20], Some(6), Some(7)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [19, 20],
                }),
                [19, 20],
                None,
                None,
            ),
            node(Kind::OrderBy, [21, 43], Some(8), None),
            node(
                Kind::OrderTerm {
                    direction: None,
                    nulls: Some(NullPlacement::First),
                },
                [30, 43],
                Some(9),
                None,
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"B".to_vec(),
                    quoted: false,
                    span: [30, 31],
                }),
                [30, 31],
                None,
                None,
            ),
        ],
        0,
    )
}

fn case_0018() -> Case {
    good(
        "select/order/None/Last",
        "structural",
        b"SELECT a AS x FROM t ORDER BY b NULLS LAST",
        vec![
            node(Kind::Select, [0, 42], Some(1), None),
            node(Kind::Projection, [7, 13], Some(2), Some(5)),
            node(Kind::ProjectionItem, [7, 13], Some(3), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [7, 8],
                }),
                [7, 8],
                None,
                Some(4),
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"X".to_vec(),
                    quoted: false,
                    span: [12, 13],
                }),
                [12, 13],
                None,
                None,
            ),
            node(Kind::Target, [19, 20], Some(6), Some(7)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [19, 20],
                }),
                [19, 20],
                None,
                None,
            ),
            node(Kind::OrderBy, [21, 42], Some(8), None),
            node(
                Kind::OrderTerm {
                    direction: None,
                    nulls: Some(NullPlacement::Last),
                },
                [30, 42],
                Some(9),
                None,
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"B".to_vec(),
                    quoted: false,
                    span: [30, 31],
                }),
                [30, 31],
                None,
                None,
            ),
        ],
        0,
    )
}

fn case_0019() -> Case {
    good(
        "select/order/Asc/None",
        "structural",
        b"SELECT a AS x FROM t ORDER BY b ASC",
        vec![
            node(Kind::Select, [0, 35], Some(1), None),
            node(Kind::Projection, [7, 13], Some(2), Some(5)),
            node(Kind::ProjectionItem, [7, 13], Some(3), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [7, 8],
                }),
                [7, 8],
                None,
                Some(4),
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"X".to_vec(),
                    quoted: false,
                    span: [12, 13],
                }),
                [12, 13],
                None,
                None,
            ),
            node(Kind::Target, [19, 20], Some(6), Some(7)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [19, 20],
                }),
                [19, 20],
                None,
                None,
            ),
            node(Kind::OrderBy, [21, 35], Some(8), None),
            node(
                Kind::OrderTerm {
                    direction: Some(Direction::Asc),
                    nulls: None,
                },
                [30, 35],
                Some(9),
                None,
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"B".to_vec(),
                    quoted: false,
                    span: [30, 31],
                }),
                [30, 31],
                None,
                None,
            ),
        ],
        0,
    )
}

fn case_0020() -> Case {
    good(
        "select/order/Asc/First",
        "structural",
        b"SELECT a AS x FROM t ORDER BY b ASC NULLS FIRST",
        vec![
            node(Kind::Select, [0, 47], Some(1), None),
            node(Kind::Projection, [7, 13], Some(2), Some(5)),
            node(Kind::ProjectionItem, [7, 13], Some(3), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [7, 8],
                }),
                [7, 8],
                None,
                Some(4),
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"X".to_vec(),
                    quoted: false,
                    span: [12, 13],
                }),
                [12, 13],
                None,
                None,
            ),
            node(Kind::Target, [19, 20], Some(6), Some(7)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [19, 20],
                }),
                [19, 20],
                None,
                None,
            ),
            node(Kind::OrderBy, [21, 47], Some(8), None),
            node(
                Kind::OrderTerm {
                    direction: Some(Direction::Asc),
                    nulls: Some(NullPlacement::First),
                },
                [30, 47],
                Some(9),
                None,
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"B".to_vec(),
                    quoted: false,
                    span: [30, 31],
                }),
                [30, 31],
                None,
                None,
            ),
        ],
        0,
    )
}

fn case_0021() -> Case {
    good(
        "select/order/Asc/Last",
        "structural",
        b"SELECT a AS x FROM t ORDER BY b ASC NULLS LAST",
        vec![
            node(Kind::Select, [0, 46], Some(1), None),
            node(Kind::Projection, [7, 13], Some(2), Some(5)),
            node(Kind::ProjectionItem, [7, 13], Some(3), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [7, 8],
                }),
                [7, 8],
                None,
                Some(4),
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"X".to_vec(),
                    quoted: false,
                    span: [12, 13],
                }),
                [12, 13],
                None,
                None,
            ),
            node(Kind::Target, [19, 20], Some(6), Some(7)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [19, 20],
                }),
                [19, 20],
                None,
                None,
            ),
            node(Kind::OrderBy, [21, 46], Some(8), None),
            node(
                Kind::OrderTerm {
                    direction: Some(Direction::Asc),
                    nulls: Some(NullPlacement::Last),
                },
                [30, 46],
                Some(9),
                None,
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"B".to_vec(),
                    quoted: false,
                    span: [30, 31],
                }),
                [30, 31],
                None,
                None,
            ),
        ],
        0,
    )
}

fn case_0022() -> Case {
    good(
        "select/order/Desc/None",
        "structural",
        b"SELECT a AS x FROM t ORDER BY b DESC",
        vec![
            node(Kind::Select, [0, 36], Some(1), None),
            node(Kind::Projection, [7, 13], Some(2), Some(5)),
            node(Kind::ProjectionItem, [7, 13], Some(3), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [7, 8],
                }),
                [7, 8],
                None,
                Some(4),
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"X".to_vec(),
                    quoted: false,
                    span: [12, 13],
                }),
                [12, 13],
                None,
                None,
            ),
            node(Kind::Target, [19, 20], Some(6), Some(7)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [19, 20],
                }),
                [19, 20],
                None,
                None,
            ),
            node(Kind::OrderBy, [21, 36], Some(8), None),
            node(
                Kind::OrderTerm {
                    direction: Some(Direction::Desc),
                    nulls: None,
                },
                [30, 36],
                Some(9),
                None,
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"B".to_vec(),
                    quoted: false,
                    span: [30, 31],
                }),
                [30, 31],
                None,
                None,
            ),
        ],
        0,
    )
}

fn case_0023() -> Case {
    good(
        "select/order/Desc/First",
        "structural",
        b"SELECT a AS x FROM t ORDER BY b DESC NULLS FIRST",
        vec![
            node(Kind::Select, [0, 48], Some(1), None),
            node(Kind::Projection, [7, 13], Some(2), Some(5)),
            node(Kind::ProjectionItem, [7, 13], Some(3), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [7, 8],
                }),
                [7, 8],
                None,
                Some(4),
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"X".to_vec(),
                    quoted: false,
                    span: [12, 13],
                }),
                [12, 13],
                None,
                None,
            ),
            node(Kind::Target, [19, 20], Some(6), Some(7)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [19, 20],
                }),
                [19, 20],
                None,
                None,
            ),
            node(Kind::OrderBy, [21, 48], Some(8), None),
            node(
                Kind::OrderTerm {
                    direction: Some(Direction::Desc),
                    nulls: Some(NullPlacement::First),
                },
                [30, 48],
                Some(9),
                None,
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"B".to_vec(),
                    quoted: false,
                    span: [30, 31],
                }),
                [30, 31],
                None,
                None,
            ),
        ],
        0,
    )
}

fn case_0024() -> Case {
    good(
        "select/order/Desc/Last",
        "structural",
        b"SELECT a AS x FROM t ORDER BY b DESC NULLS LAST",
        vec![
            node(Kind::Select, [0, 47], Some(1), None),
            node(Kind::Projection, [7, 13], Some(2), Some(5)),
            node(Kind::ProjectionItem, [7, 13], Some(3), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [7, 8],
                }),
                [7, 8],
                None,
                Some(4),
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"X".to_vec(),
                    quoted: false,
                    span: [12, 13],
                }),
                [12, 13],
                None,
                None,
            ),
            node(Kind::Target, [19, 20], Some(6), Some(7)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [19, 20],
                }),
                [19, 20],
                None,
                None,
            ),
            node(Kind::OrderBy, [21, 47], Some(8), None),
            node(
                Kind::OrderTerm {
                    direction: Some(Direction::Desc),
                    nulls: Some(NullPlacement::Last),
                },
                [30, 47],
                Some(9),
                None,
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"B".to_vec(),
                    quoted: false,
                    span: [30, 31],
                }),
                [30, 31],
                None,
                None,
            ),
        ],
        0,
    )
}

fn case_0025() -> Case {
    good(
        "select/duplicate-alias",
        "structural",
        b"SELECT a AS x,a AS x FROM t ORDER BY x",
        vec![
            node(Kind::Select, [0, 38], Some(1), None),
            node(Kind::Projection, [7, 20], Some(2), Some(8)),
            node(Kind::ProjectionItem, [7, 13], Some(3), Some(5)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [7, 8],
                }),
                [7, 8],
                None,
                Some(4),
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"X".to_vec(),
                    quoted: false,
                    span: [12, 13],
                }),
                [12, 13],
                None,
                None,
            ),
            node(Kind::ProjectionItem, [14, 20], Some(6), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [14, 15],
                }),
                [14, 15],
                None,
                Some(7),
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"X".to_vec(),
                    quoted: false,
                    span: [19, 20],
                }),
                [19, 20],
                None,
                None,
            ),
            node(Kind::Target, [26, 27], Some(9), Some(10)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [26, 27],
                }),
                [26, 27],
                None,
                None,
            ),
            node(Kind::OrderBy, [28, 38], Some(11), None),
            node(
                Kind::OrderTerm {
                    direction: None,
                    nulls: None,
                },
                [37, 38],
                Some(12),
                None,
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"X".to_vec(),
                    quoted: false,
                    span: [37, 38],
                }),
                [37, 38],
                None,
                None,
            ),
        ],
        0,
    )
}

fn case_0026() -> Case {
    good("select/quoted-public-trivia", "structural", b" \x0a/*head?;*/ SELECT \"\xc3\xa9\" AS \"x\"\"y\" FROM \"PUBLIC\".\"t\" WHERE ?=? ORDER BY a DESC NULLS FIRST LIMIT ?; -- tail?;\x0a", vec![
        node(Kind::Select, [13, 98], Some(1), None),
        node(Kind::Projection, [20, 34], Some(2), Some(5)),
        node(Kind::ProjectionItem, [20, 34], Some(3), None),
        node(Kind::Name(NameObservation { bytes: b"\xc3\xa9".to_vec(), quoted: true, span: [20, 24] }), [20, 24], None, Some(4)),
        node(Kind::Name(NameObservation { bytes: b"x\"y".to_vec(), quoted: true, span: [28, 34] }), [28, 34], None, None),
        node(Kind::Target, [40, 52], Some(6), Some(8)),
        node(Kind::Name(NameObservation { bytes: b"PUBLIC".to_vec(), quoted: true, span: [40, 48] }), [40, 48], None, Some(7)),
        node(Kind::Name(NameObservation { bytes: b"t".to_vec(), quoted: true, span: [49, 52] }), [49, 52], None, None),
        node(Kind::Where, [53, 62], Some(9), Some(14)),
        node(Kind::Or, [59, 62], Some(10), None),
        node(Kind::And, [59, 62], Some(11), None),
        node(Kind::Compare(Comparison::Eq), [59, 62], Some(12), None),
        node(Kind::Scalar(ScalarObservation::Parameter { ordinal: 1 }), [59, 60], None, Some(13)),
        node(Kind::Scalar(ScalarObservation::Parameter { ordinal: 2 }), [61, 62], None, None),
        node(Kind::OrderBy, [63, 90], Some(15), Some(17)),
        node(Kind::OrderTerm { direction: Some(Direction::Desc), nulls: Some(NullPlacement::First) }, [72, 90], Some(16), None),
        node(Kind::Name(NameObservation { bytes: b"A".to_vec(), quoted: false, span: [72, 73] }), [72, 73], None, None),
        node(Kind::Limit, [91, 98], Some(18), None),
        node(Kind::Scalar(ScalarObservation::Parameter { ordinal: 3 }), [97, 98], None, None),
    ], 3)
}

fn case_0027() -> Case {
    good(
        "update/minimum",
        "structural",
        b"UPDATE t SET a=0",
        vec![
            node(Kind::Update, [0, 16], Some(1), None),
            node(Kind::Target, [7, 8], Some(2), Some(3)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [7, 8],
                }),
                [7, 8],
                None,
                None,
            ),
            node(Kind::Assignments, [9, 16], Some(4), None),
            node(Kind::Assignment, [13, 16], Some(5), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [13, 14],
                }),
                [13, 14],
                None,
                Some(6),
            ),
            node(
                Kind::Scalar(ScalarObservation::Integer {
                    sign: "None",
                    digits: [15, 16],
                }),
                [15, 16],
                None,
                None,
            ),
        ],
        0,
    )
}

fn case_0028() -> Case {
    good(
        "update/markers-across-where",
        "structural",
        b"UPDATE t SET a=?,b=? WHERE ?=? AND ? IS NOT NULL",
        vec![
            node(Kind::Update, [0, 48], Some(1), None),
            node(Kind::Target, [7, 8], Some(2), Some(3)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [7, 8],
                }),
                [7, 8],
                None,
                None,
            ),
            node(Kind::Assignments, [9, 20], Some(4), Some(10)),
            node(Kind::Assignment, [13, 16], Some(5), Some(7)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [13, 14],
                }),
                [13, 14],
                None,
                Some(6),
            ),
            node(
                Kind::Scalar(ScalarObservation::Parameter { ordinal: 1 }),
                [15, 16],
                None,
                None,
            ),
            node(Kind::Assignment, [17, 20], Some(8), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"B".to_vec(),
                    quoted: false,
                    span: [17, 18],
                }),
                [17, 18],
                None,
                Some(9),
            ),
            node(
                Kind::Scalar(ScalarObservation::Parameter { ordinal: 2 }),
                [19, 20],
                None,
                None,
            ),
            node(Kind::Where, [21, 48], Some(11), None),
            node(Kind::Or, [27, 48], Some(12), None),
            node(Kind::And, [27, 48], Some(13), None),
            node(Kind::Compare(Comparison::Eq), [27, 30], Some(14), Some(16)),
            node(
                Kind::Scalar(ScalarObservation::Parameter { ordinal: 3 }),
                [27, 28],
                None,
                Some(15),
            ),
            node(
                Kind::Scalar(ScalarObservation::Parameter { ordinal: 4 }),
                [29, 30],
                None,
                None,
            ),
            node(Kind::IsNull { negated: true }, [35, 48], Some(17), None),
            node(
                Kind::Scalar(ScalarObservation::Parameter { ordinal: 5 }),
                [35, 36],
                None,
                None,
            ),
        ],
        5,
    )
}

fn case_0029() -> Case {
    good(
        "update/repeated-assignment",
        "structural",
        b"UPDATE public.t SET a=0,a=1",
        vec![
            node(Kind::Update, [0, 27], Some(1), None),
            node(Kind::Target, [7, 15], Some(2), Some(4)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"PUBLIC".to_vec(),
                    quoted: false,
                    span: [7, 13],
                }),
                [7, 13],
                None,
                Some(3),
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [14, 15],
                }),
                [14, 15],
                None,
                None,
            ),
            node(Kind::Assignments, [16, 27], Some(5), None),
            node(Kind::Assignment, [20, 23], Some(6), Some(8)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [20, 21],
                }),
                [20, 21],
                None,
                Some(7),
            ),
            node(
                Kind::Scalar(ScalarObservation::Integer {
                    sign: "None",
                    digits: [22, 23],
                }),
                [22, 23],
                None,
                None,
            ),
            node(Kind::Assignment, [24, 27], Some(9), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [24, 25],
                }),
                [24, 25],
                None,
                Some(10),
            ),
            node(
                Kind::Scalar(ScalarObservation::Integer {
                    sign: "None",
                    digits: [26, 27],
                }),
                [26, 27],
                None,
                None,
            ),
        ],
        0,
    )
}

fn case_0030() -> Case {
    good(
        "delete/minimum",
        "structural",
        b"DELETE FROM t",
        vec![
            node(Kind::Delete, [0, 13], Some(1), None),
            node(Kind::Target, [12, 13], Some(2), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [12, 13],
                }),
                [12, 13],
                None,
                None,
            ),
        ],
        0,
    )
}

fn case_0031() -> Case {
    good(
        "delete/where",
        "structural",
        b"DELETE FROM public.t WHERE a=b OR NOT (? IS NULL)",
        vec![
            node(Kind::Delete, [0, 49], Some(1), None),
            node(Kind::Target, [12, 20], Some(2), Some(4)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"PUBLIC".to_vec(),
                    quoted: false,
                    span: [12, 18],
                }),
                [12, 18],
                None,
                Some(3),
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [19, 20],
                }),
                [19, 20],
                None,
                None,
            ),
            node(Kind::Where, [21, 49], Some(5), None),
            node(Kind::Or, [27, 49], Some(6), None),
            node(Kind::And, [27, 30], Some(7), Some(10)),
            node(Kind::Compare(Comparison::Eq), [27, 30], Some(8), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [27, 28],
                }),
                [27, 28],
                None,
                Some(9),
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"B".to_vec(),
                    quoted: false,
                    span: [29, 30],
                }),
                [29, 30],
                None,
                None,
            ),
            node(Kind::And, [34, 49], Some(11), None),
            node(Kind::Not, [34, 49], Some(12), None),
            node(Kind::Group, [38, 49], Some(13), None),
            node(Kind::Or, [39, 48], Some(14), None),
            node(Kind::And, [39, 48], Some(15), None),
            node(Kind::IsNull { negated: false }, [39, 48], Some(16), None),
            node(
                Kind::Scalar(ScalarObservation::Parameter { ordinal: 1 }),
                [39, 40],
                None,
                None,
            ),
        ],
        1,
    )
}

fn case_0032() -> Case {
    good(
        "where/compare/Eq",
        "structural",
        b"SELECT * FROM t WHERE a=?",
        vec![
            node(Kind::Select, [0, 25], Some(1), None),
            node(Kind::Projection, [7, 8], Some(2), Some(3)),
            node(Kind::Wildcard, [7, 8], None, None),
            node(Kind::Target, [14, 15], Some(4), Some(5)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [14, 15],
                }),
                [14, 15],
                None,
                None,
            ),
            node(Kind::Where, [16, 25], Some(6), None),
            node(Kind::Or, [22, 25], Some(7), None),
            node(Kind::And, [22, 25], Some(8), None),
            node(Kind::Compare(Comparison::Eq), [22, 25], Some(9), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [22, 23],
                }),
                [22, 23],
                None,
                Some(10),
            ),
            node(
                Kind::Scalar(ScalarObservation::Parameter { ordinal: 1 }),
                [24, 25],
                None,
                None,
            ),
        ],
        1,
    )
}

fn case_0033() -> Case {
    good(
        "where/compare/Ne",
        "structural",
        b"SELECT * FROM t WHERE a<>?",
        vec![
            node(Kind::Select, [0, 26], Some(1), None),
            node(Kind::Projection, [7, 8], Some(2), Some(3)),
            node(Kind::Wildcard, [7, 8], None, None),
            node(Kind::Target, [14, 15], Some(4), Some(5)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [14, 15],
                }),
                [14, 15],
                None,
                None,
            ),
            node(Kind::Where, [16, 26], Some(6), None),
            node(Kind::Or, [22, 26], Some(7), None),
            node(Kind::And, [22, 26], Some(8), None),
            node(Kind::Compare(Comparison::Ne), [22, 26], Some(9), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [22, 23],
                }),
                [22, 23],
                None,
                Some(10),
            ),
            node(
                Kind::Scalar(ScalarObservation::Parameter { ordinal: 1 }),
                [25, 26],
                None,
                None,
            ),
        ],
        1,
    )
}

fn case_0034() -> Case {
    good(
        "where/compare/Lt",
        "structural",
        b"SELECT * FROM t WHERE a<?",
        vec![
            node(Kind::Select, [0, 25], Some(1), None),
            node(Kind::Projection, [7, 8], Some(2), Some(3)),
            node(Kind::Wildcard, [7, 8], None, None),
            node(Kind::Target, [14, 15], Some(4), Some(5)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [14, 15],
                }),
                [14, 15],
                None,
                None,
            ),
            node(Kind::Where, [16, 25], Some(6), None),
            node(Kind::Or, [22, 25], Some(7), None),
            node(Kind::And, [22, 25], Some(8), None),
            node(Kind::Compare(Comparison::Lt), [22, 25], Some(9), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [22, 23],
                }),
                [22, 23],
                None,
                Some(10),
            ),
            node(
                Kind::Scalar(ScalarObservation::Parameter { ordinal: 1 }),
                [24, 25],
                None,
                None,
            ),
        ],
        1,
    )
}

fn case_0035() -> Case {
    good(
        "where/compare/Le",
        "structural",
        b"SELECT * FROM t WHERE a<=?",
        vec![
            node(Kind::Select, [0, 26], Some(1), None),
            node(Kind::Projection, [7, 8], Some(2), Some(3)),
            node(Kind::Wildcard, [7, 8], None, None),
            node(Kind::Target, [14, 15], Some(4), Some(5)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [14, 15],
                }),
                [14, 15],
                None,
                None,
            ),
            node(Kind::Where, [16, 26], Some(6), None),
            node(Kind::Or, [22, 26], Some(7), None),
            node(Kind::And, [22, 26], Some(8), None),
            node(Kind::Compare(Comparison::Le), [22, 26], Some(9), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [22, 23],
                }),
                [22, 23],
                None,
                Some(10),
            ),
            node(
                Kind::Scalar(ScalarObservation::Parameter { ordinal: 1 }),
                [25, 26],
                None,
                None,
            ),
        ],
        1,
    )
}

fn case_0036() -> Case {
    good(
        "where/compare/Gt",
        "structural",
        b"SELECT * FROM t WHERE a>?",
        vec![
            node(Kind::Select, [0, 25], Some(1), None),
            node(Kind::Projection, [7, 8], Some(2), Some(3)),
            node(Kind::Wildcard, [7, 8], None, None),
            node(Kind::Target, [14, 15], Some(4), Some(5)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [14, 15],
                }),
                [14, 15],
                None,
                None,
            ),
            node(Kind::Where, [16, 25], Some(6), None),
            node(Kind::Or, [22, 25], Some(7), None),
            node(Kind::And, [22, 25], Some(8), None),
            node(Kind::Compare(Comparison::Gt), [22, 25], Some(9), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [22, 23],
                }),
                [22, 23],
                None,
                Some(10),
            ),
            node(
                Kind::Scalar(ScalarObservation::Parameter { ordinal: 1 }),
                [24, 25],
                None,
                None,
            ),
        ],
        1,
    )
}

fn case_0037() -> Case {
    good(
        "where/compare/Ge",
        "structural",
        b"SELECT * FROM t WHERE a>=?",
        vec![
            node(Kind::Select, [0, 26], Some(1), None),
            node(Kind::Projection, [7, 8], Some(2), Some(3)),
            node(Kind::Wildcard, [7, 8], None, None),
            node(Kind::Target, [14, 15], Some(4), Some(5)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [14, 15],
                }),
                [14, 15],
                None,
                None,
            ),
            node(Kind::Where, [16, 26], Some(6), None),
            node(Kind::Or, [22, 26], Some(7), None),
            node(Kind::And, [22, 26], Some(8), None),
            node(Kind::Compare(Comparison::Ge), [22, 26], Some(9), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [22, 23],
                }),
                [22, 23],
                None,
                Some(10),
            ),
            node(
                Kind::Scalar(ScalarObservation::Parameter { ordinal: 1 }),
                [25, 26],
                None,
                None,
            ),
        ],
        1,
    )
}

fn case_0038() -> Case {
    good(
        "where/is-null/False",
        "structural",
        b"DELETE FROM t WHERE a IS NULL",
        vec![
            node(Kind::Delete, [0, 29], Some(1), None),
            node(Kind::Target, [12, 13], Some(2), Some(3)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [12, 13],
                }),
                [12, 13],
                None,
                None,
            ),
            node(Kind::Where, [14, 29], Some(4), None),
            node(Kind::Or, [20, 29], Some(5), None),
            node(Kind::And, [20, 29], Some(6), None),
            node(Kind::IsNull { negated: false }, [20, 29], Some(7), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [20, 21],
                }),
                [20, 21],
                None,
                None,
            ),
        ],
        0,
    )
}

fn case_0039() -> Case {
    good(
        "where/is-null/True",
        "structural",
        b"DELETE FROM t WHERE a IS NOT NULL",
        vec![
            node(Kind::Delete, [0, 33], Some(1), None),
            node(Kind::Target, [12, 13], Some(2), Some(3)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [12, 13],
                }),
                [12, 13],
                None,
                None,
            ),
            node(Kind::Where, [14, 33], Some(4), None),
            node(Kind::Or, [20, 33], Some(5), None),
            node(Kind::And, [20, 33], Some(6), None),
            node(Kind::IsNull { negated: true }, [20, 33], Some(7), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [20, 21],
                }),
                [20, 21],
                None,
                None,
            ),
        ],
        0,
    )
}

fn case_0040() -> Case {
    good(
        "where/precedence",
        "structural",
        b"SELECT * FROM t WHERE a=0 AND NOT b=1 OR (a=b OR ? IS NOT NULL) LIMIT ?",
        vec![
            node(Kind::Select, [0, 71], Some(1), None),
            node(Kind::Projection, [7, 8], Some(2), Some(3)),
            node(Kind::Wildcard, [7, 8], None, None),
            node(Kind::Target, [14, 15], Some(4), Some(5)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [14, 15],
                }),
                [14, 15],
                None,
                None,
            ),
            node(Kind::Where, [16, 63], Some(6), Some(25)),
            node(Kind::Or, [22, 63], Some(7), None),
            node(Kind::And, [22, 37], Some(8), Some(15)),
            node(Kind::Compare(Comparison::Eq), [22, 25], Some(9), Some(11)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [22, 23],
                }),
                [22, 23],
                None,
                Some(10),
            ),
            node(
                Kind::Scalar(ScalarObservation::Integer {
                    sign: "None",
                    digits: [24, 25],
                }),
                [24, 25],
                None,
                None,
            ),
            node(Kind::Not, [30, 37], Some(12), None),
            node(Kind::Compare(Comparison::Eq), [34, 37], Some(13), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"B".to_vec(),
                    quoted: false,
                    span: [34, 35],
                }),
                [34, 35],
                None,
                Some(14),
            ),
            node(
                Kind::Scalar(ScalarObservation::Integer {
                    sign: "None",
                    digits: [36, 37],
                }),
                [36, 37],
                None,
                None,
            ),
            node(Kind::And, [41, 63], Some(16), None),
            node(Kind::Group, [41, 63], Some(17), None),
            node(Kind::Or, [42, 62], Some(18), None),
            node(Kind::And, [42, 45], Some(19), Some(22)),
            node(Kind::Compare(Comparison::Eq), [42, 45], Some(20), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [42, 43],
                }),
                [42, 43],
                None,
                Some(21),
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"B".to_vec(),
                    quoted: false,
                    span: [44, 45],
                }),
                [44, 45],
                None,
                None,
            ),
            node(Kind::And, [49, 62], Some(23), None),
            node(Kind::IsNull { negated: true }, [49, 62], Some(24), None),
            node(
                Kind::Scalar(ScalarObservation::Parameter { ordinal: 1 }),
                [49, 50],
                None,
                None,
            ),
            node(Kind::Limit, [64, 71], Some(26), None),
            node(
                Kind::Scalar(ScalarObservation::Parameter { ordinal: 2 }),
                [70, 71],
                None,
                None,
            ),
        ],
        2,
    )
}

fn case_0041() -> Case {
    good(
        "ddl/type-null/Int64/Implicit",
        "structural",
        b"CREATE TABLE t(a BIGINT) STORAGE DISK",
        vec![
            node(Kind::CreateTable, [0, 37], Some(1), None),
            node(Kind::Target, [13, 14], Some(2), Some(3)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [13, 14],
                }),
                [13, 14],
                None,
                None,
            ),
            node(Kind::TableDefinition, [14, 24], Some(4), Some(7)),
            node(
                Kind::ColumnDefinition {
                    nullability: Nullability::Implicit,
                },
                [15, 23],
                Some(5),
                None,
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [15, 16],
                }),
                [15, 16],
                None,
                Some(6),
            ),
            node(Kind::DataType(DataType::Int64), [17, 23], None, None),
            node(Kind::StorageDisk, [25, 37], None, None),
        ],
        0,
    )
}

fn case_0042() -> Case {
    good(
        "ddl/type-null/Int64/Null",
        "structural",
        b"CREATE TABLE t(a BIGINT NULL) STORAGE DISK",
        vec![
            node(Kind::CreateTable, [0, 42], Some(1), None),
            node(Kind::Target, [13, 14], Some(2), Some(3)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [13, 14],
                }),
                [13, 14],
                None,
                None,
            ),
            node(Kind::TableDefinition, [14, 29], Some(4), Some(7)),
            node(
                Kind::ColumnDefinition {
                    nullability: Nullability::Null,
                },
                [15, 28],
                Some(5),
                None,
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [15, 16],
                }),
                [15, 16],
                None,
                Some(6),
            ),
            node(Kind::DataType(DataType::Int64), [17, 23], None, None),
            node(Kind::StorageDisk, [30, 42], None, None),
        ],
        0,
    )
}

fn case_0043() -> Case {
    good(
        "ddl/type-null/Int64/NotNull",
        "structural",
        b"CREATE TABLE t(a BIGINT NOT NULL) STORAGE DISK",
        vec![
            node(Kind::CreateTable, [0, 46], Some(1), None),
            node(Kind::Target, [13, 14], Some(2), Some(3)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [13, 14],
                }),
                [13, 14],
                None,
                None,
            ),
            node(Kind::TableDefinition, [14, 33], Some(4), Some(7)),
            node(
                Kind::ColumnDefinition {
                    nullability: Nullability::NotNull,
                },
                [15, 32],
                Some(5),
                None,
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [15, 16],
                }),
                [15, 16],
                None,
                Some(6),
            ),
            node(Kind::DataType(DataType::Int64), [17, 23], None, None),
            node(Kind::StorageDisk, [34, 46], None, None),
        ],
        0,
    )
}

fn case_0044() -> Case {
    good(
        "ddl/type-null/Bool/Implicit",
        "structural",
        b"CREATE TABLE t(a BOOLEAN) STORAGE DISK",
        vec![
            node(Kind::CreateTable, [0, 38], Some(1), None),
            node(Kind::Target, [13, 14], Some(2), Some(3)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [13, 14],
                }),
                [13, 14],
                None,
                None,
            ),
            node(Kind::TableDefinition, [14, 25], Some(4), Some(7)),
            node(
                Kind::ColumnDefinition {
                    nullability: Nullability::Implicit,
                },
                [15, 24],
                Some(5),
                None,
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [15, 16],
                }),
                [15, 16],
                None,
                Some(6),
            ),
            node(Kind::DataType(DataType::Bool), [17, 24], None, None),
            node(Kind::StorageDisk, [26, 38], None, None),
        ],
        0,
    )
}

fn case_0045() -> Case {
    good(
        "ddl/type-null/Bool/Null",
        "structural",
        b"CREATE TABLE t(a BOOLEAN NULL) STORAGE DISK",
        vec![
            node(Kind::CreateTable, [0, 43], Some(1), None),
            node(Kind::Target, [13, 14], Some(2), Some(3)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [13, 14],
                }),
                [13, 14],
                None,
                None,
            ),
            node(Kind::TableDefinition, [14, 30], Some(4), Some(7)),
            node(
                Kind::ColumnDefinition {
                    nullability: Nullability::Null,
                },
                [15, 29],
                Some(5),
                None,
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [15, 16],
                }),
                [15, 16],
                None,
                Some(6),
            ),
            node(Kind::DataType(DataType::Bool), [17, 24], None, None),
            node(Kind::StorageDisk, [31, 43], None, None),
        ],
        0,
    )
}

fn case_0046() -> Case {
    good(
        "ddl/type-null/Bool/NotNull",
        "structural",
        b"CREATE TABLE t(a BOOLEAN NOT NULL) STORAGE DISK",
        vec![
            node(Kind::CreateTable, [0, 47], Some(1), None),
            node(Kind::Target, [13, 14], Some(2), Some(3)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [13, 14],
                }),
                [13, 14],
                None,
                None,
            ),
            node(Kind::TableDefinition, [14, 34], Some(4), Some(7)),
            node(
                Kind::ColumnDefinition {
                    nullability: Nullability::NotNull,
                },
                [15, 33],
                Some(5),
                None,
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [15, 16],
                }),
                [15, 16],
                None,
                Some(6),
            ),
            node(Kind::DataType(DataType::Bool), [17, 24], None, None),
            node(Kind::StorageDisk, [35, 47], None, None),
        ],
        0,
    )
}

fn case_0047() -> Case {
    good(
        "ddl/type-null/Utf8/Implicit",
        "structural",
        b"CREATE TABLE t(a VARCHAR(1)) STORAGE DISK",
        vec![
            node(Kind::CreateTable, [0, 41], Some(1), None),
            node(Kind::Target, [13, 14], Some(2), Some(3)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [13, 14],
                }),
                [13, 14],
                None,
                None,
            ),
            node(Kind::TableDefinition, [14, 28], Some(4), Some(7)),
            node(
                Kind::ColumnDefinition {
                    nullability: Nullability::Implicit,
                },
                [15, 27],
                Some(5),
                None,
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [15, 16],
                }),
                [15, 16],
                None,
                Some(6),
            ),
            node(
                Kind::DataType(DataType::Utf8(TypeLength {
                    sign: "None",
                    digits: [25, 26],
                    span: [25, 26],
                })),
                [17, 27],
                None,
                None,
            ),
            node(Kind::StorageDisk, [29, 41], None, None),
        ],
        0,
    )
}

fn case_0048() -> Case {
    good(
        "ddl/type-null/Utf8/Null",
        "structural",
        b"CREATE TABLE t(a VARCHAR(1) NULL) STORAGE DISK",
        vec![
            node(Kind::CreateTable, [0, 46], Some(1), None),
            node(Kind::Target, [13, 14], Some(2), Some(3)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [13, 14],
                }),
                [13, 14],
                None,
                None,
            ),
            node(Kind::TableDefinition, [14, 33], Some(4), Some(7)),
            node(
                Kind::ColumnDefinition {
                    nullability: Nullability::Null,
                },
                [15, 32],
                Some(5),
                None,
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [15, 16],
                }),
                [15, 16],
                None,
                Some(6),
            ),
            node(
                Kind::DataType(DataType::Utf8(TypeLength {
                    sign: "None",
                    digits: [25, 26],
                    span: [25, 26],
                })),
                [17, 27],
                None,
                None,
            ),
            node(Kind::StorageDisk, [34, 46], None, None),
        ],
        0,
    )
}

fn case_0049() -> Case {
    good(
        "ddl/type-null/Utf8/NotNull",
        "structural",
        b"CREATE TABLE t(a VARCHAR(1) NOT NULL) STORAGE DISK",
        vec![
            node(Kind::CreateTable, [0, 50], Some(1), None),
            node(Kind::Target, [13, 14], Some(2), Some(3)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [13, 14],
                }),
                [13, 14],
                None,
                None,
            ),
            node(Kind::TableDefinition, [14, 37], Some(4), Some(7)),
            node(
                Kind::ColumnDefinition {
                    nullability: Nullability::NotNull,
                },
                [15, 36],
                Some(5),
                None,
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [15, 16],
                }),
                [15, 16],
                None,
                Some(6),
            ),
            node(
                Kind::DataType(DataType::Utf8(TypeLength {
                    sign: "None",
                    digits: [25, 26],
                    span: [25, 26],
                })),
                [17, 27],
                None,
                None,
            ),
            node(Kind::StorageDisk, [38, 50], None, None),
        ],
        0,
    )
}

fn case_0050() -> Case {
    good(
        "ddl/type-null/Binary/Implicit",
        "structural",
        b"CREATE TABLE t(a VARBINARY(1)) STORAGE DISK",
        vec![
            node(Kind::CreateTable, [0, 43], Some(1), None),
            node(Kind::Target, [13, 14], Some(2), Some(3)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [13, 14],
                }),
                [13, 14],
                None,
                None,
            ),
            node(Kind::TableDefinition, [14, 30], Some(4), Some(7)),
            node(
                Kind::ColumnDefinition {
                    nullability: Nullability::Implicit,
                },
                [15, 29],
                Some(5),
                None,
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [15, 16],
                }),
                [15, 16],
                None,
                Some(6),
            ),
            node(
                Kind::DataType(DataType::Binary(TypeLength {
                    sign: "None",
                    digits: [27, 28],
                    span: [27, 28],
                })),
                [17, 29],
                None,
                None,
            ),
            node(Kind::StorageDisk, [31, 43], None, None),
        ],
        0,
    )
}

fn case_0051() -> Case {
    good(
        "ddl/type-null/Binary/Null",
        "structural",
        b"CREATE TABLE t(a VARBINARY(1) NULL) STORAGE DISK",
        vec![
            node(Kind::CreateTable, [0, 48], Some(1), None),
            node(Kind::Target, [13, 14], Some(2), Some(3)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [13, 14],
                }),
                [13, 14],
                None,
                None,
            ),
            node(Kind::TableDefinition, [14, 35], Some(4), Some(7)),
            node(
                Kind::ColumnDefinition {
                    nullability: Nullability::Null,
                },
                [15, 34],
                Some(5),
                None,
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [15, 16],
                }),
                [15, 16],
                None,
                Some(6),
            ),
            node(
                Kind::DataType(DataType::Binary(TypeLength {
                    sign: "None",
                    digits: [27, 28],
                    span: [27, 28],
                })),
                [17, 29],
                None,
                None,
            ),
            node(Kind::StorageDisk, [36, 48], None, None),
        ],
        0,
    )
}

fn case_0052() -> Case {
    good(
        "ddl/type-null/Binary/NotNull",
        "structural",
        b"CREATE TABLE t(a VARBINARY(1) NOT NULL) STORAGE DISK",
        vec![
            node(Kind::CreateTable, [0, 52], Some(1), None),
            node(Kind::Target, [13, 14], Some(2), Some(3)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [13, 14],
                }),
                [13, 14],
                None,
                None,
            ),
            node(Kind::TableDefinition, [14, 39], Some(4), Some(7)),
            node(
                Kind::ColumnDefinition {
                    nullability: Nullability::NotNull,
                },
                [15, 38],
                Some(5),
                None,
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [15, 16],
                }),
                [15, 16],
                None,
                Some(6),
            ),
            node(
                Kind::DataType(DataType::Binary(TypeLength {
                    sign: "None",
                    digits: [27, 28],
                    span: [27, 28],
                })),
                [17, 29],
                None,
                None,
            ),
            node(Kind::StorageDisk, [40, 52], None, None),
        ],
        0,
    )
}

fn case_0053() -> Case {
    good(
        "ddl/constraints/False/False",
        "structural",
        b"CREATE TABLE t(a BIGINT,b BOOLEAN NULL) STORAGE DISK",
        vec![
            node(Kind::CreateTable, [0, 52], Some(1), None),
            node(Kind::Target, [13, 14], Some(2), Some(3)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [13, 14],
                }),
                [13, 14],
                None,
                None,
            ),
            node(Kind::TableDefinition, [14, 39], Some(4), Some(10)),
            node(
                Kind::ColumnDefinition {
                    nullability: Nullability::Implicit,
                },
                [15, 23],
                Some(5),
                Some(7),
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [15, 16],
                }),
                [15, 16],
                None,
                Some(6),
            ),
            node(Kind::DataType(DataType::Int64), [17, 23], None, None),
            node(
                Kind::ColumnDefinition {
                    nullability: Nullability::Null,
                },
                [24, 38],
                Some(8),
                None,
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"B".to_vec(),
                    quoted: false,
                    span: [24, 25],
                }),
                [24, 25],
                None,
                Some(9),
            ),
            node(Kind::DataType(DataType::Bool), [26, 33], None, None),
            node(Kind::StorageDisk, [40, 52], None, None),
        ],
        0,
    )
}

fn case_0054() -> Case {
    good(
        "ddl/constraints/False/True",
        "structural",
        b"CREATE TABLE t(a BIGINT,b BOOLEAN NULL,UNIQUE (a,b),UNIQUE (b)) STORAGE DISK",
        vec![
            node(Kind::CreateTable, [0, 76], Some(1), None),
            node(Kind::Target, [13, 14], Some(2), Some(3)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [13, 14],
                }),
                [13, 14],
                None,
                None,
            ),
            node(Kind::TableDefinition, [14, 63], Some(4), Some(17)),
            node(
                Kind::ColumnDefinition {
                    nullability: Nullability::Implicit,
                },
                [15, 23],
                Some(5),
                Some(7),
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [15, 16],
                }),
                [15, 16],
                None,
                Some(6),
            ),
            node(Kind::DataType(DataType::Int64), [17, 23], None, None),
            node(
                Kind::ColumnDefinition {
                    nullability: Nullability::Null,
                },
                [24, 38],
                Some(8),
                Some(10),
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"B".to_vec(),
                    quoted: false,
                    span: [24, 25],
                }),
                [24, 25],
                None,
                Some(9),
            ),
            node(Kind::DataType(DataType::Bool), [26, 33], None, None),
            node(Kind::UniqueConstraint, [39, 51], Some(11), Some(14)),
            node(Kind::Columns, [46, 51], Some(12), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [47, 48],
                }),
                [47, 48],
                None,
                Some(13),
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"B".to_vec(),
                    quoted: false,
                    span: [49, 50],
                }),
                [49, 50],
                None,
                None,
            ),
            node(Kind::UniqueConstraint, [52, 62], Some(15), None),
            node(Kind::Columns, [59, 62], Some(16), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"B".to_vec(),
                    quoted: false,
                    span: [60, 61],
                }),
                [60, 61],
                None,
                None,
            ),
            node(Kind::StorageDisk, [64, 76], None, None),
        ],
        0,
    )
}

fn case_0055() -> Case {
    good(
        "ddl/constraints/True/False",
        "structural",
        b"CREATE TABLE t(a BIGINT,b BOOLEAN NULL,PRIMARY KEY (a)) STORAGE DISK",
        vec![
            node(Kind::CreateTable, [0, 68], Some(1), None),
            node(Kind::Target, [13, 14], Some(2), Some(3)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [13, 14],
                }),
                [13, 14],
                None,
                None,
            ),
            node(Kind::TableDefinition, [14, 55], Some(4), Some(13)),
            node(
                Kind::ColumnDefinition {
                    nullability: Nullability::Implicit,
                },
                [15, 23],
                Some(5),
                Some(7),
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [15, 16],
                }),
                [15, 16],
                None,
                Some(6),
            ),
            node(Kind::DataType(DataType::Int64), [17, 23], None, None),
            node(
                Kind::ColumnDefinition {
                    nullability: Nullability::Null,
                },
                [24, 38],
                Some(8),
                Some(10),
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"B".to_vec(),
                    quoted: false,
                    span: [24, 25],
                }),
                [24, 25],
                None,
                Some(9),
            ),
            node(Kind::DataType(DataType::Bool), [26, 33], None, None),
            node(Kind::PrimaryKey, [39, 54], Some(11), None),
            node(Kind::Columns, [51, 54], Some(12), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [52, 53],
                }),
                [52, 53],
                None,
                None,
            ),
            node(Kind::StorageDisk, [56, 68], None, None),
        ],
        0,
    )
}

fn case_0056() -> Case {
    good("ddl/constraints/True/True", "structural", b"CREATE TABLE t(a BIGINT,b BOOLEAN NULL,PRIMARY KEY (a),UNIQUE (a,b),UNIQUE (b)) STORAGE DISK", vec![
        node(Kind::CreateTable, [0, 92], Some(1), None),
        node(Kind::Target, [13, 14], Some(2), Some(3)),
        node(Kind::Name(NameObservation { bytes: b"T".to_vec(), quoted: false, span: [13, 14] }), [13, 14], None, None),
        node(Kind::TableDefinition, [14, 79], Some(4), Some(20)),
        node(Kind::ColumnDefinition { nullability: Nullability::Implicit }, [15, 23], Some(5), Some(7)),
        node(Kind::Name(NameObservation { bytes: b"A".to_vec(), quoted: false, span: [15, 16] }), [15, 16], None, Some(6)),
        node(Kind::DataType(DataType::Int64), [17, 23], None, None),
        node(Kind::ColumnDefinition { nullability: Nullability::Null }, [24, 38], Some(8), Some(10)),
        node(Kind::Name(NameObservation { bytes: b"B".to_vec(), quoted: false, span: [24, 25] }), [24, 25], None, Some(9)),
        node(Kind::DataType(DataType::Bool), [26, 33], None, None),
        node(Kind::PrimaryKey, [39, 54], Some(11), Some(13)),
        node(Kind::Columns, [51, 54], Some(12), None),
        node(Kind::Name(NameObservation { bytes: b"A".to_vec(), quoted: false, span: [52, 53] }), [52, 53], None, None),
        node(Kind::UniqueConstraint, [55, 67], Some(14), Some(17)),
        node(Kind::Columns, [62, 67], Some(15), None),
        node(Kind::Name(NameObservation { bytes: b"A".to_vec(), quoted: false, span: [63, 64] }), [63, 64], None, Some(16)),
        node(Kind::Name(NameObservation { bytes: b"B".to_vec(), quoted: false, span: [65, 66] }), [65, 66], None, None),
        node(Kind::UniqueConstraint, [68, 78], Some(18), None),
        node(Kind::Columns, [75, 78], Some(19), None),
        node(Kind::Name(NameObservation { bytes: b"B".to_vec(), quoted: false, span: [76, 77] }), [76, 77], None, None),
        node(Kind::StorageDisk, [80, 92], None, None),
    ], 0)
}

fn case_0057() -> Case {
    good(
        "ddl/qualified",
        "structural",
        b"CREATE TABLE public.t(a BIGINT) STORAGE DISK; /*done*/",
        vec![
            node(Kind::CreateTable, [0, 44], Some(1), None),
            node(Kind::Target, [13, 21], Some(2), Some(4)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"PUBLIC".to_vec(),
                    quoted: false,
                    span: [13, 19],
                }),
                [13, 19],
                None,
                Some(3),
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [20, 21],
                }),
                [20, 21],
                None,
                None,
            ),
            node(Kind::TableDefinition, [21, 31], Some(5), Some(8)),
            node(
                Kind::ColumnDefinition {
                    nullability: Nullability::Implicit,
                },
                [22, 30],
                Some(6),
                None,
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [22, 23],
                }),
                [22, 23],
                None,
                Some(7),
            ),
            node(Kind::DataType(DataType::Int64), [24, 30], None, None),
            node(Kind::StorageDisk, [32, 44], None, None),
        ],
        0,
    )
}

fn case_0058() -> Case {
    good(
        "index/targets/False/False/False",
        "structural",
        b"CREATE INDEX i ON t(a,b)",
        vec![
            node(Kind::CreateIndex { unique: false }, [0, 24], Some(1), None),
            node(Kind::Target, [13, 14], Some(2), Some(3)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"I".to_vec(),
                    quoted: false,
                    span: [13, 14],
                }),
                [13, 14],
                None,
                None,
            ),
            node(Kind::Target, [18, 19], Some(4), Some(5)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [18, 19],
                }),
                [18, 19],
                None,
                None,
            ),
            node(Kind::Columns, [19, 24], Some(6), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [20, 21],
                }),
                [20, 21],
                None,
                Some(7),
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"B".to_vec(),
                    quoted: false,
                    span: [22, 23],
                }),
                [22, 23],
                None,
                None,
            ),
        ],
        0,
    )
}

fn case_0059() -> Case {
    good(
        "index/targets/False/False/True",
        "structural",
        b"CREATE INDEX i ON public.t(a,b)",
        vec![
            node(Kind::CreateIndex { unique: false }, [0, 31], Some(1), None),
            node(Kind::Target, [13, 14], Some(2), Some(3)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"I".to_vec(),
                    quoted: false,
                    span: [13, 14],
                }),
                [13, 14],
                None,
                None,
            ),
            node(Kind::Target, [18, 26], Some(4), Some(6)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"PUBLIC".to_vec(),
                    quoted: false,
                    span: [18, 24],
                }),
                [18, 24],
                None,
                Some(5),
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [25, 26],
                }),
                [25, 26],
                None,
                None,
            ),
            node(Kind::Columns, [26, 31], Some(7), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [27, 28],
                }),
                [27, 28],
                None,
                Some(8),
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"B".to_vec(),
                    quoted: false,
                    span: [29, 30],
                }),
                [29, 30],
                None,
                None,
            ),
        ],
        0,
    )
}

fn case_0060() -> Case {
    good(
        "index/targets/False/True/False",
        "structural",
        b"CREATE INDEX public.i ON t(a,b)",
        vec![
            node(Kind::CreateIndex { unique: false }, [0, 31], Some(1), None),
            node(Kind::Target, [13, 21], Some(2), Some(4)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"PUBLIC".to_vec(),
                    quoted: false,
                    span: [13, 19],
                }),
                [13, 19],
                None,
                Some(3),
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"I".to_vec(),
                    quoted: false,
                    span: [20, 21],
                }),
                [20, 21],
                None,
                None,
            ),
            node(Kind::Target, [25, 26], Some(5), Some(6)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [25, 26],
                }),
                [25, 26],
                None,
                None,
            ),
            node(Kind::Columns, [26, 31], Some(7), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [27, 28],
                }),
                [27, 28],
                None,
                Some(8),
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"B".to_vec(),
                    quoted: false,
                    span: [29, 30],
                }),
                [29, 30],
                None,
                None,
            ),
        ],
        0,
    )
}

fn case_0061() -> Case {
    good(
        "index/targets/False/True/True",
        "structural",
        b"CREATE INDEX public.i ON public.t(a,b)",
        vec![
            node(Kind::CreateIndex { unique: false }, [0, 38], Some(1), None),
            node(Kind::Target, [13, 21], Some(2), Some(4)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"PUBLIC".to_vec(),
                    quoted: false,
                    span: [13, 19],
                }),
                [13, 19],
                None,
                Some(3),
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"I".to_vec(),
                    quoted: false,
                    span: [20, 21],
                }),
                [20, 21],
                None,
                None,
            ),
            node(Kind::Target, [25, 33], Some(5), Some(7)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"PUBLIC".to_vec(),
                    quoted: false,
                    span: [25, 31],
                }),
                [25, 31],
                None,
                Some(6),
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [32, 33],
                }),
                [32, 33],
                None,
                None,
            ),
            node(Kind::Columns, [33, 38], Some(8), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [34, 35],
                }),
                [34, 35],
                None,
                Some(9),
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"B".to_vec(),
                    quoted: false,
                    span: [36, 37],
                }),
                [36, 37],
                None,
                None,
            ),
        ],
        0,
    )
}

fn case_0062() -> Case {
    good(
        "index/targets/True/False/False",
        "structural",
        b"CREATE UNIQUE INDEX i ON t(a,b)",
        vec![
            node(Kind::CreateIndex { unique: true }, [0, 31], Some(1), None),
            node(Kind::Target, [20, 21], Some(2), Some(3)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"I".to_vec(),
                    quoted: false,
                    span: [20, 21],
                }),
                [20, 21],
                None,
                None,
            ),
            node(Kind::Target, [25, 26], Some(4), Some(5)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [25, 26],
                }),
                [25, 26],
                None,
                None,
            ),
            node(Kind::Columns, [26, 31], Some(6), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [27, 28],
                }),
                [27, 28],
                None,
                Some(7),
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"B".to_vec(),
                    quoted: false,
                    span: [29, 30],
                }),
                [29, 30],
                None,
                None,
            ),
        ],
        0,
    )
}

fn case_0063() -> Case {
    good(
        "index/targets/True/False/True",
        "structural",
        b"CREATE UNIQUE INDEX i ON public.t(a,b)",
        vec![
            node(Kind::CreateIndex { unique: true }, [0, 38], Some(1), None),
            node(Kind::Target, [20, 21], Some(2), Some(3)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"I".to_vec(),
                    quoted: false,
                    span: [20, 21],
                }),
                [20, 21],
                None,
                None,
            ),
            node(Kind::Target, [25, 33], Some(4), Some(6)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"PUBLIC".to_vec(),
                    quoted: false,
                    span: [25, 31],
                }),
                [25, 31],
                None,
                Some(5),
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [32, 33],
                }),
                [32, 33],
                None,
                None,
            ),
            node(Kind::Columns, [33, 38], Some(7), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [34, 35],
                }),
                [34, 35],
                None,
                Some(8),
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"B".to_vec(),
                    quoted: false,
                    span: [36, 37],
                }),
                [36, 37],
                None,
                None,
            ),
        ],
        0,
    )
}

fn case_0064() -> Case {
    good(
        "index/targets/True/True/False",
        "structural",
        b"CREATE UNIQUE INDEX public.i ON t(a,b)",
        vec![
            node(Kind::CreateIndex { unique: true }, [0, 38], Some(1), None),
            node(Kind::Target, [20, 28], Some(2), Some(4)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"PUBLIC".to_vec(),
                    quoted: false,
                    span: [20, 26],
                }),
                [20, 26],
                None,
                Some(3),
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"I".to_vec(),
                    quoted: false,
                    span: [27, 28],
                }),
                [27, 28],
                None,
                None,
            ),
            node(Kind::Target, [32, 33], Some(5), Some(6)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [32, 33],
                }),
                [32, 33],
                None,
                None,
            ),
            node(Kind::Columns, [33, 38], Some(7), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [34, 35],
                }),
                [34, 35],
                None,
                Some(8),
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"B".to_vec(),
                    quoted: false,
                    span: [36, 37],
                }),
                [36, 37],
                None,
                None,
            ),
        ],
        0,
    )
}

fn case_0065() -> Case {
    good(
        "index/targets/True/True/True",
        "structural",
        b"CREATE UNIQUE INDEX public.i ON public.t(a,b)",
        vec![
            node(Kind::CreateIndex { unique: true }, [0, 45], Some(1), None),
            node(Kind::Target, [20, 28], Some(2), Some(4)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"PUBLIC".to_vec(),
                    quoted: false,
                    span: [20, 26],
                }),
                [20, 26],
                None,
                Some(3),
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"I".to_vec(),
                    quoted: false,
                    span: [27, 28],
                }),
                [27, 28],
                None,
                None,
            ),
            node(Kind::Target, [32, 40], Some(5), Some(7)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"PUBLIC".to_vec(),
                    quoted: false,
                    span: [32, 38],
                }),
                [32, 38],
                None,
                Some(6),
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [39, 40],
                }),
                [39, 40],
                None,
                None,
            ),
            node(Kind::Columns, [40, 45], Some(8), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [41, 42],
                }),
                [41, 42],
                None,
                Some(9),
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"B".to_vec(),
                    quoted: false,
                    span: [43, 44],
                }),
                [43, 44],
                None,
                None,
            ),
        ],
        0,
    )
}

fn case_0066() -> Case {
    good(
        "drop/False/False",
        "structural",
        b"DROP TABLE t",
        vec![
            node(Kind::DropTable, [0, 12], Some(1), None),
            node(Kind::Target, [11, 12], Some(2), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [11, 12],
                }),
                [11, 12],
                None,
                None,
            ),
        ],
        0,
    )
}

fn case_0067() -> Case {
    good(
        "drop/False/True",
        "structural",
        b"DROP TABLE public.t",
        vec![
            node(Kind::DropTable, [0, 19], Some(1), None),
            node(Kind::Target, [11, 19], Some(2), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"PUBLIC".to_vec(),
                    quoted: false,
                    span: [11, 17],
                }),
                [11, 17],
                None,
                Some(3),
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [18, 19],
                }),
                [18, 19],
                None,
                None,
            ),
        ],
        0,
    )
}

fn case_0068() -> Case {
    good(
        "drop/True/False",
        "structural",
        b"DROP INDEX t",
        vec![
            node(Kind::DropIndex, [0, 12], Some(1), None),
            node(Kind::Target, [11, 12], Some(2), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [11, 12],
                }),
                [11, 12],
                None,
                None,
            ),
        ],
        0,
    )
}

fn case_0069() -> Case {
    good(
        "drop/True/True",
        "structural",
        b"DROP INDEX public.t",
        vec![
            node(Kind::DropIndex, [0, 19], Some(1), None),
            node(Kind::Target, [11, 19], Some(2), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"PUBLIC".to_vec(),
                    quoted: false,
                    span: [11, 17],
                }),
                [11, 17],
                None,
                Some(3),
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [18, 19],
                }),
                [18, 19],
                None,
                None,
            ),
        ],
        0,
    )
}

fn case_0070() -> Case {
    good(
        "raw/update/zero",
        "raw",
        b"UPDATE t SET a=0",
        vec![
            node(Kind::Update, [0, 16], Some(1), None),
            node(Kind::Target, [7, 8], Some(2), Some(3)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [7, 8],
                }),
                [7, 8],
                None,
                None,
            ),
            node(Kind::Assignments, [9, 16], Some(4), None),
            node(Kind::Assignment, [13, 16], Some(5), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [13, 14],
                }),
                [13, 14],
                None,
                Some(6),
            ),
            node(
                Kind::Scalar(ScalarObservation::Integer {
                    sign: "None",
                    digits: [15, 16],
                }),
                [15, 16],
                None,
                None,
            ),
        ],
        0,
    )
}

fn case_0071() -> Case {
    good(
        "raw/update/plus",
        "raw",
        b"UPDATE t SET a=+ /*?;*/ 001",
        vec![
            node(Kind::Update, [0, 27], Some(1), None),
            node(Kind::Target, [7, 8], Some(2), Some(3)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [7, 8],
                }),
                [7, 8],
                None,
                None,
            ),
            node(Kind::Assignments, [9, 27], Some(4), None),
            node(Kind::Assignment, [13, 27], Some(5), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [13, 14],
                }),
                [13, 14],
                None,
                Some(6),
            ),
            node(
                Kind::Scalar(ScalarObservation::Integer {
                    sign: "Plus",
                    digits: [24, 27],
                }),
                [15, 27],
                None,
                None,
            ),
        ],
        0,
    )
}

fn case_0072() -> Case {
    good(
        "raw/update/minus",
        "raw",
        b"UPDATE t SET a=- --?;\x0a12",
        vec![
            node(Kind::Update, [0, 24], Some(1), None),
            node(Kind::Target, [7, 8], Some(2), Some(3)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [7, 8],
                }),
                [7, 8],
                None,
                None,
            ),
            node(Kind::Assignments, [9, 24], Some(4), None),
            node(Kind::Assignment, [13, 24], Some(5), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [13, 14],
                }),
                [13, 14],
                None,
                Some(6),
            ),
            node(
                Kind::Scalar(ScalarObservation::Integer {
                    sign: "Minus",
                    digits: [22, 24],
                }),
                [15, 24],
                None,
                None,
            ),
        ],
        0,
    )
}

fn case_0073() -> Case {
    good(
        "raw/update/max",
        "raw",
        b"UPDATE t SET a=9223372036854775807",
        vec![
            node(Kind::Update, [0, 34], Some(1), None),
            node(Kind::Target, [7, 8], Some(2), Some(3)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [7, 8],
                }),
                [7, 8],
                None,
                None,
            ),
            node(Kind::Assignments, [9, 34], Some(4), None),
            node(Kind::Assignment, [13, 34], Some(5), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [13, 14],
                }),
                [13, 14],
                None,
                Some(6),
            ),
            node(
                Kind::Scalar(ScalarObservation::Integer {
                    sign: "None",
                    digits: [15, 34],
                }),
                [15, 34],
                None,
                None,
            ),
        ],
        0,
    )
}

fn case_0074() -> Case {
    good(
        "raw/update/overflow",
        "raw",
        b"UPDATE t SET a=9223372036854775808",
        vec![
            node(Kind::Update, [0, 34], Some(1), None),
            node(Kind::Target, [7, 8], Some(2), Some(3)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [7, 8],
                }),
                [7, 8],
                None,
                None,
            ),
            node(Kind::Assignments, [9, 34], Some(4), None),
            node(Kind::Assignment, [13, 34], Some(5), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [13, 14],
                }),
                [13, 14],
                None,
                Some(6),
            ),
            node(
                Kind::Scalar(ScalarObservation::Integer {
                    sign: "None",
                    digits: [15, 34],
                }),
                [15, 34],
                None,
                None,
            ),
        ],
        0,
    )
}

fn case_0075() -> Case {
    good(
        "raw/update/min",
        "raw",
        b"UPDATE t SET a=-9223372036854775808",
        vec![
            node(Kind::Update, [0, 35], Some(1), None),
            node(Kind::Target, [7, 8], Some(2), Some(3)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [7, 8],
                }),
                [7, 8],
                None,
                None,
            ),
            node(Kind::Assignments, [9, 35], Some(4), None),
            node(Kind::Assignment, [13, 35], Some(5), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [13, 14],
                }),
                [13, 14],
                None,
                Some(6),
            ),
            node(
                Kind::Scalar(ScalarObservation::Integer {
                    sign: "Minus",
                    digits: [16, 35],
                }),
                [15, 35],
                None,
                None,
            ),
        ],
        0,
    )
}

fn case_0076() -> Case {
    good(
        "raw/update/below-min",
        "raw",
        b"UPDATE t SET a=-9223372036854775809",
        vec![
            node(Kind::Update, [0, 35], Some(1), None),
            node(Kind::Target, [7, 8], Some(2), Some(3)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [7, 8],
                }),
                [7, 8],
                None,
                None,
            ),
            node(Kind::Assignments, [9, 35], Some(4), None),
            node(Kind::Assignment, [13, 35], Some(5), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [13, 14],
                }),
                [13, 14],
                None,
                Some(6),
            ),
            node(
                Kind::Scalar(ScalarObservation::Integer {
                    sign: "Minus",
                    digits: [16, 35],
                }),
                [15, 35],
                None,
                None,
            ),
        ],
        0,
    )
}

fn case_0077() -> Case {
    good(
        "raw/update/text-empty",
        "raw",
        b"UPDATE t SET a=''",
        vec![
            node(Kind::Update, [0, 17], Some(1), None),
            node(Kind::Target, [7, 8], Some(2), Some(3)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [7, 8],
                }),
                [7, 8],
                None,
                None,
            ),
            node(Kind::Assignments, [9, 17], Some(4), None),
            node(Kind::Assignment, [13, 17], Some(5), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [13, 14],
                }),
                [13, 14],
                None,
                Some(6),
            ),
            node(Kind::Scalar(ScalarObservation::Text), [15, 17], None, None),
        ],
        0,
    )
}

fn case_0078() -> Case {
    good(
        "raw/update/text-escape",
        "raw",
        b"UPDATE t SET a='a''b?;'",
        vec![
            node(Kind::Update, [0, 23], Some(1), None),
            node(Kind::Target, [7, 8], Some(2), Some(3)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [7, 8],
                }),
                [7, 8],
                None,
                None,
            ),
            node(Kind::Assignments, [9, 23], Some(4), None),
            node(Kind::Assignment, [13, 23], Some(5), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [13, 14],
                }),
                [13, 14],
                None,
                Some(6),
            ),
            node(Kind::Scalar(ScalarObservation::Text), [15, 23], None, None),
        ],
        0,
    )
}

fn case_0079() -> Case {
    good(
        "raw/update/text-nul",
        "raw",
        b"UPDATE t SET a='a\x00b'",
        vec![
            node(Kind::Update, [0, 20], Some(1), None),
            node(Kind::Target, [7, 8], Some(2), Some(3)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [7, 8],
                }),
                [7, 8],
                None,
                None,
            ),
            node(Kind::Assignments, [9, 20], Some(4), None),
            node(Kind::Assignment, [13, 20], Some(5), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [13, 14],
                }),
                [13, 14],
                None,
                Some(6),
            ),
            node(Kind::Scalar(ScalarObservation::Text), [15, 20], None, None),
        ],
        0,
    )
}

fn case_0080() -> Case {
    good(
        "raw/update/binary-empty",
        "raw",
        b"UPDATE t SET a=X''",
        vec![
            node(Kind::Update, [0, 18], Some(1), None),
            node(Kind::Target, [7, 8], Some(2), Some(3)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [7, 8],
                }),
                [7, 8],
                None,
                None,
            ),
            node(Kind::Assignments, [9, 18], Some(4), None),
            node(Kind::Assignment, [13, 18], Some(5), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [13, 14],
                }),
                [13, 14],
                None,
                Some(6),
            ),
            node(
                Kind::Scalar(ScalarObservation::Binary),
                [15, 18],
                None,
                None,
            ),
        ],
        0,
    )
}

fn case_0081() -> Case {
    good(
        "raw/update/binary-odd",
        "raw",
        b"UPDATE t SET a=X'0'",
        vec![
            node(Kind::Update, [0, 19], Some(1), None),
            node(Kind::Target, [7, 8], Some(2), Some(3)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [7, 8],
                }),
                [7, 8],
                None,
                None,
            ),
            node(Kind::Assignments, [9, 19], Some(4), None),
            node(Kind::Assignment, [13, 19], Some(5), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [13, 14],
                }),
                [13, 14],
                None,
                Some(6),
            ),
            node(
                Kind::Scalar(ScalarObservation::Binary),
                [15, 19],
                None,
                None,
            ),
        ],
        0,
    )
}

fn case_0082() -> Case {
    good(
        "raw/update/binary-invalid",
        "raw",
        b"UPDATE t SET a=X'zz'",
        vec![
            node(Kind::Update, [0, 20], Some(1), None),
            node(Kind::Target, [7, 8], Some(2), Some(3)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [7, 8],
                }),
                [7, 8],
                None,
                None,
            ),
            node(Kind::Assignments, [9, 20], Some(4), None),
            node(Kind::Assignment, [13, 20], Some(5), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [13, 14],
                }),
                [13, 14],
                None,
                Some(6),
            ),
            node(
                Kind::Scalar(ScalarObservation::Binary),
                [15, 20],
                None,
                None,
            ),
        ],
        0,
    )
}

fn case_0083() -> Case {
    good(
        "raw/update/true",
        "raw",
        b"UPDATE t SET a=TRUE",
        vec![
            node(Kind::Update, [0, 19], Some(1), None),
            node(Kind::Target, [7, 8], Some(2), Some(3)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [7, 8],
                }),
                [7, 8],
                None,
                None,
            ),
            node(Kind::Assignments, [9, 19], Some(4), None),
            node(Kind::Assignment, [13, 19], Some(5), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [13, 14],
                }),
                [13, 14],
                None,
                Some(6),
            ),
            node(
                Kind::Scalar(ScalarObservation::Bool(true)),
                [15, 19],
                None,
                None,
            ),
        ],
        0,
    )
}

fn case_0084() -> Case {
    good(
        "raw/update/false",
        "raw",
        b"UPDATE t SET a=FALSE",
        vec![
            node(Kind::Update, [0, 20], Some(1), None),
            node(Kind::Target, [7, 8], Some(2), Some(3)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [7, 8],
                }),
                [7, 8],
                None,
                None,
            ),
            node(Kind::Assignments, [9, 20], Some(4), None),
            node(Kind::Assignment, [13, 20], Some(5), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [13, 14],
                }),
                [13, 14],
                None,
                Some(6),
            ),
            node(
                Kind::Scalar(ScalarObservation::Bool(false)),
                [15, 20],
                None,
                None,
            ),
        ],
        0,
    )
}

fn case_0085() -> Case {
    good(
        "raw/update/null",
        "raw",
        b"UPDATE t SET a=NULL",
        vec![
            node(Kind::Update, [0, 19], Some(1), None),
            node(Kind::Target, [7, 8], Some(2), Some(3)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [7, 8],
                }),
                [7, 8],
                None,
                None,
            ),
            node(Kind::Assignments, [9, 19], Some(4), None),
            node(Kind::Assignment, [13, 19], Some(5), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [13, 14],
                }),
                [13, 14],
                None,
                Some(6),
            ),
            node(Kind::Scalar(ScalarObservation::Null), [15, 19], None, None),
        ],
        0,
    )
}

fn case_0086() -> Case {
    good(
        "raw/update/marker",
        "raw",
        b"UPDATE t SET a=?",
        vec![
            node(Kind::Update, [0, 16], Some(1), None),
            node(Kind::Target, [7, 8], Some(2), Some(3)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [7, 8],
                }),
                [7, 8],
                None,
                None,
            ),
            node(Kind::Assignments, [9, 16], Some(4), None),
            node(Kind::Assignment, [13, 16], Some(5), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [13, 14],
                }),
                [13, 14],
                None,
                Some(6),
            ),
            node(
                Kind::Scalar(ScalarObservation::Parameter { ordinal: 1 }),
                [15, 16],
                None,
                None,
            ),
        ],
        1,
    )
}

fn case_0087() -> Case {
    good(
        "raw/limit/0",
        "raw",
        b"SELECT * FROM t LIMIT 0",
        vec![
            node(Kind::Select, [0, 23], Some(1), None),
            node(Kind::Projection, [7, 8], Some(2), Some(3)),
            node(Kind::Wildcard, [7, 8], None, None),
            node(Kind::Target, [14, 15], Some(4), Some(5)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [14, 15],
                }),
                [14, 15],
                None,
                None,
            ),
            node(Kind::Limit, [16, 23], Some(6), None),
            node(
                Kind::Scalar(ScalarObservation::Integer {
                    sign: "None",
                    digits: [22, 23],
                }),
                [22, 23],
                None,
                None,
            ),
        ],
        0,
    )
}

fn case_0088() -> Case {
    good(
        "raw/limit/0001",
        "raw",
        b"SELECT * FROM t LIMIT 0001",
        vec![
            node(Kind::Select, [0, 26], Some(1), None),
            node(Kind::Projection, [7, 8], Some(2), Some(3)),
            node(Kind::Wildcard, [7, 8], None, None),
            node(Kind::Target, [14, 15], Some(4), Some(5)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [14, 15],
                }),
                [14, 15],
                None,
                None,
            ),
            node(Kind::Limit, [16, 26], Some(6), None),
            node(
                Kind::Scalar(ScalarObservation::Integer {
                    sign: "None",
                    digits: [22, 26],
                }),
                [22, 26],
                None,
                None,
            ),
        ],
        0,
    )
}

fn case_0089() -> Case {
    good(
        "raw/limit/9223372036854775808",
        "raw",
        b"SELECT * FROM t LIMIT 9223372036854775808",
        vec![
            node(Kind::Select, [0, 41], Some(1), None),
            node(Kind::Projection, [7, 8], Some(2), Some(3)),
            node(Kind::Wildcard, [7, 8], None, None),
            node(Kind::Target, [14, 15], Some(4), Some(5)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [14, 15],
                }),
                [14, 15],
                None,
                None,
            ),
            node(Kind::Limit, [16, 41], Some(6), None),
            node(
                Kind::Scalar(ScalarObservation::Integer {
                    sign: "None",
                    digits: [22, 41],
                }),
                [22, 41],
                None,
                None,
            ),
        ],
        0,
    )
}

fn case_0090() -> Case {
    good(
        "raw/limit/18446744073709551616",
        "raw",
        b"SELECT * FROM t LIMIT 18446744073709551616",
        vec![
            node(Kind::Select, [0, 42], Some(1), None),
            node(Kind::Projection, [7, 8], Some(2), Some(3)),
            node(Kind::Wildcard, [7, 8], None, None),
            node(Kind::Target, [14, 15], Some(4), Some(5)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [14, 15],
                }),
                [14, 15],
                None,
                None,
            ),
            node(Kind::Limit, [16, 42], Some(6), None),
            node(
                Kind::Scalar(ScalarObservation::Integer {
                    sign: "None",
                    digits: [22, 42],
                }),
                [22, 42],
                None,
                None,
            ),
        ],
        0,
    )
}

fn case_0091() -> Case {
    good(
        "raw/type-length/Utf8/zero",
        "raw",
        b"CREATE TABLE t(a VARCHAR(0)) STORAGE DISK",
        vec![
            node(Kind::CreateTable, [0, 41], Some(1), None),
            node(Kind::Target, [13, 14], Some(2), Some(3)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [13, 14],
                }),
                [13, 14],
                None,
                None,
            ),
            node(Kind::TableDefinition, [14, 28], Some(4), Some(7)),
            node(
                Kind::ColumnDefinition {
                    nullability: Nullability::Implicit,
                },
                [15, 27],
                Some(5),
                None,
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [15, 16],
                }),
                [15, 16],
                None,
                Some(6),
            ),
            node(
                Kind::DataType(DataType::Utf8(TypeLength {
                    sign: "None",
                    digits: [25, 26],
                    span: [25, 26],
                })),
                [17, 27],
                None,
                None,
            ),
            node(Kind::StorageDisk, [29, 41], None, None),
        ],
        0,
    )
}

fn case_0092() -> Case {
    good(
        "raw/type-length/Utf8/minus-zero",
        "raw",
        b"CREATE TABLE t(a VARCHAR(-0)) STORAGE DISK",
        vec![
            node(Kind::CreateTable, [0, 42], Some(1), None),
            node(Kind::Target, [13, 14], Some(2), Some(3)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [13, 14],
                }),
                [13, 14],
                None,
                None,
            ),
            node(Kind::TableDefinition, [14, 29], Some(4), Some(7)),
            node(
                Kind::ColumnDefinition {
                    nullability: Nullability::Implicit,
                },
                [15, 28],
                Some(5),
                None,
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [15, 16],
                }),
                [15, 16],
                None,
                Some(6),
            ),
            node(
                Kind::DataType(DataType::Utf8(TypeLength {
                    sign: "Minus",
                    digits: [26, 27],
                    span: [25, 27],
                })),
                [17, 28],
                None,
                None,
            ),
            node(Kind::StorageDisk, [30, 42], None, None),
        ],
        0,
    )
}

fn case_0093() -> Case {
    good(
        "raw/type-length/Utf8/negative",
        "raw",
        b"CREATE TABLE t(a VARCHAR(-1)) STORAGE DISK",
        vec![
            node(Kind::CreateTable, [0, 42], Some(1), None),
            node(Kind::Target, [13, 14], Some(2), Some(3)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [13, 14],
                }),
                [13, 14],
                None,
                None,
            ),
            node(Kind::TableDefinition, [14, 29], Some(4), Some(7)),
            node(
                Kind::ColumnDefinition {
                    nullability: Nullability::Implicit,
                },
                [15, 28],
                Some(5),
                None,
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [15, 16],
                }),
                [15, 16],
                None,
                Some(6),
            ),
            node(
                Kind::DataType(DataType::Utf8(TypeLength {
                    sign: "Minus",
                    digits: [26, 27],
                    span: [25, 27],
                })),
                [17, 28],
                None,
                None,
            ),
            node(Kind::StorageDisk, [30, 42], None, None),
        ],
        0,
    )
}

fn case_0094() -> Case {
    good(
        "raw/type-length/Utf8/large",
        "raw",
        b"CREATE TABLE t(a VARCHAR(65537)) STORAGE DISK",
        vec![
            node(Kind::CreateTable, [0, 45], Some(1), None),
            node(Kind::Target, [13, 14], Some(2), Some(3)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [13, 14],
                }),
                [13, 14],
                None,
                None,
            ),
            node(Kind::TableDefinition, [14, 32], Some(4), Some(7)),
            node(
                Kind::ColumnDefinition {
                    nullability: Nullability::Implicit,
                },
                [15, 31],
                Some(5),
                None,
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [15, 16],
                }),
                [15, 16],
                None,
                Some(6),
            ),
            node(
                Kind::DataType(DataType::Utf8(TypeLength {
                    sign: "None",
                    digits: [25, 30],
                    span: [25, 30],
                })),
                [17, 31],
                None,
                None,
            ),
            node(Kind::StorageDisk, [33, 45], None, None),
        ],
        0,
    )
}

fn case_0095() -> Case {
    good(
        "raw/type-length/Utf8/huge",
        "raw",
        b"CREATE TABLE t(a VARCHAR(9223372036854775808123456789)) STORAGE DISK",
        vec![
            node(Kind::CreateTable, [0, 68], Some(1), None),
            node(Kind::Target, [13, 14], Some(2), Some(3)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [13, 14],
                }),
                [13, 14],
                None,
                None,
            ),
            node(Kind::TableDefinition, [14, 55], Some(4), Some(7)),
            node(
                Kind::ColumnDefinition {
                    nullability: Nullability::Implicit,
                },
                [15, 54],
                Some(5),
                None,
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [15, 16],
                }),
                [15, 16],
                None,
                Some(6),
            ),
            node(
                Kind::DataType(DataType::Utf8(TypeLength {
                    sign: "None",
                    digits: [25, 53],
                    span: [25, 53],
                })),
                [17, 54],
                None,
                None,
            ),
            node(Kind::StorageDisk, [56, 68], None, None),
        ],
        0,
    )
}

fn case_0096() -> Case {
    good(
        "raw/type-length/Utf8/plus-trivia",
        "raw",
        b"CREATE TABLE t(a VARCHAR(+ /*?;*/ 0001)) STORAGE DISK",
        vec![
            node(Kind::CreateTable, [0, 53], Some(1), None),
            node(Kind::Target, [13, 14], Some(2), Some(3)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [13, 14],
                }),
                [13, 14],
                None,
                None,
            ),
            node(Kind::TableDefinition, [14, 40], Some(4), Some(7)),
            node(
                Kind::ColumnDefinition {
                    nullability: Nullability::Implicit,
                },
                [15, 39],
                Some(5),
                None,
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [15, 16],
                }),
                [15, 16],
                None,
                Some(6),
            ),
            node(
                Kind::DataType(DataType::Utf8(TypeLength {
                    sign: "Plus",
                    digits: [34, 38],
                    span: [25, 38],
                })),
                [17, 39],
                None,
                None,
            ),
            node(Kind::StorageDisk, [41, 53], None, None),
        ],
        0,
    )
}

fn case_0097() -> Case {
    good(
        "raw/type-length/Binary/zero",
        "raw",
        b"CREATE TABLE t(a VARBINARY(0)) STORAGE DISK",
        vec![
            node(Kind::CreateTable, [0, 43], Some(1), None),
            node(Kind::Target, [13, 14], Some(2), Some(3)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [13, 14],
                }),
                [13, 14],
                None,
                None,
            ),
            node(Kind::TableDefinition, [14, 30], Some(4), Some(7)),
            node(
                Kind::ColumnDefinition {
                    nullability: Nullability::Implicit,
                },
                [15, 29],
                Some(5),
                None,
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [15, 16],
                }),
                [15, 16],
                None,
                Some(6),
            ),
            node(
                Kind::DataType(DataType::Binary(TypeLength {
                    sign: "None",
                    digits: [27, 28],
                    span: [27, 28],
                })),
                [17, 29],
                None,
                None,
            ),
            node(Kind::StorageDisk, [31, 43], None, None),
        ],
        0,
    )
}

fn case_0098() -> Case {
    good(
        "raw/type-length/Binary/minus-zero",
        "raw",
        b"CREATE TABLE t(a VARBINARY(-0)) STORAGE DISK",
        vec![
            node(Kind::CreateTable, [0, 44], Some(1), None),
            node(Kind::Target, [13, 14], Some(2), Some(3)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [13, 14],
                }),
                [13, 14],
                None,
                None,
            ),
            node(Kind::TableDefinition, [14, 31], Some(4), Some(7)),
            node(
                Kind::ColumnDefinition {
                    nullability: Nullability::Implicit,
                },
                [15, 30],
                Some(5),
                None,
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [15, 16],
                }),
                [15, 16],
                None,
                Some(6),
            ),
            node(
                Kind::DataType(DataType::Binary(TypeLength {
                    sign: "Minus",
                    digits: [28, 29],
                    span: [27, 29],
                })),
                [17, 30],
                None,
                None,
            ),
            node(Kind::StorageDisk, [32, 44], None, None),
        ],
        0,
    )
}

fn case_0099() -> Case {
    good(
        "raw/type-length/Binary/negative",
        "raw",
        b"CREATE TABLE t(a VARBINARY(-1)) STORAGE DISK",
        vec![
            node(Kind::CreateTable, [0, 44], Some(1), None),
            node(Kind::Target, [13, 14], Some(2), Some(3)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [13, 14],
                }),
                [13, 14],
                None,
                None,
            ),
            node(Kind::TableDefinition, [14, 31], Some(4), Some(7)),
            node(
                Kind::ColumnDefinition {
                    nullability: Nullability::Implicit,
                },
                [15, 30],
                Some(5),
                None,
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [15, 16],
                }),
                [15, 16],
                None,
                Some(6),
            ),
            node(
                Kind::DataType(DataType::Binary(TypeLength {
                    sign: "Minus",
                    digits: [28, 29],
                    span: [27, 29],
                })),
                [17, 30],
                None,
                None,
            ),
            node(Kind::StorageDisk, [32, 44], None, None),
        ],
        0,
    )
}

fn case_0100() -> Case {
    good(
        "raw/type-length/Binary/large",
        "raw",
        b"CREATE TABLE t(a VARBINARY(65537)) STORAGE DISK",
        vec![
            node(Kind::CreateTable, [0, 47], Some(1), None),
            node(Kind::Target, [13, 14], Some(2), Some(3)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [13, 14],
                }),
                [13, 14],
                None,
                None,
            ),
            node(Kind::TableDefinition, [14, 34], Some(4), Some(7)),
            node(
                Kind::ColumnDefinition {
                    nullability: Nullability::Implicit,
                },
                [15, 33],
                Some(5),
                None,
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [15, 16],
                }),
                [15, 16],
                None,
                Some(6),
            ),
            node(
                Kind::DataType(DataType::Binary(TypeLength {
                    sign: "None",
                    digits: [27, 32],
                    span: [27, 32],
                })),
                [17, 33],
                None,
                None,
            ),
            node(Kind::StorageDisk, [35, 47], None, None),
        ],
        0,
    )
}

fn case_0101() -> Case {
    good(
        "raw/type-length/Binary/huge",
        "raw",
        b"CREATE TABLE t(a VARBINARY(9223372036854775808123456789)) STORAGE DISK",
        vec![
            node(Kind::CreateTable, [0, 70], Some(1), None),
            node(Kind::Target, [13, 14], Some(2), Some(3)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [13, 14],
                }),
                [13, 14],
                None,
                None,
            ),
            node(Kind::TableDefinition, [14, 57], Some(4), Some(7)),
            node(
                Kind::ColumnDefinition {
                    nullability: Nullability::Implicit,
                },
                [15, 56],
                Some(5),
                None,
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [15, 16],
                }),
                [15, 16],
                None,
                Some(6),
            ),
            node(
                Kind::DataType(DataType::Binary(TypeLength {
                    sign: "None",
                    digits: [27, 55],
                    span: [27, 55],
                })),
                [17, 56],
                None,
                None,
            ),
            node(Kind::StorageDisk, [58, 70], None, None),
        ],
        0,
    )
}

fn case_0102() -> Case {
    good(
        "raw/type-length/Binary/plus-trivia",
        "raw",
        b"CREATE TABLE t(a VARBINARY(+ /*?;*/ 0001)) STORAGE DISK",
        vec![
            node(Kind::CreateTable, [0, 55], Some(1), None),
            node(Kind::Target, [13, 14], Some(2), Some(3)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [13, 14],
                }),
                [13, 14],
                None,
                None,
            ),
            node(Kind::TableDefinition, [14, 42], Some(4), Some(7)),
            node(
                Kind::ColumnDefinition {
                    nullability: Nullability::Implicit,
                },
                [15, 41],
                Some(5),
                None,
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [15, 16],
                }),
                [15, 16],
                None,
                Some(6),
            ),
            node(
                Kind::DataType(DataType::Binary(TypeLength {
                    sign: "Plus",
                    digits: [36, 40],
                    span: [27, 40],
                })),
                [17, 41],
                None,
                None,
            ),
            node(Kind::StorageDisk, [43, 55], None, None),
        ],
        0,
    )
}

fn case_0103() -> Case {
    good(
        "raw/duplicate-columns",
        "raw",
        b"CREATE TABLE t(a BIGINT,a BOOLEAN) STORAGE DISK",
        vec![
            node(Kind::CreateTable, [0, 47], Some(1), None),
            node(Kind::Target, [13, 14], Some(2), Some(3)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [13, 14],
                }),
                [13, 14],
                None,
                None,
            ),
            node(Kind::TableDefinition, [14, 34], Some(4), Some(10)),
            node(
                Kind::ColumnDefinition {
                    nullability: Nullability::Implicit,
                },
                [15, 23],
                Some(5),
                Some(7),
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [15, 16],
                }),
                [15, 16],
                None,
                Some(6),
            ),
            node(Kind::DataType(DataType::Int64), [17, 23], None, None),
            node(
                Kind::ColumnDefinition {
                    nullability: Nullability::Implicit,
                },
                [24, 33],
                Some(8),
                None,
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [24, 25],
                }),
                [24, 25],
                None,
                Some(9),
            ),
            node(Kind::DataType(DataType::Bool), [26, 33], None, None),
            node(Kind::StorageDisk, [35, 47], None, None),
        ],
        0,
    )
}

fn case_0104() -> Case {
    good(
        "raw/pk-explicit-null",
        "raw",
        b"CREATE TABLE t(a BIGINT NULL,PRIMARY KEY (a)) STORAGE DISK",
        vec![
            node(Kind::CreateTable, [0, 58], Some(1), None),
            node(Kind::Target, [13, 14], Some(2), Some(3)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [13, 14],
                }),
                [13, 14],
                None,
                None,
            ),
            node(Kind::TableDefinition, [14, 45], Some(4), Some(10)),
            node(
                Kind::ColumnDefinition {
                    nullability: Nullability::Null,
                },
                [15, 28],
                Some(5),
                Some(7),
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [15, 16],
                }),
                [15, 16],
                None,
                Some(6),
            ),
            node(Kind::DataType(DataType::Int64), [17, 23], None, None),
            node(Kind::PrimaryKey, [29, 44], Some(8), None),
            node(Kind::Columns, [41, 44], Some(9), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [42, 43],
                }),
                [42, 43],
                None,
                None,
            ),
            node(Kind::StorageDisk, [46, 58], None, None),
        ],
        0,
    )
}

fn case_0105() -> Case {
    good(
        "raw/missing-key-column",
        "raw",
        b"CREATE TABLE t(a BIGINT,PRIMARY KEY (b),UNIQUE (a,a)) STORAGE DISK",
        vec![
            node(Kind::CreateTable, [0, 66], Some(1), None),
            node(Kind::Target, [13, 14], Some(2), Some(3)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [13, 14],
                }),
                [13, 14],
                None,
                None,
            ),
            node(Kind::TableDefinition, [14, 53], Some(4), Some(14)),
            node(
                Kind::ColumnDefinition {
                    nullability: Nullability::Implicit,
                },
                [15, 23],
                Some(5),
                Some(7),
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [15, 16],
                }),
                [15, 16],
                None,
                Some(6),
            ),
            node(Kind::DataType(DataType::Int64), [17, 23], None, None),
            node(Kind::PrimaryKey, [24, 39], Some(8), Some(10)),
            node(Kind::Columns, [36, 39], Some(9), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"B".to_vec(),
                    quoted: false,
                    span: [37, 38],
                }),
                [37, 38],
                None,
                None,
            ),
            node(Kind::UniqueConstraint, [40, 52], Some(11), None),
            node(Kind::Columns, [47, 52], Some(12), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [48, 49],
                }),
                [48, 49],
                None,
                Some(13),
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [50, 51],
                }),
                [50, 51],
                None,
                None,
            ),
            node(Kind::StorageDisk, [54, 66], None, None),
        ],
        0,
    )
}

fn case_0106() -> Case {
    good("raw/33-columns", "raw", b"CREATE TABLE t(c0 BIGINT,c1 BIGINT,c2 BIGINT,c3 BIGINT,c4 BIGINT,c5 BIGINT,c6 BIGINT,c7 BIGINT,c8 BIGINT,c9 BIGINT,c10 BIGINT,c11 BIGINT,c12 BIGINT,c13 BIGINT,c14 BIGINT,c15 BIGINT,c16 BIGINT,c17 BIGINT,c18 BIGINT,c19 BIGINT,c20 BIGINT,c21 BIGINT,c22 BIGINT,c23 BIGINT,c24 BIGINT,c25 BIGINT,c26 BIGINT,c27 BIGINT,c28 BIGINT,c29 BIGINT,c30 BIGINT,c31 BIGINT,c32 BIGINT) STORAGE DISK", vec![
        node(Kind::CreateTable, [0, 381], Some(1), None),
        node(Kind::Target, [13, 14], Some(2), Some(3)),
        node(Kind::Name(NameObservation { bytes: b"T".to_vec(), quoted: false, span: [13, 14] }), [13, 14], None, None),
        node(Kind::TableDefinition, [14, 368], Some(4), Some(103)),
        node(Kind::ColumnDefinition { nullability: Nullability::Implicit }, [15, 24], Some(5), Some(7)),
        node(Kind::Name(NameObservation { bytes: b"C0".to_vec(), quoted: false, span: [15, 17] }), [15, 17], None, Some(6)),
        node(Kind::DataType(DataType::Int64), [18, 24], None, None),
        node(Kind::ColumnDefinition { nullability: Nullability::Implicit }, [25, 34], Some(8), Some(10)),
        node(Kind::Name(NameObservation { bytes: b"C1".to_vec(), quoted: false, span: [25, 27] }), [25, 27], None, Some(9)),
        node(Kind::DataType(DataType::Int64), [28, 34], None, None),
        node(Kind::ColumnDefinition { nullability: Nullability::Implicit }, [35, 44], Some(11), Some(13)),
        node(Kind::Name(NameObservation { bytes: b"C2".to_vec(), quoted: false, span: [35, 37] }), [35, 37], None, Some(12)),
        node(Kind::DataType(DataType::Int64), [38, 44], None, None),
        node(Kind::ColumnDefinition { nullability: Nullability::Implicit }, [45, 54], Some(14), Some(16)),
        node(Kind::Name(NameObservation { bytes: b"C3".to_vec(), quoted: false, span: [45, 47] }), [45, 47], None, Some(15)),
        node(Kind::DataType(DataType::Int64), [48, 54], None, None),
        node(Kind::ColumnDefinition { nullability: Nullability::Implicit }, [55, 64], Some(17), Some(19)),
        node(Kind::Name(NameObservation { bytes: b"C4".to_vec(), quoted: false, span: [55, 57] }), [55, 57], None, Some(18)),
        node(Kind::DataType(DataType::Int64), [58, 64], None, None),
        node(Kind::ColumnDefinition { nullability: Nullability::Implicit }, [65, 74], Some(20), Some(22)),
        node(Kind::Name(NameObservation { bytes: b"C5".to_vec(), quoted: false, span: [65, 67] }), [65, 67], None, Some(21)),
        node(Kind::DataType(DataType::Int64), [68, 74], None, None),
        node(Kind::ColumnDefinition { nullability: Nullability::Implicit }, [75, 84], Some(23), Some(25)),
        node(Kind::Name(NameObservation { bytes: b"C6".to_vec(), quoted: false, span: [75, 77] }), [75, 77], None, Some(24)),
        node(Kind::DataType(DataType::Int64), [78, 84], None, None),
        node(Kind::ColumnDefinition { nullability: Nullability::Implicit }, [85, 94], Some(26), Some(28)),
        node(Kind::Name(NameObservation { bytes: b"C7".to_vec(), quoted: false, span: [85, 87] }), [85, 87], None, Some(27)),
        node(Kind::DataType(DataType::Int64), [88, 94], None, None),
        node(Kind::ColumnDefinition { nullability: Nullability::Implicit }, [95, 104], Some(29), Some(31)),
        node(Kind::Name(NameObservation { bytes: b"C8".to_vec(), quoted: false, span: [95, 97] }), [95, 97], None, Some(30)),
        node(Kind::DataType(DataType::Int64), [98, 104], None, None),
        node(Kind::ColumnDefinition { nullability: Nullability::Implicit }, [105, 114], Some(32), Some(34)),
        node(Kind::Name(NameObservation { bytes: b"C9".to_vec(), quoted: false, span: [105, 107] }), [105, 107], None, Some(33)),
        node(Kind::DataType(DataType::Int64), [108, 114], None, None),
        node(Kind::ColumnDefinition { nullability: Nullability::Implicit }, [115, 125], Some(35), Some(37)),
        node(Kind::Name(NameObservation { bytes: b"C10".to_vec(), quoted: false, span: [115, 118] }), [115, 118], None, Some(36)),
        node(Kind::DataType(DataType::Int64), [119, 125], None, None),
        node(Kind::ColumnDefinition { nullability: Nullability::Implicit }, [126, 136], Some(38), Some(40)),
        node(Kind::Name(NameObservation { bytes: b"C11".to_vec(), quoted: false, span: [126, 129] }), [126, 129], None, Some(39)),
        node(Kind::DataType(DataType::Int64), [130, 136], None, None),
        node(Kind::ColumnDefinition { nullability: Nullability::Implicit }, [137, 147], Some(41), Some(43)),
        node(Kind::Name(NameObservation { bytes: b"C12".to_vec(), quoted: false, span: [137, 140] }), [137, 140], None, Some(42)),
        node(Kind::DataType(DataType::Int64), [141, 147], None, None),
        node(Kind::ColumnDefinition { nullability: Nullability::Implicit }, [148, 158], Some(44), Some(46)),
        node(Kind::Name(NameObservation { bytes: b"C13".to_vec(), quoted: false, span: [148, 151] }), [148, 151], None, Some(45)),
        node(Kind::DataType(DataType::Int64), [152, 158], None, None),
        node(Kind::ColumnDefinition { nullability: Nullability::Implicit }, [159, 169], Some(47), Some(49)),
        node(Kind::Name(NameObservation { bytes: b"C14".to_vec(), quoted: false, span: [159, 162] }), [159, 162], None, Some(48)),
        node(Kind::DataType(DataType::Int64), [163, 169], None, None),
        node(Kind::ColumnDefinition { nullability: Nullability::Implicit }, [170, 180], Some(50), Some(52)),
        node(Kind::Name(NameObservation { bytes: b"C15".to_vec(), quoted: false, span: [170, 173] }), [170, 173], None, Some(51)),
        node(Kind::DataType(DataType::Int64), [174, 180], None, None),
        node(Kind::ColumnDefinition { nullability: Nullability::Implicit }, [181, 191], Some(53), Some(55)),
        node(Kind::Name(NameObservation { bytes: b"C16".to_vec(), quoted: false, span: [181, 184] }), [181, 184], None, Some(54)),
        node(Kind::DataType(DataType::Int64), [185, 191], None, None),
        node(Kind::ColumnDefinition { nullability: Nullability::Implicit }, [192, 202], Some(56), Some(58)),
        node(Kind::Name(NameObservation { bytes: b"C17".to_vec(), quoted: false, span: [192, 195] }), [192, 195], None, Some(57)),
        node(Kind::DataType(DataType::Int64), [196, 202], None, None),
        node(Kind::ColumnDefinition { nullability: Nullability::Implicit }, [203, 213], Some(59), Some(61)),
        node(Kind::Name(NameObservation { bytes: b"C18".to_vec(), quoted: false, span: [203, 206] }), [203, 206], None, Some(60)),
        node(Kind::DataType(DataType::Int64), [207, 213], None, None),
        node(Kind::ColumnDefinition { nullability: Nullability::Implicit }, [214, 224], Some(62), Some(64)),
        node(Kind::Name(NameObservation { bytes: b"C19".to_vec(), quoted: false, span: [214, 217] }), [214, 217], None, Some(63)),
        node(Kind::DataType(DataType::Int64), [218, 224], None, None),
        node(Kind::ColumnDefinition { nullability: Nullability::Implicit }, [225, 235], Some(65), Some(67)),
        node(Kind::Name(NameObservation { bytes: b"C20".to_vec(), quoted: false, span: [225, 228] }), [225, 228], None, Some(66)),
        node(Kind::DataType(DataType::Int64), [229, 235], None, None),
        node(Kind::ColumnDefinition { nullability: Nullability::Implicit }, [236, 246], Some(68), Some(70)),
        node(Kind::Name(NameObservation { bytes: b"C21".to_vec(), quoted: false, span: [236, 239] }), [236, 239], None, Some(69)),
        node(Kind::DataType(DataType::Int64), [240, 246], None, None),
        node(Kind::ColumnDefinition { nullability: Nullability::Implicit }, [247, 257], Some(71), Some(73)),
        node(Kind::Name(NameObservation { bytes: b"C22".to_vec(), quoted: false, span: [247, 250] }), [247, 250], None, Some(72)),
        node(Kind::DataType(DataType::Int64), [251, 257], None, None),
        node(Kind::ColumnDefinition { nullability: Nullability::Implicit }, [258, 268], Some(74), Some(76)),
        node(Kind::Name(NameObservation { bytes: b"C23".to_vec(), quoted: false, span: [258, 261] }), [258, 261], None, Some(75)),
        node(Kind::DataType(DataType::Int64), [262, 268], None, None),
        node(Kind::ColumnDefinition { nullability: Nullability::Implicit }, [269, 279], Some(77), Some(79)),
        node(Kind::Name(NameObservation { bytes: b"C24".to_vec(), quoted: false, span: [269, 272] }), [269, 272], None, Some(78)),
        node(Kind::DataType(DataType::Int64), [273, 279], None, None),
        node(Kind::ColumnDefinition { nullability: Nullability::Implicit }, [280, 290], Some(80), Some(82)),
        node(Kind::Name(NameObservation { bytes: b"C25".to_vec(), quoted: false, span: [280, 283] }), [280, 283], None, Some(81)),
        node(Kind::DataType(DataType::Int64), [284, 290], None, None),
        node(Kind::ColumnDefinition { nullability: Nullability::Implicit }, [291, 301], Some(83), Some(85)),
        node(Kind::Name(NameObservation { bytes: b"C26".to_vec(), quoted: false, span: [291, 294] }), [291, 294], None, Some(84)),
        node(Kind::DataType(DataType::Int64), [295, 301], None, None),
        node(Kind::ColumnDefinition { nullability: Nullability::Implicit }, [302, 312], Some(86), Some(88)),
        node(Kind::Name(NameObservation { bytes: b"C27".to_vec(), quoted: false, span: [302, 305] }), [302, 305], None, Some(87)),
        node(Kind::DataType(DataType::Int64), [306, 312], None, None),
        node(Kind::ColumnDefinition { nullability: Nullability::Implicit }, [313, 323], Some(89), Some(91)),
        node(Kind::Name(NameObservation { bytes: b"C28".to_vec(), quoted: false, span: [313, 316] }), [313, 316], None, Some(90)),
        node(Kind::DataType(DataType::Int64), [317, 323], None, None),
        node(Kind::ColumnDefinition { nullability: Nullability::Implicit }, [324, 334], Some(92), Some(94)),
        node(Kind::Name(NameObservation { bytes: b"C29".to_vec(), quoted: false, span: [324, 327] }), [324, 327], None, Some(93)),
        node(Kind::DataType(DataType::Int64), [328, 334], None, None),
        node(Kind::ColumnDefinition { nullability: Nullability::Implicit }, [335, 345], Some(95), Some(97)),
        node(Kind::Name(NameObservation { bytes: b"C30".to_vec(), quoted: false, span: [335, 338] }), [335, 338], None, Some(96)),
        node(Kind::DataType(DataType::Int64), [339, 345], None, None),
        node(Kind::ColumnDefinition { nullability: Nullability::Implicit }, [346, 356], Some(98), Some(100)),
        node(Kind::Name(NameObservation { bytes: b"C31".to_vec(), quoted: false, span: [346, 349] }), [346, 349], None, Some(99)),
        node(Kind::DataType(DataType::Int64), [350, 356], None, None),
        node(Kind::ColumnDefinition { nullability: Nullability::Implicit }, [357, 367], Some(101), None),
        node(Kind::Name(NameObservation { bytes: b"C32".to_vec(), quoted: false, span: [357, 360] }), [357, 360], None, Some(102)),
        node(Kind::DataType(DataType::Int64), [361, 367], None, None),
        node(Kind::StorageDisk, [369, 381], None, None),
    ], 0)
}

fn case_0107() -> Case {
    good("raw/9-unique", "raw", b"CREATE TABLE t(a BIGINT,UNIQUE (a),UNIQUE (a),UNIQUE (a),UNIQUE (a),UNIQUE (a),UNIQUE (a),UNIQUE (a),UNIQUE (a),UNIQUE (a)) STORAGE DISK", vec![
        node(Kind::CreateTable, [0, 136], Some(1), None),
        node(Kind::Target, [13, 14], Some(2), Some(3)),
        node(Kind::Name(NameObservation { bytes: b"T".to_vec(), quoted: false, span: [13, 14] }), [13, 14], None, None),
        node(Kind::TableDefinition, [14, 123], Some(4), Some(34)),
        node(Kind::ColumnDefinition { nullability: Nullability::Implicit }, [15, 23], Some(5), Some(7)),
        node(Kind::Name(NameObservation { bytes: b"A".to_vec(), quoted: false, span: [15, 16] }), [15, 16], None, Some(6)),
        node(Kind::DataType(DataType::Int64), [17, 23], None, None),
        node(Kind::UniqueConstraint, [24, 34], Some(8), Some(10)),
        node(Kind::Columns, [31, 34], Some(9), None),
        node(Kind::Name(NameObservation { bytes: b"A".to_vec(), quoted: false, span: [32, 33] }), [32, 33], None, None),
        node(Kind::UniqueConstraint, [35, 45], Some(11), Some(13)),
        node(Kind::Columns, [42, 45], Some(12), None),
        node(Kind::Name(NameObservation { bytes: b"A".to_vec(), quoted: false, span: [43, 44] }), [43, 44], None, None),
        node(Kind::UniqueConstraint, [46, 56], Some(14), Some(16)),
        node(Kind::Columns, [53, 56], Some(15), None),
        node(Kind::Name(NameObservation { bytes: b"A".to_vec(), quoted: false, span: [54, 55] }), [54, 55], None, None),
        node(Kind::UniqueConstraint, [57, 67], Some(17), Some(19)),
        node(Kind::Columns, [64, 67], Some(18), None),
        node(Kind::Name(NameObservation { bytes: b"A".to_vec(), quoted: false, span: [65, 66] }), [65, 66], None, None),
        node(Kind::UniqueConstraint, [68, 78], Some(20), Some(22)),
        node(Kind::Columns, [75, 78], Some(21), None),
        node(Kind::Name(NameObservation { bytes: b"A".to_vec(), quoted: false, span: [76, 77] }), [76, 77], None, None),
        node(Kind::UniqueConstraint, [79, 89], Some(23), Some(25)),
        node(Kind::Columns, [86, 89], Some(24), None),
        node(Kind::Name(NameObservation { bytes: b"A".to_vec(), quoted: false, span: [87, 88] }), [87, 88], None, None),
        node(Kind::UniqueConstraint, [90, 100], Some(26), Some(28)),
        node(Kind::Columns, [97, 100], Some(27), None),
        node(Kind::Name(NameObservation { bytes: b"A".to_vec(), quoted: false, span: [98, 99] }), [98, 99], None, None),
        node(Kind::UniqueConstraint, [101, 111], Some(29), Some(31)),
        node(Kind::Columns, [108, 111], Some(30), None),
        node(Kind::Name(NameObservation { bytes: b"A".to_vec(), quoted: false, span: [109, 110] }), [109, 110], None, None),
        node(Kind::UniqueConstraint, [112, 122], Some(32), None),
        node(Kind::Columns, [119, 122], Some(33), None),
        node(Kind::Name(NameObservation { bytes: b"A".to_vec(), quoted: false, span: [120, 121] }), [120, 121], None, None),
        node(Kind::StorageDisk, [124, 136], None, None),
    ], 0)
}

fn case_0108() -> Case {
    good(
        "raw/5-primary-key-names",
        "raw",
        b"CREATE TABLE t(a BIGINT,PRIMARY KEY (a,a,a,a,a)) STORAGE DISK",
        vec![
            node(Kind::CreateTable, [0, 61], Some(1), None),
            node(Kind::Target, [13, 14], Some(2), Some(3)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [13, 14],
                }),
                [13, 14],
                None,
                None,
            ),
            node(Kind::TableDefinition, [14, 48], Some(4), Some(14)),
            node(
                Kind::ColumnDefinition {
                    nullability: Nullability::Implicit,
                },
                [15, 23],
                Some(5),
                Some(7),
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [15, 16],
                }),
                [15, 16],
                None,
                Some(6),
            ),
            node(Kind::DataType(DataType::Int64), [17, 23], None, None),
            node(Kind::PrimaryKey, [24, 47], Some(8), None),
            node(Kind::Columns, [36, 47], Some(9), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [37, 38],
                }),
                [37, 38],
                None,
                Some(10),
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [39, 40],
                }),
                [39, 40],
                None,
                Some(11),
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [41, 42],
                }),
                [41, 42],
                None,
                Some(12),
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [43, 44],
                }),
                [43, 44],
                None,
                Some(13),
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [45, 46],
                }),
                [45, 46],
                None,
                None,
            ),
            node(Kind::StorageDisk, [49, 61], None, None),
        ],
        0,
    )
}

fn case_0109() -> Case {
    good(
        "raw/5-index-key-names",
        "raw",
        b"CREATE INDEX i ON t(a,a,a,a,a)",
        vec![
            node(Kind::CreateIndex { unique: false }, [0, 30], Some(1), None),
            node(Kind::Target, [13, 14], Some(2), Some(3)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"I".to_vec(),
                    quoted: false,
                    span: [13, 14],
                }),
                [13, 14],
                None,
                None,
            ),
            node(Kind::Target, [18, 19], Some(4), Some(5)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [18, 19],
                }),
                [18, 19],
                None,
                None,
            ),
            node(Kind::Columns, [19, 30], Some(6), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [20, 21],
                }),
                [20, 21],
                None,
                Some(7),
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [22, 23],
                }),
                [22, 23],
                None,
                Some(8),
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [24, 25],
                }),
                [24, 25],
                None,
                Some(9),
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [26, 27],
                }),
                [26, 27],
                None,
                Some(10),
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [28, 29],
                }),
                [28, 29],
                None,
                None,
            ),
        ],
        0,
    )
}

fn case_0110() -> Case {
    good("raw/33-assignments", "raw", b"UPDATE t SET a=0,a=0,a=0,a=0,a=0,a=0,a=0,a=0,a=0,a=0,a=0,a=0,a=0,a=0,a=0,a=0,a=0,a=0,a=0,a=0,a=0,a=0,a=0,a=0,a=0,a=0,a=0,a=0,a=0,a=0,a=0,a=0,a=0", vec![
        node(Kind::Update, [0, 144], Some(1), None),
        node(Kind::Target, [7, 8], Some(2), Some(3)),
        node(Kind::Name(NameObservation { bytes: b"T".to_vec(), quoted: false, span: [7, 8] }), [7, 8], None, None),
        node(Kind::Assignments, [9, 144], Some(4), None),
        node(Kind::Assignment, [13, 16], Some(5), Some(7)),
        node(Kind::Name(NameObservation { bytes: b"A".to_vec(), quoted: false, span: [13, 14] }), [13, 14], None, Some(6)),
        node(Kind::Scalar(ScalarObservation::Integer { sign: "None", digits: [15, 16] }), [15, 16], None, None),
        node(Kind::Assignment, [17, 20], Some(8), Some(10)),
        node(Kind::Name(NameObservation { bytes: b"A".to_vec(), quoted: false, span: [17, 18] }), [17, 18], None, Some(9)),
        node(Kind::Scalar(ScalarObservation::Integer { sign: "None", digits: [19, 20] }), [19, 20], None, None),
        node(Kind::Assignment, [21, 24], Some(11), Some(13)),
        node(Kind::Name(NameObservation { bytes: b"A".to_vec(), quoted: false, span: [21, 22] }), [21, 22], None, Some(12)),
        node(Kind::Scalar(ScalarObservation::Integer { sign: "None", digits: [23, 24] }), [23, 24], None, None),
        node(Kind::Assignment, [25, 28], Some(14), Some(16)),
        node(Kind::Name(NameObservation { bytes: b"A".to_vec(), quoted: false, span: [25, 26] }), [25, 26], None, Some(15)),
        node(Kind::Scalar(ScalarObservation::Integer { sign: "None", digits: [27, 28] }), [27, 28], None, None),
        node(Kind::Assignment, [29, 32], Some(17), Some(19)),
        node(Kind::Name(NameObservation { bytes: b"A".to_vec(), quoted: false, span: [29, 30] }), [29, 30], None, Some(18)),
        node(Kind::Scalar(ScalarObservation::Integer { sign: "None", digits: [31, 32] }), [31, 32], None, None),
        node(Kind::Assignment, [33, 36], Some(20), Some(22)),
        node(Kind::Name(NameObservation { bytes: b"A".to_vec(), quoted: false, span: [33, 34] }), [33, 34], None, Some(21)),
        node(Kind::Scalar(ScalarObservation::Integer { sign: "None", digits: [35, 36] }), [35, 36], None, None),
        node(Kind::Assignment, [37, 40], Some(23), Some(25)),
        node(Kind::Name(NameObservation { bytes: b"A".to_vec(), quoted: false, span: [37, 38] }), [37, 38], None, Some(24)),
        node(Kind::Scalar(ScalarObservation::Integer { sign: "None", digits: [39, 40] }), [39, 40], None, None),
        node(Kind::Assignment, [41, 44], Some(26), Some(28)),
        node(Kind::Name(NameObservation { bytes: b"A".to_vec(), quoted: false, span: [41, 42] }), [41, 42], None, Some(27)),
        node(Kind::Scalar(ScalarObservation::Integer { sign: "None", digits: [43, 44] }), [43, 44], None, None),
        node(Kind::Assignment, [45, 48], Some(29), Some(31)),
        node(Kind::Name(NameObservation { bytes: b"A".to_vec(), quoted: false, span: [45, 46] }), [45, 46], None, Some(30)),
        node(Kind::Scalar(ScalarObservation::Integer { sign: "None", digits: [47, 48] }), [47, 48], None, None),
        node(Kind::Assignment, [49, 52], Some(32), Some(34)),
        node(Kind::Name(NameObservation { bytes: b"A".to_vec(), quoted: false, span: [49, 50] }), [49, 50], None, Some(33)),
        node(Kind::Scalar(ScalarObservation::Integer { sign: "None", digits: [51, 52] }), [51, 52], None, None),
        node(Kind::Assignment, [53, 56], Some(35), Some(37)),
        node(Kind::Name(NameObservation { bytes: b"A".to_vec(), quoted: false, span: [53, 54] }), [53, 54], None, Some(36)),
        node(Kind::Scalar(ScalarObservation::Integer { sign: "None", digits: [55, 56] }), [55, 56], None, None),
        node(Kind::Assignment, [57, 60], Some(38), Some(40)),
        node(Kind::Name(NameObservation { bytes: b"A".to_vec(), quoted: false, span: [57, 58] }), [57, 58], None, Some(39)),
        node(Kind::Scalar(ScalarObservation::Integer { sign: "None", digits: [59, 60] }), [59, 60], None, None),
        node(Kind::Assignment, [61, 64], Some(41), Some(43)),
        node(Kind::Name(NameObservation { bytes: b"A".to_vec(), quoted: false, span: [61, 62] }), [61, 62], None, Some(42)),
        node(Kind::Scalar(ScalarObservation::Integer { sign: "None", digits: [63, 64] }), [63, 64], None, None),
        node(Kind::Assignment, [65, 68], Some(44), Some(46)),
        node(Kind::Name(NameObservation { bytes: b"A".to_vec(), quoted: false, span: [65, 66] }), [65, 66], None, Some(45)),
        node(Kind::Scalar(ScalarObservation::Integer { sign: "None", digits: [67, 68] }), [67, 68], None, None),
        node(Kind::Assignment, [69, 72], Some(47), Some(49)),
        node(Kind::Name(NameObservation { bytes: b"A".to_vec(), quoted: false, span: [69, 70] }), [69, 70], None, Some(48)),
        node(Kind::Scalar(ScalarObservation::Integer { sign: "None", digits: [71, 72] }), [71, 72], None, None),
        node(Kind::Assignment, [73, 76], Some(50), Some(52)),
        node(Kind::Name(NameObservation { bytes: b"A".to_vec(), quoted: false, span: [73, 74] }), [73, 74], None, Some(51)),
        node(Kind::Scalar(ScalarObservation::Integer { sign: "None", digits: [75, 76] }), [75, 76], None, None),
        node(Kind::Assignment, [77, 80], Some(53), Some(55)),
        node(Kind::Name(NameObservation { bytes: b"A".to_vec(), quoted: false, span: [77, 78] }), [77, 78], None, Some(54)),
        node(Kind::Scalar(ScalarObservation::Integer { sign: "None", digits: [79, 80] }), [79, 80], None, None),
        node(Kind::Assignment, [81, 84], Some(56), Some(58)),
        node(Kind::Name(NameObservation { bytes: b"A".to_vec(), quoted: false, span: [81, 82] }), [81, 82], None, Some(57)),
        node(Kind::Scalar(ScalarObservation::Integer { sign: "None", digits: [83, 84] }), [83, 84], None, None),
        node(Kind::Assignment, [85, 88], Some(59), Some(61)),
        node(Kind::Name(NameObservation { bytes: b"A".to_vec(), quoted: false, span: [85, 86] }), [85, 86], None, Some(60)),
        node(Kind::Scalar(ScalarObservation::Integer { sign: "None", digits: [87, 88] }), [87, 88], None, None),
        node(Kind::Assignment, [89, 92], Some(62), Some(64)),
        node(Kind::Name(NameObservation { bytes: b"A".to_vec(), quoted: false, span: [89, 90] }), [89, 90], None, Some(63)),
        node(Kind::Scalar(ScalarObservation::Integer { sign: "None", digits: [91, 92] }), [91, 92], None, None),
        node(Kind::Assignment, [93, 96], Some(65), Some(67)),
        node(Kind::Name(NameObservation { bytes: b"A".to_vec(), quoted: false, span: [93, 94] }), [93, 94], None, Some(66)),
        node(Kind::Scalar(ScalarObservation::Integer { sign: "None", digits: [95, 96] }), [95, 96], None, None),
        node(Kind::Assignment, [97, 100], Some(68), Some(70)),
        node(Kind::Name(NameObservation { bytes: b"A".to_vec(), quoted: false, span: [97, 98] }), [97, 98], None, Some(69)),
        node(Kind::Scalar(ScalarObservation::Integer { sign: "None", digits: [99, 100] }), [99, 100], None, None),
        node(Kind::Assignment, [101, 104], Some(71), Some(73)),
        node(Kind::Name(NameObservation { bytes: b"A".to_vec(), quoted: false, span: [101, 102] }), [101, 102], None, Some(72)),
        node(Kind::Scalar(ScalarObservation::Integer { sign: "None", digits: [103, 104] }), [103, 104], None, None),
        node(Kind::Assignment, [105, 108], Some(74), Some(76)),
        node(Kind::Name(NameObservation { bytes: b"A".to_vec(), quoted: false, span: [105, 106] }), [105, 106], None, Some(75)),
        node(Kind::Scalar(ScalarObservation::Integer { sign: "None", digits: [107, 108] }), [107, 108], None, None),
        node(Kind::Assignment, [109, 112], Some(77), Some(79)),
        node(Kind::Name(NameObservation { bytes: b"A".to_vec(), quoted: false, span: [109, 110] }), [109, 110], None, Some(78)),
        node(Kind::Scalar(ScalarObservation::Integer { sign: "None", digits: [111, 112] }), [111, 112], None, None),
        node(Kind::Assignment, [113, 116], Some(80), Some(82)),
        node(Kind::Name(NameObservation { bytes: b"A".to_vec(), quoted: false, span: [113, 114] }), [113, 114], None, Some(81)),
        node(Kind::Scalar(ScalarObservation::Integer { sign: "None", digits: [115, 116] }), [115, 116], None, None),
        node(Kind::Assignment, [117, 120], Some(83), Some(85)),
        node(Kind::Name(NameObservation { bytes: b"A".to_vec(), quoted: false, span: [117, 118] }), [117, 118], None, Some(84)),
        node(Kind::Scalar(ScalarObservation::Integer { sign: "None", digits: [119, 120] }), [119, 120], None, None),
        node(Kind::Assignment, [121, 124], Some(86), Some(88)),
        node(Kind::Name(NameObservation { bytes: b"A".to_vec(), quoted: false, span: [121, 122] }), [121, 122], None, Some(87)),
        node(Kind::Scalar(ScalarObservation::Integer { sign: "None", digits: [123, 124] }), [123, 124], None, None),
        node(Kind::Assignment, [125, 128], Some(89), Some(91)),
        node(Kind::Name(NameObservation { bytes: b"A".to_vec(), quoted: false, span: [125, 126] }), [125, 126], None, Some(90)),
        node(Kind::Scalar(ScalarObservation::Integer { sign: "None", digits: [127, 128] }), [127, 128], None, None),
        node(Kind::Assignment, [129, 132], Some(92), Some(94)),
        node(Kind::Name(NameObservation { bytes: b"A".to_vec(), quoted: false, span: [129, 130] }), [129, 130], None, Some(93)),
        node(Kind::Scalar(ScalarObservation::Integer { sign: "None", digits: [131, 132] }), [131, 132], None, None),
        node(Kind::Assignment, [133, 136], Some(95), Some(97)),
        node(Kind::Name(NameObservation { bytes: b"A".to_vec(), quoted: false, span: [133, 134] }), [133, 134], None, Some(96)),
        node(Kind::Scalar(ScalarObservation::Integer { sign: "None", digits: [135, 136] }), [135, 136], None, None),
        node(Kind::Assignment, [137, 140], Some(98), Some(100)),
        node(Kind::Name(NameObservation { bytes: b"A".to_vec(), quoted: false, span: [137, 138] }), [137, 138], None, Some(99)),
        node(Kind::Scalar(ScalarObservation::Integer { sign: "None", digits: [139, 140] }), [139, 140], None, None),
        node(Kind::Assignment, [141, 144], Some(101), None),
        node(Kind::Name(NameObservation { bytes: b"A".to_vec(), quoted: false, span: [141, 142] }), [141, 142], None, Some(102)),
        node(Kind::Scalar(ScalarObservation::Integer { sign: "None", digits: [143, 144] }), [143, 144], None, None),
    ], 0)
}

fn case_0111() -> Case {
    good(
        "raw/where/mixed-type",
        "raw",
        b"SELECT * FROM t WHERE 'x'=1",
        vec![
            node(Kind::Select, [0, 27], Some(1), None),
            node(Kind::Projection, [7, 8], Some(2), Some(3)),
            node(Kind::Wildcard, [7, 8], None, None),
            node(Kind::Target, [14, 15], Some(4), Some(5)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [14, 15],
                }),
                [14, 15],
                None,
                None,
            ),
            node(Kind::Where, [16, 27], Some(6), None),
            node(Kind::Or, [22, 27], Some(7), None),
            node(Kind::And, [22, 27], Some(8), None),
            node(Kind::Compare(Comparison::Eq), [22, 27], Some(9), None),
            node(
                Kind::Scalar(ScalarObservation::Text),
                [22, 25],
                None,
                Some(10),
            ),
            node(
                Kind::Scalar(ScalarObservation::Integer {
                    sign: "None",
                    digits: [26, 27],
                }),
                [26, 27],
                None,
                None,
            ),
        ],
        0,
    )
}

fn case_0112() -> Case {
    good(
        "raw/where/overflow",
        "raw",
        b"SELECT * FROM t WHERE 9223372036854775808=0",
        vec![
            node(Kind::Select, [0, 43], Some(1), None),
            node(Kind::Projection, [7, 8], Some(2), Some(3)),
            node(Kind::Wildcard, [7, 8], None, None),
            node(Kind::Target, [14, 15], Some(4), Some(5)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [14, 15],
                }),
                [14, 15],
                None,
                None,
            ),
            node(Kind::Where, [16, 43], Some(6), None),
            node(Kind::Or, [22, 43], Some(7), None),
            node(Kind::And, [22, 43], Some(8), None),
            node(Kind::Compare(Comparison::Eq), [22, 43], Some(9), None),
            node(
                Kind::Scalar(ScalarObservation::Integer {
                    sign: "None",
                    digits: [22, 41],
                }),
                [22, 41],
                None,
                Some(10),
            ),
            node(
                Kind::Scalar(ScalarObservation::Integer {
                    sign: "None",
                    digits: [42, 43],
                }),
                [42, 43],
                None,
                None,
            ),
        ],
        0,
    )
}

fn case_0113() -> Case {
    good(
        "raw/where/bad-hex",
        "raw",
        b"SELECT * FROM t WHERE X'z'=X'0'",
        vec![
            node(Kind::Select, [0, 31], Some(1), None),
            node(Kind::Projection, [7, 8], Some(2), Some(3)),
            node(Kind::Wildcard, [7, 8], None, None),
            node(Kind::Target, [14, 15], Some(4), Some(5)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [14, 15],
                }),
                [14, 15],
                None,
                None,
            ),
            node(Kind::Where, [16, 31], Some(6), None),
            node(Kind::Or, [22, 31], Some(7), None),
            node(Kind::And, [22, 31], Some(8), None),
            node(Kind::Compare(Comparison::Eq), [22, 31], Some(9), None),
            node(
                Kind::Scalar(ScalarObservation::Binary),
                [22, 26],
                None,
                Some(10),
            ),
            node(
                Kind::Scalar(ScalarObservation::Binary),
                [27, 31],
                None,
                None,
            ),
        ],
        0,
    )
}

fn case_0114() -> Case {
    good(
        "raw/where/empty",
        "raw",
        b"SELECT * FROM t WHERE ''=NULL",
        vec![
            node(Kind::Select, [0, 29], Some(1), None),
            node(Kind::Projection, [7, 8], Some(2), Some(3)),
            node(Kind::Wildcard, [7, 8], None, None),
            node(Kind::Target, [14, 15], Some(4), Some(5)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [14, 15],
                }),
                [14, 15],
                None,
                None,
            ),
            node(Kind::Where, [16, 29], Some(6), None),
            node(Kind::Or, [22, 29], Some(7), None),
            node(Kind::And, [22, 29], Some(8), None),
            node(Kind::Compare(Comparison::Eq), [22, 29], Some(9), None),
            node(
                Kind::Scalar(ScalarObservation::Text),
                [22, 24],
                None,
                Some(10),
            ),
            node(Kind::Scalar(ScalarObservation::Null), [25, 29], None, None),
        ],
        0,
    )
}

fn case_0115() -> Case {
    good(
        "context/name/distinct",
        "contextual",
        b"SELECT distinct FROM t",
        vec![
            node(Kind::Select, [0, 22], Some(1), None),
            node(Kind::Projection, [7, 15], Some(2), Some(4)),
            node(Kind::ProjectionItem, [7, 15], Some(3), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"DISTINCT".to_vec(),
                    quoted: false,
                    span: [7, 15],
                }),
                [7, 15],
                None,
                None,
            ),
            node(Kind::Target, [21, 22], Some(5), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [21, 22],
                }),
                [21, 22],
                None,
                None,
            ),
        ],
        0,
    )
}

fn case_0116() -> Case {
    good(
        "context/alias/distinct",
        "contextual",
        b"SELECT distinct AS a FROM t",
        vec![
            node(Kind::Select, [0, 27], Some(1), None),
            node(Kind::Projection, [7, 20], Some(2), Some(5)),
            node(Kind::ProjectionItem, [7, 20], Some(3), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"DISTINCT".to_vec(),
                    quoted: false,
                    span: [7, 15],
                }),
                [7, 15],
                None,
                Some(4),
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [19, 20],
                }),
                [19, 20],
                None,
                None,
            ),
            node(Kind::Target, [26, 27], Some(6), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [26, 27],
                }),
                [26, 27],
                None,
                None,
            ),
        ],
        0,
    )
}

fn case_0117() -> Case {
    good(
        "context/list/distinct",
        "contextual",
        b"SELECT distinct,a FROM t",
        vec![
            node(Kind::Select, [0, 24], Some(1), None),
            node(Kind::Projection, [7, 17], Some(2), Some(6)),
            node(Kind::ProjectionItem, [7, 15], Some(3), Some(4)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"DISTINCT".to_vec(),
                    quoted: false,
                    span: [7, 15],
                }),
                [7, 15],
                None,
                None,
            ),
            node(Kind::ProjectionItem, [16, 17], Some(5), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [16, 17],
                }),
                [16, 17],
                None,
                None,
            ),
            node(Kind::Target, [23, 24], Some(7), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [23, 24],
                }),
                [23, 24],
                None,
                None,
            ),
        ],
        0,
    )
}

fn case_0118() -> Case {
    bad(
        "context/modifier/distinct/61",
        "contextual",
        b"SELECT distinct a FROM t",
        "Unsupported",
        "Parse",
        [7, 15],
    )
}

fn case_0119() -> Case {
    bad(
        "context/modifier/distinct/2a",
        "contextual",
        b"SELECT distinct * FROM t",
        "Unsupported",
        "Parse",
        [7, 15],
    )
}

fn case_0120() -> Case {
    bad(
        "context/modifier/distinct/31",
        "contextual",
        b"SELECT distinct 1 FROM t",
        "Unsupported",
        "Parse",
        [7, 15],
    )
}

fn case_0121() -> Case {
    bad(
        "context/modifier/distinct/277827",
        "contextual",
        b"SELECT distinct 'x' FROM t",
        "Unsupported",
        "Parse",
        [7, 15],
    )
}

fn case_0122() -> Case {
    bad(
        "context/modifier/distinct/5827303027",
        "contextual",
        b"SELECT distinct X'00' FROM t",
        "Unsupported",
        "Parse",
        [7, 15],
    )
}

fn case_0123() -> Case {
    bad(
        "context/modifier/distinct/54525545",
        "contextual",
        b"SELECT distinct TRUE FROM t",
        "Unsupported",
        "Parse",
        [7, 15],
    )
}

fn case_0124() -> Case {
    bad(
        "context/modifier/distinct/46414c5345",
        "contextual",
        b"SELECT distinct FALSE FROM t",
        "Unsupported",
        "Parse",
        [7, 15],
    )
}

fn case_0125() -> Case {
    bad(
        "context/modifier/distinct/4e554c4c",
        "contextual",
        b"SELECT distinct NULL FROM t",
        "Unsupported",
        "Parse",
        [7, 15],
    )
}

fn case_0126() -> Case {
    bad(
        "context/modifier/distinct/3f",
        "contextual",
        b"SELECT distinct ? FROM t",
        "Unsupported",
        "Parse",
        [7, 15],
    )
}

fn case_0127() -> Case {
    bad(
        "context/modifier/distinct/2b31",
        "contextual",
        b"SELECT distinct +1 FROM t",
        "Unsupported",
        "Parse",
        [7, 15],
    )
}

fn case_0128() -> Case {
    bad(
        "context/modifier/distinct/2d31",
        "contextual",
        b"SELECT distinct -1 FROM t",
        "Unsupported",
        "Parse",
        [7, 15],
    )
}

fn case_0129() -> Case {
    bad(
        "context/modifier/distinct/286129",
        "contextual",
        b"SELECT distinct (a) FROM t",
        "Unsupported",
        "Parse",
        [7, 15],
    )
}

fn case_0130() -> Case {
    good(
        "context/name/all",
        "contextual",
        b"SELECT all FROM t",
        vec![
            node(Kind::Select, [0, 17], Some(1), None),
            node(Kind::Projection, [7, 10], Some(2), Some(4)),
            node(Kind::ProjectionItem, [7, 10], Some(3), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"ALL".to_vec(),
                    quoted: false,
                    span: [7, 10],
                }),
                [7, 10],
                None,
                None,
            ),
            node(Kind::Target, [16, 17], Some(5), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [16, 17],
                }),
                [16, 17],
                None,
                None,
            ),
        ],
        0,
    )
}

fn case_0131() -> Case {
    good(
        "context/alias/all",
        "contextual",
        b"SELECT all AS a FROM t",
        vec![
            node(Kind::Select, [0, 22], Some(1), None),
            node(Kind::Projection, [7, 15], Some(2), Some(5)),
            node(Kind::ProjectionItem, [7, 15], Some(3), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"ALL".to_vec(),
                    quoted: false,
                    span: [7, 10],
                }),
                [7, 10],
                None,
                Some(4),
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [14, 15],
                }),
                [14, 15],
                None,
                None,
            ),
            node(Kind::Target, [21, 22], Some(6), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [21, 22],
                }),
                [21, 22],
                None,
                None,
            ),
        ],
        0,
    )
}

fn case_0132() -> Case {
    good(
        "context/list/all",
        "contextual",
        b"SELECT all,a FROM t",
        vec![
            node(Kind::Select, [0, 19], Some(1), None),
            node(Kind::Projection, [7, 12], Some(2), Some(6)),
            node(Kind::ProjectionItem, [7, 10], Some(3), Some(4)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"ALL".to_vec(),
                    quoted: false,
                    span: [7, 10],
                }),
                [7, 10],
                None,
                None,
            ),
            node(Kind::ProjectionItem, [11, 12], Some(5), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [11, 12],
                }),
                [11, 12],
                None,
                None,
            ),
            node(Kind::Target, [18, 19], Some(7), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [18, 19],
                }),
                [18, 19],
                None,
                None,
            ),
        ],
        0,
    )
}

fn case_0133() -> Case {
    bad(
        "context/modifier/all/61",
        "contextual",
        b"SELECT all a FROM t",
        "Unsupported",
        "Parse",
        [7, 10],
    )
}

fn case_0134() -> Case {
    bad(
        "context/modifier/all/2a",
        "contextual",
        b"SELECT all * FROM t",
        "Unsupported",
        "Parse",
        [7, 10],
    )
}

fn case_0135() -> Case {
    bad(
        "context/modifier/all/31",
        "contextual",
        b"SELECT all 1 FROM t",
        "Unsupported",
        "Parse",
        [7, 10],
    )
}

fn case_0136() -> Case {
    bad(
        "context/modifier/all/277827",
        "contextual",
        b"SELECT all 'x' FROM t",
        "Unsupported",
        "Parse",
        [7, 10],
    )
}

fn case_0137() -> Case {
    bad(
        "context/modifier/all/5827303027",
        "contextual",
        b"SELECT all X'00' FROM t",
        "Unsupported",
        "Parse",
        [7, 10],
    )
}

fn case_0138() -> Case {
    bad(
        "context/modifier/all/54525545",
        "contextual",
        b"SELECT all TRUE FROM t",
        "Unsupported",
        "Parse",
        [7, 10],
    )
}

fn case_0139() -> Case {
    bad(
        "context/modifier/all/46414c5345",
        "contextual",
        b"SELECT all FALSE FROM t",
        "Unsupported",
        "Parse",
        [7, 10],
    )
}

fn case_0140() -> Case {
    bad(
        "context/modifier/all/4e554c4c",
        "contextual",
        b"SELECT all NULL FROM t",
        "Unsupported",
        "Parse",
        [7, 10],
    )
}

fn case_0141() -> Case {
    bad(
        "context/modifier/all/3f",
        "contextual",
        b"SELECT all ? FROM t",
        "Unsupported",
        "Parse",
        [7, 10],
    )
}

fn case_0142() -> Case {
    bad(
        "context/modifier/all/2b31",
        "contextual",
        b"SELECT all +1 FROM t",
        "Unsupported",
        "Parse",
        [7, 10],
    )
}

fn case_0143() -> Case {
    bad(
        "context/modifier/all/2d31",
        "contextual",
        b"SELECT all -1 FROM t",
        "Unsupported",
        "Parse",
        [7, 10],
    )
}

fn case_0144() -> Case {
    bad(
        "context/modifier/all/286129",
        "contextual",
        b"SELECT all (a) FROM t",
        "Unsupported",
        "Parse",
        [7, 10],
    )
}

fn case_0145() -> Case {
    good(
        "context/ordinary/join",
        "contextual",
        b"SELECT join AS join FROM join WHERE join=0 ORDER BY join",
        vec![
            node(Kind::Select, [0, 56], Some(1), None),
            node(Kind::Projection, [7, 19], Some(2), Some(5)),
            node(Kind::ProjectionItem, [7, 19], Some(3), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"JOIN".to_vec(),
                    quoted: false,
                    span: [7, 11],
                }),
                [7, 11],
                None,
                Some(4),
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"JOIN".to_vec(),
                    quoted: false,
                    span: [15, 19],
                }),
                [15, 19],
                None,
                None,
            ),
            node(Kind::Target, [25, 29], Some(6), Some(7)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"JOIN".to_vec(),
                    quoted: false,
                    span: [25, 29],
                }),
                [25, 29],
                None,
                None,
            ),
            node(Kind::Where, [30, 42], Some(8), Some(13)),
            node(Kind::Or, [36, 42], Some(9), None),
            node(Kind::And, [36, 42], Some(10), None),
            node(Kind::Compare(Comparison::Eq), [36, 42], Some(11), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"JOIN".to_vec(),
                    quoted: false,
                    span: [36, 40],
                }),
                [36, 40],
                None,
                Some(12),
            ),
            node(
                Kind::Scalar(ScalarObservation::Integer {
                    sign: "None",
                    digits: [41, 42],
                }),
                [41, 42],
                None,
                None,
            ),
            node(Kind::OrderBy, [43, 56], Some(14), None),
            node(
                Kind::OrderTerm {
                    direction: None,
                    nulls: None,
                },
                [52, 56],
                Some(15),
                None,
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"JOIN".to_vec(),
                    quoted: false,
                    span: [52, 56],
                }),
                [52, 56],
                None,
                None,
            ),
        ],
        0,
    )
}

fn case_0146() -> Case {
    good(
        "context/ordinary/group",
        "contextual",
        b"SELECT group AS group FROM group WHERE group=0 ORDER BY group",
        vec![
            node(Kind::Select, [0, 61], Some(1), None),
            node(Kind::Projection, [7, 21], Some(2), Some(5)),
            node(Kind::ProjectionItem, [7, 21], Some(3), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"GROUP".to_vec(),
                    quoted: false,
                    span: [7, 12],
                }),
                [7, 12],
                None,
                Some(4),
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"GROUP".to_vec(),
                    quoted: false,
                    span: [16, 21],
                }),
                [16, 21],
                None,
                None,
            ),
            node(Kind::Target, [27, 32], Some(6), Some(7)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"GROUP".to_vec(),
                    quoted: false,
                    span: [27, 32],
                }),
                [27, 32],
                None,
                None,
            ),
            node(Kind::Where, [33, 46], Some(8), Some(13)),
            node(Kind::Or, [39, 46], Some(9), None),
            node(Kind::And, [39, 46], Some(10), None),
            node(Kind::Compare(Comparison::Eq), [39, 46], Some(11), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"GROUP".to_vec(),
                    quoted: false,
                    span: [39, 44],
                }),
                [39, 44],
                None,
                Some(12),
            ),
            node(
                Kind::Scalar(ScalarObservation::Integer {
                    sign: "None",
                    digits: [45, 46],
                }),
                [45, 46],
                None,
                None,
            ),
            node(Kind::OrderBy, [47, 61], Some(14), None),
            node(
                Kind::OrderTerm {
                    direction: None,
                    nulls: None,
                },
                [56, 61],
                Some(15),
                None,
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"GROUP".to_vec(),
                    quoted: false,
                    span: [56, 61],
                }),
                [56, 61],
                None,
                None,
            ),
        ],
        0,
    )
}

fn case_0147() -> Case {
    good(
        "context/ordinary/returning",
        "contextual",
        b"SELECT returning AS returning FROM returning WHERE returning=0 ORDER BY returning",
        vec![
            node(Kind::Select, [0, 81], Some(1), None),
            node(Kind::Projection, [7, 29], Some(2), Some(5)),
            node(Kind::ProjectionItem, [7, 29], Some(3), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"RETURNING".to_vec(),
                    quoted: false,
                    span: [7, 16],
                }),
                [7, 16],
                None,
                Some(4),
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"RETURNING".to_vec(),
                    quoted: false,
                    span: [20, 29],
                }),
                [20, 29],
                None,
                None,
            ),
            node(Kind::Target, [35, 44], Some(6), Some(7)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"RETURNING".to_vec(),
                    quoted: false,
                    span: [35, 44],
                }),
                [35, 44],
                None,
                None,
            ),
            node(Kind::Where, [45, 62], Some(8), Some(13)),
            node(Kind::Or, [51, 62], Some(9), None),
            node(Kind::And, [51, 62], Some(10), None),
            node(Kind::Compare(Comparison::Eq), [51, 62], Some(11), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"RETURNING".to_vec(),
                    quoted: false,
                    span: [51, 60],
                }),
                [51, 60],
                None,
                Some(12),
            ),
            node(
                Kind::Scalar(ScalarObservation::Integer {
                    sign: "None",
                    digits: [61, 62],
                }),
                [61, 62],
                None,
                None,
            ),
            node(Kind::OrderBy, [63, 81], Some(14), None),
            node(
                Kind::OrderTerm {
                    direction: None,
                    nulls: None,
                },
                [72, 81],
                Some(15),
                None,
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"RETURNING".to_vec(),
                    quoted: false,
                    span: [72, 81],
                }),
                [72, 81],
                None,
                None,
            ),
        ],
        0,
    )
}

fn case_0148() -> Case {
    good(
        "context/ordinary/default",
        "contextual",
        b"SELECT default AS default FROM default WHERE default=0 ORDER BY default",
        vec![
            node(Kind::Select, [0, 71], Some(1), None),
            node(Kind::Projection, [7, 25], Some(2), Some(5)),
            node(Kind::ProjectionItem, [7, 25], Some(3), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"DEFAULT".to_vec(),
                    quoted: false,
                    span: [7, 14],
                }),
                [7, 14],
                None,
                Some(4),
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"DEFAULT".to_vec(),
                    quoted: false,
                    span: [18, 25],
                }),
                [18, 25],
                None,
                None,
            ),
            node(Kind::Target, [31, 38], Some(6), Some(7)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"DEFAULT".to_vec(),
                    quoted: false,
                    span: [31, 38],
                }),
                [31, 38],
                None,
                None,
            ),
            node(Kind::Where, [39, 54], Some(8), Some(13)),
            node(Kind::Or, [45, 54], Some(9), None),
            node(Kind::And, [45, 54], Some(10), None),
            node(Kind::Compare(Comparison::Eq), [45, 54], Some(11), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"DEFAULT".to_vec(),
                    quoted: false,
                    span: [45, 52],
                }),
                [45, 52],
                None,
                Some(12),
            ),
            node(
                Kind::Scalar(ScalarObservation::Integer {
                    sign: "None",
                    digits: [53, 54],
                }),
                [53, 54],
                None,
                None,
            ),
            node(Kind::OrderBy, [55, 71], Some(14), None),
            node(
                Kind::OrderTerm {
                    direction: None,
                    nulls: None,
                },
                [64, 71],
                Some(15),
                None,
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"DEFAULT".to_vec(),
                    quoted: false,
                    span: [64, 71],
                }),
                [64, 71],
                None,
                None,
            ),
        ],
        0,
    )
}

fn case_0149() -> Case {
    good(
        "context/ordinary/in",
        "contextual",
        b"SELECT in AS in FROM in WHERE in=0 ORDER BY in",
        vec![
            node(Kind::Select, [0, 46], Some(1), None),
            node(Kind::Projection, [7, 15], Some(2), Some(5)),
            node(Kind::ProjectionItem, [7, 15], Some(3), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"IN".to_vec(),
                    quoted: false,
                    span: [7, 9],
                }),
                [7, 9],
                None,
                Some(4),
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"IN".to_vec(),
                    quoted: false,
                    span: [13, 15],
                }),
                [13, 15],
                None,
                None,
            ),
            node(Kind::Target, [21, 23], Some(6), Some(7)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"IN".to_vec(),
                    quoted: false,
                    span: [21, 23],
                }),
                [21, 23],
                None,
                None,
            ),
            node(Kind::Where, [24, 34], Some(8), Some(13)),
            node(Kind::Or, [30, 34], Some(9), None),
            node(Kind::And, [30, 34], Some(10), None),
            node(Kind::Compare(Comparison::Eq), [30, 34], Some(11), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"IN".to_vec(),
                    quoted: false,
                    span: [30, 32],
                }),
                [30, 32],
                None,
                Some(12),
            ),
            node(
                Kind::Scalar(ScalarObservation::Integer {
                    sign: "None",
                    digits: [33, 34],
                }),
                [33, 34],
                None,
                None,
            ),
            node(Kind::OrderBy, [35, 46], Some(14), None),
            node(
                Kind::OrderTerm {
                    direction: None,
                    nulls: None,
                },
                [44, 46],
                Some(15),
                None,
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"IN".to_vec(),
                    quoted: false,
                    span: [44, 46],
                }),
                [44, 46],
                None,
                None,
            ),
        ],
        0,
    )
}

fn case_0150() -> Case {
    good(
        "context/ordinary/like",
        "contextual",
        b"SELECT like AS like FROM like WHERE like=0 ORDER BY like",
        vec![
            node(Kind::Select, [0, 56], Some(1), None),
            node(Kind::Projection, [7, 19], Some(2), Some(5)),
            node(Kind::ProjectionItem, [7, 19], Some(3), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"LIKE".to_vec(),
                    quoted: false,
                    span: [7, 11],
                }),
                [7, 11],
                None,
                Some(4),
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"LIKE".to_vec(),
                    quoted: false,
                    span: [15, 19],
                }),
                [15, 19],
                None,
                None,
            ),
            node(Kind::Target, [25, 29], Some(6), Some(7)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"LIKE".to_vec(),
                    quoted: false,
                    span: [25, 29],
                }),
                [25, 29],
                None,
                None,
            ),
            node(Kind::Where, [30, 42], Some(8), Some(13)),
            node(Kind::Or, [36, 42], Some(9), None),
            node(Kind::And, [36, 42], Some(10), None),
            node(Kind::Compare(Comparison::Eq), [36, 42], Some(11), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"LIKE".to_vec(),
                    quoted: false,
                    span: [36, 40],
                }),
                [36, 40],
                None,
                Some(12),
            ),
            node(
                Kind::Scalar(ScalarObservation::Integer {
                    sign: "None",
                    digits: [41, 42],
                }),
                [41, 42],
                None,
                None,
            ),
            node(Kind::OrderBy, [43, 56], Some(14), None),
            node(
                Kind::OrderTerm {
                    direction: None,
                    nulls: None,
                },
                [52, 56],
                Some(15),
                None,
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"LIKE".to_vec(),
                    quoted: false,
                    span: [52, 56],
                }),
                [52, 56],
                None,
                None,
            ),
        ],
        0,
    )
}

fn case_0151() -> Case {
    good(
        "context/ordinary/exists",
        "contextual",
        b"SELECT exists AS exists FROM exists WHERE exists=0 ORDER BY exists",
        vec![
            node(Kind::Select, [0, 66], Some(1), None),
            node(Kind::Projection, [7, 23], Some(2), Some(5)),
            node(Kind::ProjectionItem, [7, 23], Some(3), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"EXISTS".to_vec(),
                    quoted: false,
                    span: [7, 13],
                }),
                [7, 13],
                None,
                Some(4),
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"EXISTS".to_vec(),
                    quoted: false,
                    span: [17, 23],
                }),
                [17, 23],
                None,
                None,
            ),
            node(Kind::Target, [29, 35], Some(6), Some(7)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"EXISTS".to_vec(),
                    quoted: false,
                    span: [29, 35],
                }),
                [29, 35],
                None,
                None,
            ),
            node(Kind::Where, [36, 50], Some(8), Some(13)),
            node(Kind::Or, [42, 50], Some(9), None),
            node(Kind::And, [42, 50], Some(10), None),
            node(Kind::Compare(Comparison::Eq), [42, 50], Some(11), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"EXISTS".to_vec(),
                    quoted: false,
                    span: [42, 48],
                }),
                [42, 48],
                None,
                Some(12),
            ),
            node(
                Kind::Scalar(ScalarObservation::Integer {
                    sign: "None",
                    digits: [49, 50],
                }),
                [49, 50],
                None,
                None,
            ),
            node(Kind::OrderBy, [51, 66], Some(14), None),
            node(
                Kind::OrderTerm {
                    direction: None,
                    nulls: None,
                },
                [60, 66],
                Some(15),
                None,
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"EXISTS".to_vec(),
                    quoted: false,
                    span: [60, 66],
                }),
                [60, 66],
                None,
                None,
            ),
        ],
        0,
    )
}

fn case_0152() -> Case {
    good(
        "context/create-table-if",
        "contextual",
        b"CREATE TABLE if(a BIGINT) STORAGE DISK",
        vec![
            node(Kind::CreateTable, [0, 38], Some(1), None),
            node(Kind::Target, [13, 15], Some(2), Some(3)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"IF".to_vec(),
                    quoted: false,
                    span: [13, 15],
                }),
                [13, 15],
                None,
                None,
            ),
            node(Kind::TableDefinition, [15, 25], Some(4), Some(7)),
            node(
                Kind::ColumnDefinition {
                    nullability: Nullability::Implicit,
                },
                [16, 24],
                Some(5),
                None,
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [16, 17],
                }),
                [16, 17],
                None,
                Some(6),
            ),
            node(Kind::DataType(DataType::Int64), [18, 24], None, None),
            node(Kind::StorageDisk, [26, 38], None, None),
        ],
        0,
    )
}

fn case_0153() -> Case {
    good(
        "context/create-index-if",
        "contextual",
        b"CREATE INDEX if ON t(a)",
        vec![
            node(Kind::CreateIndex { unique: false }, [0, 23], Some(1), None),
            node(Kind::Target, [13, 15], Some(2), Some(3)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"IF".to_vec(),
                    quoted: false,
                    span: [13, 15],
                }),
                [13, 15],
                None,
                None,
            ),
            node(Kind::Target, [19, 20], Some(4), Some(5)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [19, 20],
                }),
                [19, 20],
                None,
                None,
            ),
            node(Kind::Columns, [20, 23], Some(6), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [21, 22],
                }),
                [21, 22],
                None,
                None,
            ),
        ],
        0,
    )
}

fn case_0154() -> Case {
    good(
        "context/drop-if/False",
        "contextual",
        b"DROP TABLE if",
        vec![
            node(Kind::DropTable, [0, 13], Some(1), None),
            node(Kind::Target, [11, 13], Some(2), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"IF".to_vec(),
                    quoted: false,
                    span: [11, 13],
                }),
                [11, 13],
                None,
                None,
            ),
        ],
        0,
    )
}

fn case_0155() -> Case {
    good(
        "context/drop-if/True",
        "contextual",
        b"DROP INDEX if",
        vec![
            node(Kind::DropIndex, [0, 13], Some(1), None),
            node(Kind::Target, [11, 13], Some(2), None),
            node(
                Kind::Name(NameObservation {
                    bytes: b"IF".to_vec(),
                    quoted: false,
                    span: [11, 13],
                }),
                [11, 13],
                None,
                None,
            ),
        ],
        0,
    )
}

fn case_0156() -> Case {
    bad(
        "context/if-not-exists/435245415445205441424c4520",
        "contextual",
        b"CREATE TABLE if NOT exists t",
        "Unsupported",
        "Parse",
        [13, 15],
    )
}

fn case_0157() -> Case {
    bad(
        "context/if-not-wrong/435245415445205441424c4520",
        "contextual",
        b"CREATE TABLE if NOT nope",
        "Syntax",
        "Parse",
        [16, 19],
    )
}

fn case_0158() -> Case {
    bad(
        "context/if-not-lex/435245415445205441424c4520",
        "contextual",
        b"CREATE TABLE if NOT !",
        "Syntax",
        "Lex",
        [20, 21],
    )
}

fn case_0159() -> Case {
    bad(
        "context/if-not-exists/43524541544520494e44455820",
        "contextual",
        b"CREATE INDEX if NOT exists t",
        "Unsupported",
        "Parse",
        [13, 15],
    )
}

fn case_0160() -> Case {
    bad(
        "context/if-not-wrong/43524541544520494e44455820",
        "contextual",
        b"CREATE INDEX if NOT nope",
        "Syntax",
        "Parse",
        [16, 19],
    )
}

fn case_0161() -> Case {
    bad(
        "context/if-not-lex/43524541544520494e44455820",
        "contextual",
        b"CREATE INDEX if NOT !",
        "Syntax",
        "Lex",
        [20, 21],
    )
}

fn case_0162() -> Case {
    bad(
        "context/if-not-exists/43524541544520554e4951554520494e44455820",
        "contextual",
        b"CREATE UNIQUE INDEX if NOT exists t",
        "Unsupported",
        "Parse",
        [20, 22],
    )
}

fn case_0163() -> Case {
    bad(
        "context/if-not-wrong/43524541544520554e4951554520494e44455820",
        "contextual",
        b"CREATE UNIQUE INDEX if NOT nope",
        "Syntax",
        "Parse",
        [23, 26],
    )
}

fn case_0164() -> Case {
    bad(
        "context/if-not-lex/43524541544520554e4951554520494e44455820",
        "contextual",
        b"CREATE UNIQUE INDEX if NOT !",
        "Syntax",
        "Lex",
        [27, 28],
    )
}

fn case_0165() -> Case {
    bad(
        "context/if-exists/44524f50205441424c4520",
        "contextual",
        b"DROP TABLE if exists t",
        "Unsupported",
        "Parse",
        [11, 13],
    )
}

fn case_0166() -> Case {
    bad(
        "context/if-exists/44524f5020494e44455820",
        "contextual",
        b"DROP INDEX if exists t",
        "Unsupported",
        "Parse",
        [11, 13],
    )
}

fn case_0167() -> Case {
    good(
        "context/column/check/Int64/False",
        "contextual",
        b"CREATE TABLE t(a BIGINT,check BIGINT) STORAGE DISK",
        vec![
            node(Kind::CreateTable, [0, 50], Some(1), None),
            node(Kind::Target, [13, 14], Some(2), Some(3)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [13, 14],
                }),
                [13, 14],
                None,
                None,
            ),
            node(Kind::TableDefinition, [14, 37], Some(4), Some(10)),
            node(
                Kind::ColumnDefinition {
                    nullability: Nullability::Implicit,
                },
                [15, 23],
                Some(5),
                Some(7),
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [15, 16],
                }),
                [15, 16],
                None,
                Some(6),
            ),
            node(Kind::DataType(DataType::Int64), [17, 23], None, None),
            node(
                Kind::ColumnDefinition {
                    nullability: Nullability::Implicit,
                },
                [24, 36],
                Some(8),
                None,
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"CHECK".to_vec(),
                    quoted: false,
                    span: [24, 29],
                }),
                [24, 29],
                None,
                Some(9),
            ),
            node(Kind::DataType(DataType::Int64), [30, 36], None, None),
            node(Kind::StorageDisk, [38, 50], None, None),
        ],
        0,
    )
}

fn case_0168() -> Case {
    good(
        "context/column/check/Int64/True",
        "contextual",
        b"CREATE TABLE t(check BIGINT) STORAGE DISK",
        vec![
            node(Kind::CreateTable, [0, 41], Some(1), None),
            node(Kind::Target, [13, 14], Some(2), Some(3)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [13, 14],
                }),
                [13, 14],
                None,
                None,
            ),
            node(Kind::TableDefinition, [14, 28], Some(4), Some(7)),
            node(
                Kind::ColumnDefinition {
                    nullability: Nullability::Implicit,
                },
                [15, 27],
                Some(5),
                None,
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"CHECK".to_vec(),
                    quoted: false,
                    span: [15, 20],
                }),
                [15, 20],
                None,
                Some(6),
            ),
            node(Kind::DataType(DataType::Int64), [21, 27], None, None),
            node(Kind::StorageDisk, [29, 41], None, None),
        ],
        0,
    )
}

fn case_0169() -> Case {
    good(
        "context/column/check/Bool/False",
        "contextual",
        b"CREATE TABLE t(a BIGINT,check BOOLEAN) STORAGE DISK",
        vec![
            node(Kind::CreateTable, [0, 51], Some(1), None),
            node(Kind::Target, [13, 14], Some(2), Some(3)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [13, 14],
                }),
                [13, 14],
                None,
                None,
            ),
            node(Kind::TableDefinition, [14, 38], Some(4), Some(10)),
            node(
                Kind::ColumnDefinition {
                    nullability: Nullability::Implicit,
                },
                [15, 23],
                Some(5),
                Some(7),
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [15, 16],
                }),
                [15, 16],
                None,
                Some(6),
            ),
            node(Kind::DataType(DataType::Int64), [17, 23], None, None),
            node(
                Kind::ColumnDefinition {
                    nullability: Nullability::Implicit,
                },
                [24, 37],
                Some(8),
                None,
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"CHECK".to_vec(),
                    quoted: false,
                    span: [24, 29],
                }),
                [24, 29],
                None,
                Some(9),
            ),
            node(Kind::DataType(DataType::Bool), [30, 37], None, None),
            node(Kind::StorageDisk, [39, 51], None, None),
        ],
        0,
    )
}

fn case_0170() -> Case {
    good(
        "context/column/check/Bool/True",
        "contextual",
        b"CREATE TABLE t(check BOOLEAN) STORAGE DISK",
        vec![
            node(Kind::CreateTable, [0, 42], Some(1), None),
            node(Kind::Target, [13, 14], Some(2), Some(3)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [13, 14],
                }),
                [13, 14],
                None,
                None,
            ),
            node(Kind::TableDefinition, [14, 29], Some(4), Some(7)),
            node(
                Kind::ColumnDefinition {
                    nullability: Nullability::Implicit,
                },
                [15, 28],
                Some(5),
                None,
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"CHECK".to_vec(),
                    quoted: false,
                    span: [15, 20],
                }),
                [15, 20],
                None,
                Some(6),
            ),
            node(Kind::DataType(DataType::Bool), [21, 28], None, None),
            node(Kind::StorageDisk, [30, 42], None, None),
        ],
        0,
    )
}

fn case_0171() -> Case {
    good(
        "context/column/check/Utf8/False",
        "contextual",
        b"CREATE TABLE t(a BIGINT,check VARCHAR(1)) STORAGE DISK",
        vec![
            node(Kind::CreateTable, [0, 54], Some(1), None),
            node(Kind::Target, [13, 14], Some(2), Some(3)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [13, 14],
                }),
                [13, 14],
                None,
                None,
            ),
            node(Kind::TableDefinition, [14, 41], Some(4), Some(10)),
            node(
                Kind::ColumnDefinition {
                    nullability: Nullability::Implicit,
                },
                [15, 23],
                Some(5),
                Some(7),
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [15, 16],
                }),
                [15, 16],
                None,
                Some(6),
            ),
            node(Kind::DataType(DataType::Int64), [17, 23], None, None),
            node(
                Kind::ColumnDefinition {
                    nullability: Nullability::Implicit,
                },
                [24, 40],
                Some(8),
                None,
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"CHECK".to_vec(),
                    quoted: false,
                    span: [24, 29],
                }),
                [24, 29],
                None,
                Some(9),
            ),
            node(
                Kind::DataType(DataType::Utf8(TypeLength {
                    sign: "None",
                    digits: [38, 39],
                    span: [38, 39],
                })),
                [30, 40],
                None,
                None,
            ),
            node(Kind::StorageDisk, [42, 54], None, None),
        ],
        0,
    )
}

fn case_0172() -> Case {
    good(
        "context/column/check/Utf8/True",
        "contextual",
        b"CREATE TABLE t(check VARCHAR(1)) STORAGE DISK",
        vec![
            node(Kind::CreateTable, [0, 45], Some(1), None),
            node(Kind::Target, [13, 14], Some(2), Some(3)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [13, 14],
                }),
                [13, 14],
                None,
                None,
            ),
            node(Kind::TableDefinition, [14, 32], Some(4), Some(7)),
            node(
                Kind::ColumnDefinition {
                    nullability: Nullability::Implicit,
                },
                [15, 31],
                Some(5),
                None,
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"CHECK".to_vec(),
                    quoted: false,
                    span: [15, 20],
                }),
                [15, 20],
                None,
                Some(6),
            ),
            node(
                Kind::DataType(DataType::Utf8(TypeLength {
                    sign: "None",
                    digits: [29, 30],
                    span: [29, 30],
                })),
                [21, 31],
                None,
                None,
            ),
            node(Kind::StorageDisk, [33, 45], None, None),
        ],
        0,
    )
}

fn case_0173() -> Case {
    good(
        "context/column/check/Binary/False",
        "contextual",
        b"CREATE TABLE t(a BIGINT,check VARBINARY(1)) STORAGE DISK",
        vec![
            node(Kind::CreateTable, [0, 56], Some(1), None),
            node(Kind::Target, [13, 14], Some(2), Some(3)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [13, 14],
                }),
                [13, 14],
                None,
                None,
            ),
            node(Kind::TableDefinition, [14, 43], Some(4), Some(10)),
            node(
                Kind::ColumnDefinition {
                    nullability: Nullability::Implicit,
                },
                [15, 23],
                Some(5),
                Some(7),
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [15, 16],
                }),
                [15, 16],
                None,
                Some(6),
            ),
            node(Kind::DataType(DataType::Int64), [17, 23], None, None),
            node(
                Kind::ColumnDefinition {
                    nullability: Nullability::Implicit,
                },
                [24, 42],
                Some(8),
                None,
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"CHECK".to_vec(),
                    quoted: false,
                    span: [24, 29],
                }),
                [24, 29],
                None,
                Some(9),
            ),
            node(
                Kind::DataType(DataType::Binary(TypeLength {
                    sign: "None",
                    digits: [40, 41],
                    span: [40, 41],
                })),
                [30, 42],
                None,
                None,
            ),
            node(Kind::StorageDisk, [44, 56], None, None),
        ],
        0,
    )
}

fn case_0174() -> Case {
    good(
        "context/column/check/Binary/True",
        "contextual",
        b"CREATE TABLE t(check VARBINARY(1)) STORAGE DISK",
        vec![
            node(Kind::CreateTable, [0, 47], Some(1), None),
            node(Kind::Target, [13, 14], Some(2), Some(3)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [13, 14],
                }),
                [13, 14],
                None,
                None,
            ),
            node(Kind::TableDefinition, [14, 34], Some(4), Some(7)),
            node(
                Kind::ColumnDefinition {
                    nullability: Nullability::Implicit,
                },
                [15, 33],
                Some(5),
                None,
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"CHECK".to_vec(),
                    quoted: false,
                    span: [15, 20],
                }),
                [15, 20],
                None,
                Some(6),
            ),
            node(
                Kind::DataType(DataType::Binary(TypeLength {
                    sign: "None",
                    digits: [31, 32],
                    span: [31, 32],
                })),
                [21, 33],
                None,
                None,
            ),
            node(Kind::StorageDisk, [35, 47], None, None),
        ],
        0,
    )
}

fn case_0175() -> Case {
    good(
        "context/quoted-column/check",
        "contextual",
        b"CREATE TABLE t(\"check\" BIGINT) STORAGE DISK",
        vec![
            node(Kind::CreateTable, [0, 43], Some(1), None),
            node(Kind::Target, [13, 14], Some(2), Some(3)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [13, 14],
                }),
                [13, 14],
                None,
                None,
            ),
            node(Kind::TableDefinition, [14, 30], Some(4), Some(7)),
            node(
                Kind::ColumnDefinition {
                    nullability: Nullability::Implicit,
                },
                [15, 29],
                Some(5),
                None,
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"check".to_vec(),
                    quoted: true,
                    span: [15, 22],
                }),
                [15, 22],
                None,
                Some(6),
            ),
            node(Kind::DataType(DataType::Int64), [23, 29], None, None),
            node(Kind::StorageDisk, [31, 43], None, None),
        ],
        0,
    )
}

fn case_0176() -> Case {
    good(
        "context/column/foreign/Int64/False",
        "contextual",
        b"CREATE TABLE t(a BIGINT,foreign BIGINT) STORAGE DISK",
        vec![
            node(Kind::CreateTable, [0, 52], Some(1), None),
            node(Kind::Target, [13, 14], Some(2), Some(3)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [13, 14],
                }),
                [13, 14],
                None,
                None,
            ),
            node(Kind::TableDefinition, [14, 39], Some(4), Some(10)),
            node(
                Kind::ColumnDefinition {
                    nullability: Nullability::Implicit,
                },
                [15, 23],
                Some(5),
                Some(7),
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [15, 16],
                }),
                [15, 16],
                None,
                Some(6),
            ),
            node(Kind::DataType(DataType::Int64), [17, 23], None, None),
            node(
                Kind::ColumnDefinition {
                    nullability: Nullability::Implicit,
                },
                [24, 38],
                Some(8),
                None,
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"FOREIGN".to_vec(),
                    quoted: false,
                    span: [24, 31],
                }),
                [24, 31],
                None,
                Some(9),
            ),
            node(Kind::DataType(DataType::Int64), [32, 38], None, None),
            node(Kind::StorageDisk, [40, 52], None, None),
        ],
        0,
    )
}

fn case_0177() -> Case {
    good(
        "context/column/foreign/Int64/True",
        "contextual",
        b"CREATE TABLE t(foreign BIGINT) STORAGE DISK",
        vec![
            node(Kind::CreateTable, [0, 43], Some(1), None),
            node(Kind::Target, [13, 14], Some(2), Some(3)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [13, 14],
                }),
                [13, 14],
                None,
                None,
            ),
            node(Kind::TableDefinition, [14, 30], Some(4), Some(7)),
            node(
                Kind::ColumnDefinition {
                    nullability: Nullability::Implicit,
                },
                [15, 29],
                Some(5),
                None,
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"FOREIGN".to_vec(),
                    quoted: false,
                    span: [15, 22],
                }),
                [15, 22],
                None,
                Some(6),
            ),
            node(Kind::DataType(DataType::Int64), [23, 29], None, None),
            node(Kind::StorageDisk, [31, 43], None, None),
        ],
        0,
    )
}

fn case_0178() -> Case {
    good(
        "context/column/foreign/Bool/False",
        "contextual",
        b"CREATE TABLE t(a BIGINT,foreign BOOLEAN) STORAGE DISK",
        vec![
            node(Kind::CreateTable, [0, 53], Some(1), None),
            node(Kind::Target, [13, 14], Some(2), Some(3)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [13, 14],
                }),
                [13, 14],
                None,
                None,
            ),
            node(Kind::TableDefinition, [14, 40], Some(4), Some(10)),
            node(
                Kind::ColumnDefinition {
                    nullability: Nullability::Implicit,
                },
                [15, 23],
                Some(5),
                Some(7),
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [15, 16],
                }),
                [15, 16],
                None,
                Some(6),
            ),
            node(Kind::DataType(DataType::Int64), [17, 23], None, None),
            node(
                Kind::ColumnDefinition {
                    nullability: Nullability::Implicit,
                },
                [24, 39],
                Some(8),
                None,
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"FOREIGN".to_vec(),
                    quoted: false,
                    span: [24, 31],
                }),
                [24, 31],
                None,
                Some(9),
            ),
            node(Kind::DataType(DataType::Bool), [32, 39], None, None),
            node(Kind::StorageDisk, [41, 53], None, None),
        ],
        0,
    )
}

fn case_0179() -> Case {
    good(
        "context/column/foreign/Bool/True",
        "contextual",
        b"CREATE TABLE t(foreign BOOLEAN) STORAGE DISK",
        vec![
            node(Kind::CreateTable, [0, 44], Some(1), None),
            node(Kind::Target, [13, 14], Some(2), Some(3)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [13, 14],
                }),
                [13, 14],
                None,
                None,
            ),
            node(Kind::TableDefinition, [14, 31], Some(4), Some(7)),
            node(
                Kind::ColumnDefinition {
                    nullability: Nullability::Implicit,
                },
                [15, 30],
                Some(5),
                None,
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"FOREIGN".to_vec(),
                    quoted: false,
                    span: [15, 22],
                }),
                [15, 22],
                None,
                Some(6),
            ),
            node(Kind::DataType(DataType::Bool), [23, 30], None, None),
            node(Kind::StorageDisk, [32, 44], None, None),
        ],
        0,
    )
}

fn case_0180() -> Case {
    good(
        "context/column/foreign/Utf8/False",
        "contextual",
        b"CREATE TABLE t(a BIGINT,foreign VARCHAR(1)) STORAGE DISK",
        vec![
            node(Kind::CreateTable, [0, 56], Some(1), None),
            node(Kind::Target, [13, 14], Some(2), Some(3)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [13, 14],
                }),
                [13, 14],
                None,
                None,
            ),
            node(Kind::TableDefinition, [14, 43], Some(4), Some(10)),
            node(
                Kind::ColumnDefinition {
                    nullability: Nullability::Implicit,
                },
                [15, 23],
                Some(5),
                Some(7),
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [15, 16],
                }),
                [15, 16],
                None,
                Some(6),
            ),
            node(Kind::DataType(DataType::Int64), [17, 23], None, None),
            node(
                Kind::ColumnDefinition {
                    nullability: Nullability::Implicit,
                },
                [24, 42],
                Some(8),
                None,
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"FOREIGN".to_vec(),
                    quoted: false,
                    span: [24, 31],
                }),
                [24, 31],
                None,
                Some(9),
            ),
            node(
                Kind::DataType(DataType::Utf8(TypeLength {
                    sign: "None",
                    digits: [40, 41],
                    span: [40, 41],
                })),
                [32, 42],
                None,
                None,
            ),
            node(Kind::StorageDisk, [44, 56], None, None),
        ],
        0,
    )
}

fn case_0181() -> Case {
    good(
        "context/column/foreign/Utf8/True",
        "contextual",
        b"CREATE TABLE t(foreign VARCHAR(1)) STORAGE DISK",
        vec![
            node(Kind::CreateTable, [0, 47], Some(1), None),
            node(Kind::Target, [13, 14], Some(2), Some(3)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [13, 14],
                }),
                [13, 14],
                None,
                None,
            ),
            node(Kind::TableDefinition, [14, 34], Some(4), Some(7)),
            node(
                Kind::ColumnDefinition {
                    nullability: Nullability::Implicit,
                },
                [15, 33],
                Some(5),
                None,
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"FOREIGN".to_vec(),
                    quoted: false,
                    span: [15, 22],
                }),
                [15, 22],
                None,
                Some(6),
            ),
            node(
                Kind::DataType(DataType::Utf8(TypeLength {
                    sign: "None",
                    digits: [31, 32],
                    span: [31, 32],
                })),
                [23, 33],
                None,
                None,
            ),
            node(Kind::StorageDisk, [35, 47], None, None),
        ],
        0,
    )
}

fn case_0182() -> Case {
    good(
        "context/column/foreign/Binary/False",
        "contextual",
        b"CREATE TABLE t(a BIGINT,foreign VARBINARY(1)) STORAGE DISK",
        vec![
            node(Kind::CreateTable, [0, 58], Some(1), None),
            node(Kind::Target, [13, 14], Some(2), Some(3)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [13, 14],
                }),
                [13, 14],
                None,
                None,
            ),
            node(Kind::TableDefinition, [14, 45], Some(4), Some(10)),
            node(
                Kind::ColumnDefinition {
                    nullability: Nullability::Implicit,
                },
                [15, 23],
                Some(5),
                Some(7),
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [15, 16],
                }),
                [15, 16],
                None,
                Some(6),
            ),
            node(Kind::DataType(DataType::Int64), [17, 23], None, None),
            node(
                Kind::ColumnDefinition {
                    nullability: Nullability::Implicit,
                },
                [24, 44],
                Some(8),
                None,
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"FOREIGN".to_vec(),
                    quoted: false,
                    span: [24, 31],
                }),
                [24, 31],
                None,
                Some(9),
            ),
            node(
                Kind::DataType(DataType::Binary(TypeLength {
                    sign: "None",
                    digits: [42, 43],
                    span: [42, 43],
                })),
                [32, 44],
                None,
                None,
            ),
            node(Kind::StorageDisk, [46, 58], None, None),
        ],
        0,
    )
}

fn case_0183() -> Case {
    good(
        "context/column/foreign/Binary/True",
        "contextual",
        b"CREATE TABLE t(foreign VARBINARY(1)) STORAGE DISK",
        vec![
            node(Kind::CreateTable, [0, 49], Some(1), None),
            node(Kind::Target, [13, 14], Some(2), Some(3)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [13, 14],
                }),
                [13, 14],
                None,
                None,
            ),
            node(Kind::TableDefinition, [14, 36], Some(4), Some(7)),
            node(
                Kind::ColumnDefinition {
                    nullability: Nullability::Implicit,
                },
                [15, 35],
                Some(5),
                None,
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"FOREIGN".to_vec(),
                    quoted: false,
                    span: [15, 22],
                }),
                [15, 22],
                None,
                Some(6),
            ),
            node(
                Kind::DataType(DataType::Binary(TypeLength {
                    sign: "None",
                    digits: [33, 34],
                    span: [33, 34],
                })),
                [23, 35],
                None,
                None,
            ),
            node(Kind::StorageDisk, [37, 49], None, None),
        ],
        0,
    )
}

fn case_0184() -> Case {
    good(
        "context/quoted-column/foreign",
        "contextual",
        b"CREATE TABLE t(\"foreign\" BIGINT) STORAGE DISK",
        vec![
            node(Kind::CreateTable, [0, 45], Some(1), None),
            node(Kind::Target, [13, 14], Some(2), Some(3)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [13, 14],
                }),
                [13, 14],
                None,
                None,
            ),
            node(Kind::TableDefinition, [14, 32], Some(4), Some(7)),
            node(
                Kind::ColumnDefinition {
                    nullability: Nullability::Implicit,
                },
                [15, 31],
                Some(5),
                None,
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"foreign".to_vec(),
                    quoted: true,
                    span: [15, 24],
                }),
                [15, 24],
                None,
                Some(6),
            ),
            node(Kind::DataType(DataType::Int64), [25, 31], None, None),
            node(Kind::StorageDisk, [33, 45], None, None),
        ],
        0,
    )
}

fn case_0185() -> Case {
    good(
        "context/column/constraint/Int64/False",
        "contextual",
        b"CREATE TABLE t(a BIGINT,constraint BIGINT) STORAGE DISK",
        vec![
            node(Kind::CreateTable, [0, 55], Some(1), None),
            node(Kind::Target, [13, 14], Some(2), Some(3)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [13, 14],
                }),
                [13, 14],
                None,
                None,
            ),
            node(Kind::TableDefinition, [14, 42], Some(4), Some(10)),
            node(
                Kind::ColumnDefinition {
                    nullability: Nullability::Implicit,
                },
                [15, 23],
                Some(5),
                Some(7),
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [15, 16],
                }),
                [15, 16],
                None,
                Some(6),
            ),
            node(Kind::DataType(DataType::Int64), [17, 23], None, None),
            node(
                Kind::ColumnDefinition {
                    nullability: Nullability::Implicit,
                },
                [24, 41],
                Some(8),
                None,
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"CONSTRAINT".to_vec(),
                    quoted: false,
                    span: [24, 34],
                }),
                [24, 34],
                None,
                Some(9),
            ),
            node(Kind::DataType(DataType::Int64), [35, 41], None, None),
            node(Kind::StorageDisk, [43, 55], None, None),
        ],
        0,
    )
}

fn case_0186() -> Case {
    good(
        "context/column/constraint/Int64/True",
        "contextual",
        b"CREATE TABLE t(constraint BIGINT) STORAGE DISK",
        vec![
            node(Kind::CreateTable, [0, 46], Some(1), None),
            node(Kind::Target, [13, 14], Some(2), Some(3)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [13, 14],
                }),
                [13, 14],
                None,
                None,
            ),
            node(Kind::TableDefinition, [14, 33], Some(4), Some(7)),
            node(
                Kind::ColumnDefinition {
                    nullability: Nullability::Implicit,
                },
                [15, 32],
                Some(5),
                None,
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"CONSTRAINT".to_vec(),
                    quoted: false,
                    span: [15, 25],
                }),
                [15, 25],
                None,
                Some(6),
            ),
            node(Kind::DataType(DataType::Int64), [26, 32], None, None),
            node(Kind::StorageDisk, [34, 46], None, None),
        ],
        0,
    )
}

fn case_0187() -> Case {
    good(
        "context/column/constraint/Bool/False",
        "contextual",
        b"CREATE TABLE t(a BIGINT,constraint BOOLEAN) STORAGE DISK",
        vec![
            node(Kind::CreateTable, [0, 56], Some(1), None),
            node(Kind::Target, [13, 14], Some(2), Some(3)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [13, 14],
                }),
                [13, 14],
                None,
                None,
            ),
            node(Kind::TableDefinition, [14, 43], Some(4), Some(10)),
            node(
                Kind::ColumnDefinition {
                    nullability: Nullability::Implicit,
                },
                [15, 23],
                Some(5),
                Some(7),
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [15, 16],
                }),
                [15, 16],
                None,
                Some(6),
            ),
            node(Kind::DataType(DataType::Int64), [17, 23], None, None),
            node(
                Kind::ColumnDefinition {
                    nullability: Nullability::Implicit,
                },
                [24, 42],
                Some(8),
                None,
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"CONSTRAINT".to_vec(),
                    quoted: false,
                    span: [24, 34],
                }),
                [24, 34],
                None,
                Some(9),
            ),
            node(Kind::DataType(DataType::Bool), [35, 42], None, None),
            node(Kind::StorageDisk, [44, 56], None, None),
        ],
        0,
    )
}

fn case_0188() -> Case {
    good(
        "context/column/constraint/Bool/True",
        "contextual",
        b"CREATE TABLE t(constraint BOOLEAN) STORAGE DISK",
        vec![
            node(Kind::CreateTable, [0, 47], Some(1), None),
            node(Kind::Target, [13, 14], Some(2), Some(3)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [13, 14],
                }),
                [13, 14],
                None,
                None,
            ),
            node(Kind::TableDefinition, [14, 34], Some(4), Some(7)),
            node(
                Kind::ColumnDefinition {
                    nullability: Nullability::Implicit,
                },
                [15, 33],
                Some(5),
                None,
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"CONSTRAINT".to_vec(),
                    quoted: false,
                    span: [15, 25],
                }),
                [15, 25],
                None,
                Some(6),
            ),
            node(Kind::DataType(DataType::Bool), [26, 33], None, None),
            node(Kind::StorageDisk, [35, 47], None, None),
        ],
        0,
    )
}

fn case_0189() -> Case {
    good(
        "context/column/constraint/Utf8/False",
        "contextual",
        b"CREATE TABLE t(a BIGINT,constraint VARCHAR(1)) STORAGE DISK",
        vec![
            node(Kind::CreateTable, [0, 59], Some(1), None),
            node(Kind::Target, [13, 14], Some(2), Some(3)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [13, 14],
                }),
                [13, 14],
                None,
                None,
            ),
            node(Kind::TableDefinition, [14, 46], Some(4), Some(10)),
            node(
                Kind::ColumnDefinition {
                    nullability: Nullability::Implicit,
                },
                [15, 23],
                Some(5),
                Some(7),
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [15, 16],
                }),
                [15, 16],
                None,
                Some(6),
            ),
            node(Kind::DataType(DataType::Int64), [17, 23], None, None),
            node(
                Kind::ColumnDefinition {
                    nullability: Nullability::Implicit,
                },
                [24, 45],
                Some(8),
                None,
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"CONSTRAINT".to_vec(),
                    quoted: false,
                    span: [24, 34],
                }),
                [24, 34],
                None,
                Some(9),
            ),
            node(
                Kind::DataType(DataType::Utf8(TypeLength {
                    sign: "None",
                    digits: [43, 44],
                    span: [43, 44],
                })),
                [35, 45],
                None,
                None,
            ),
            node(Kind::StorageDisk, [47, 59], None, None),
        ],
        0,
    )
}

fn case_0190() -> Case {
    good(
        "context/column/constraint/Utf8/True",
        "contextual",
        b"CREATE TABLE t(constraint VARCHAR(1)) STORAGE DISK",
        vec![
            node(Kind::CreateTable, [0, 50], Some(1), None),
            node(Kind::Target, [13, 14], Some(2), Some(3)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [13, 14],
                }),
                [13, 14],
                None,
                None,
            ),
            node(Kind::TableDefinition, [14, 37], Some(4), Some(7)),
            node(
                Kind::ColumnDefinition {
                    nullability: Nullability::Implicit,
                },
                [15, 36],
                Some(5),
                None,
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"CONSTRAINT".to_vec(),
                    quoted: false,
                    span: [15, 25],
                }),
                [15, 25],
                None,
                Some(6),
            ),
            node(
                Kind::DataType(DataType::Utf8(TypeLength {
                    sign: "None",
                    digits: [34, 35],
                    span: [34, 35],
                })),
                [26, 36],
                None,
                None,
            ),
            node(Kind::StorageDisk, [38, 50], None, None),
        ],
        0,
    )
}

fn case_0191() -> Case {
    good(
        "context/column/constraint/Binary/False",
        "contextual",
        b"CREATE TABLE t(a BIGINT,constraint VARBINARY(1)) STORAGE DISK",
        vec![
            node(Kind::CreateTable, [0, 61], Some(1), None),
            node(Kind::Target, [13, 14], Some(2), Some(3)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [13, 14],
                }),
                [13, 14],
                None,
                None,
            ),
            node(Kind::TableDefinition, [14, 48], Some(4), Some(10)),
            node(
                Kind::ColumnDefinition {
                    nullability: Nullability::Implicit,
                },
                [15, 23],
                Some(5),
                Some(7),
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"A".to_vec(),
                    quoted: false,
                    span: [15, 16],
                }),
                [15, 16],
                None,
                Some(6),
            ),
            node(Kind::DataType(DataType::Int64), [17, 23], None, None),
            node(
                Kind::ColumnDefinition {
                    nullability: Nullability::Implicit,
                },
                [24, 47],
                Some(8),
                None,
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"CONSTRAINT".to_vec(),
                    quoted: false,
                    span: [24, 34],
                }),
                [24, 34],
                None,
                Some(9),
            ),
            node(
                Kind::DataType(DataType::Binary(TypeLength {
                    sign: "None",
                    digits: [45, 46],
                    span: [45, 46],
                })),
                [35, 47],
                None,
                None,
            ),
            node(Kind::StorageDisk, [49, 61], None, None),
        ],
        0,
    )
}

fn case_0192() -> Case {
    good(
        "context/column/constraint/Binary/True",
        "contextual",
        b"CREATE TABLE t(constraint VARBINARY(1)) STORAGE DISK",
        vec![
            node(Kind::CreateTable, [0, 52], Some(1), None),
            node(Kind::Target, [13, 14], Some(2), Some(3)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [13, 14],
                }),
                [13, 14],
                None,
                None,
            ),
            node(Kind::TableDefinition, [14, 39], Some(4), Some(7)),
            node(
                Kind::ColumnDefinition {
                    nullability: Nullability::Implicit,
                },
                [15, 38],
                Some(5),
                None,
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"CONSTRAINT".to_vec(),
                    quoted: false,
                    span: [15, 25],
                }),
                [15, 25],
                None,
                Some(6),
            ),
            node(
                Kind::DataType(DataType::Binary(TypeLength {
                    sign: "None",
                    digits: [36, 37],
                    span: [36, 37],
                })),
                [26, 38],
                None,
                None,
            ),
            node(Kind::StorageDisk, [40, 52], None, None),
        ],
        0,
    )
}

fn case_0193() -> Case {
    good(
        "context/quoted-column/constraint",
        "contextual",
        b"CREATE TABLE t(\"constraint\" BIGINT) STORAGE DISK",
        vec![
            node(Kind::CreateTable, [0, 48], Some(1), None),
            node(Kind::Target, [13, 14], Some(2), Some(3)),
            node(
                Kind::Name(NameObservation {
                    bytes: b"T".to_vec(),
                    quoted: false,
                    span: [13, 14],
                }),
                [13, 14],
                None,
                None,
            ),
            node(Kind::TableDefinition, [14, 35], Some(4), Some(7)),
            node(
                Kind::ColumnDefinition {
                    nullability: Nullability::Implicit,
                },
                [15, 34],
                Some(5),
                None,
            ),
            node(
                Kind::Name(NameObservation {
                    bytes: b"constraint".to_vec(),
                    quoted: true,
                    span: [15, 27],
                }),
                [15, 27],
                None,
                Some(6),
            ),
            node(Kind::DataType(DataType::Int64), [28, 34], None, None),
            node(Kind::StorageDisk, [36, 48], None, None),
        ],
        0,
    )
}

fn case_0194() -> Case {
    bad(
        "context/constraint-feature/435245415445205441424c45207428/CHECK",
        "contextual",
        b"CREATE TABLE t(CHECK (a=1)) STORAGE DISK",
        "Unsupported",
        "Parse",
        [15, 20],
    )
}

fn case_0195() -> Case {
    bad(
        "context/constraint-feature/435245415445205441424c452074286120424947494e542c/CHECK",
        "contextual",
        b"CREATE TABLE t(a BIGINT,CHECK (a=1)) STORAGE DISK",
        "Unsupported",
        "Parse",
        [24, 29],
    )
}

fn case_0196() -> Case {
    bad(
        "context/constraint-feature/435245415445205441424c452074286120424947494e542c5052494d415259204b45592861292c/CHECK",
        "contextual",
        b"CREATE TABLE t(a BIGINT,PRIMARY KEY(a),CHECK (a=1)) STORAGE DISK",
        "Unsupported",
        "Parse",
        [39, 44],
    )
}

fn case_0197() -> Case {
    bad(
        "context/constraint-feature/435245415445205441424c452074286120424947494e542c554e495155452861292c/CHECK",
        "contextual",
        b"CREATE TABLE t(a BIGINT,UNIQUE(a),CHECK (a=1)) STORAGE DISK",
        "Unsupported",
        "Parse",
        [34, 39],
    )
}

fn case_0198() -> Case {
    bad(
        "context/constraint-feature/435245415445205441424c45207428/FOREIGN",
        "contextual",
        b"CREATE TABLE t(FOREIGN KEY(a)) STORAGE DISK",
        "Unsupported",
        "Parse",
        [15, 22],
    )
}

fn case_0199() -> Case {
    bad(
        "context/constraint-feature/435245415445205441424c452074286120424947494e542c/FOREIGN",
        "contextual",
        b"CREATE TABLE t(a BIGINT,FOREIGN KEY(a)) STORAGE DISK",
        "Unsupported",
        "Parse",
        [24, 31],
    )
}

fn case_0200() -> Case {
    bad(
        "context/constraint-feature/435245415445205441424c452074286120424947494e542c5052494d415259204b45592861292c/FOREIGN",
        "contextual",
        b"CREATE TABLE t(a BIGINT,PRIMARY KEY(a),FOREIGN KEY(a)) STORAGE DISK",
        "Unsupported",
        "Parse",
        [39, 46],
    )
}

fn case_0201() -> Case {
    bad(
        "context/constraint-feature/435245415445205441424c452074286120424947494e542c554e495155452861292c/FOREIGN",
        "contextual",
        b"CREATE TABLE t(a BIGINT,UNIQUE(a),FOREIGN KEY(a)) STORAGE DISK",
        "Unsupported",
        "Parse",
        [34, 41],
    )
}

fn case_0202() -> Case {
    bad(
        "context/constraint-feature/435245415445205441424c45207428/CONSTRAINT",
        "contextual",
        b"CREATE TABLE t(CONSTRAINT c UNIQUE(a)) STORAGE DISK",
        "Unsupported",
        "Parse",
        [15, 25],
    )
}

fn case_0203() -> Case {
    bad(
        "context/constraint-feature/435245415445205441424c452074286120424947494e542c/CONSTRAINT",
        "contextual",
        b"CREATE TABLE t(a BIGINT,CONSTRAINT c UNIQUE(a)) STORAGE DISK",
        "Unsupported",
        "Parse",
        [24, 34],
    )
}

fn case_0204() -> Case {
    bad(
        "context/constraint-feature/435245415445205441424c452074286120424947494e542c5052494d415259204b45592861292c/CONSTRAINT",
        "contextual",
        b"CREATE TABLE t(a BIGINT,PRIMARY KEY(a),CONSTRAINT c UNIQUE(a)) STORAGE DISK",
        "Unsupported",
        "Parse",
        [39, 49],
    )
}

fn case_0205() -> Case {
    bad(
        "context/constraint-feature/435245415445205441424c452074286120424947494e542c554e495155452861292c/CONSTRAINT",
        "contextual",
        b"CREATE TABLE t(a BIGINT,UNIQUE(a),CONSTRAINT c UNIQUE(a)) STORAGE DISK",
        "Unsupported",
        "Parse",
        [34, 44],
    )
}

fn case_0206() -> Case {
    bad(
        "context/peek-lex/53454c4543542064697374696e63742021",
        "contextual",
        b"SELECT distinct !",
        "Syntax",
        "Lex",
        [16, 17],
    )
}

fn case_0207() -> Case {
    bad(
        "context/peek-lex/435245415445205441424c45207428636865636b2021",
        "contextual",
        b"CREATE TABLE t(check !",
        "Syntax",
        "Lex",
        [21, 22],
    )
}

fn case_0208() -> Case {
    bad(
        "context/peek-lex/435245415445205441424c45207428666f726569676e2021",
        "contextual",
        b"CREATE TABLE t(foreign !",
        "Syntax",
        "Lex",
        [23, 24],
    )
}

fn case_0209() -> Case {
    bad(
        "context/peek-lex/435245415445205441424c45207428636f6e73747261696e742021",
        "contextual",
        b"CREATE TABLE t(constraint !",
        "Syntax",
        "Lex",
        [26, 27],
    )
}

fn case_0210() -> Case {
    bad(
        "unsupported/first/WITH",
        "diagnostic",
        b"WITH",
        "Unsupported",
        "Parse",
        [0, 4],
    )
}

fn case_0211() -> Case {
    bad(
        "unsupported/first/ALTER",
        "diagnostic",
        b"ALTER",
        "Unsupported",
        "Parse",
        [0, 5],
    )
}

fn case_0212() -> Case {
    bad(
        "unsupported/first/SAVEPOINT",
        "diagnostic",
        b"SAVEPOINT",
        "Unsupported",
        "Parse",
        [0, 9],
    )
}

fn case_0213() -> Case {
    bad(
        "unsupported/first/EXPLAIN",
        "diagnostic",
        b"EXPLAIN",
        "Unsupported",
        "Parse",
        [0, 7],
    )
}

fn case_0214() -> Case {
    bad(
        "unsupported/first/PREPARE",
        "diagnostic",
        b"PREPARE",
        "Unsupported",
        "Parse",
        [0, 7],
    )
}

fn case_0215() -> Case {
    bad(
        "unsupported/first/EXECUTE",
        "diagnostic",
        b"EXECUTE",
        "Unsupported",
        "Parse",
        [0, 7],
    )
}

fn case_0216() -> Case {
    bad(
        "first-select",
        "dispatch",
        b" /*prefix*/ SELECT ",
        "Syntax",
        "Parse",
        [19, 19],
    )
}

fn case_0217() -> Case {
    bad(
        "first-update",
        "dispatch",
        b" /*prefix*/ UPDATE ",
        "Syntax",
        "Parse",
        [19, 19],
    )
}

fn case_0218() -> Case {
    bad(
        "first-delete",
        "dispatch",
        b" /*prefix*/ DELETE ",
        "Syntax",
        "Parse",
        [19, 19],
    )
}

fn case_0219() -> Case {
    bad(
        "first-create",
        "dispatch",
        b" /*prefix*/ CREATE ",
        "Syntax",
        "Parse",
        [19, 19],
    )
}

fn case_0220() -> Case {
    bad(
        "first-drop",
        "dispatch",
        b" /*prefix*/ DROP ",
        "Syntax",
        "Parse",
        [17, 17],
    )
}

fn case_0221() -> Case {
    bad(
        "syntax/truncated/00",
        "diagnostic",
        b"SELECT *",
        "Syntax",
        "Parse",
        [8, 8],
    )
}

fn case_0222() -> Case {
    bad(
        "syntax/truncated/01",
        "diagnostic",
        b"SELECT a",
        "Syntax",
        "Parse",
        [8, 8],
    )
}

fn case_0223() -> Case {
    bad(
        "syntax/truncated/02",
        "diagnostic",
        b"SELECT a AS",
        "Syntax",
        "Parse",
        [11, 11],
    )
}

fn case_0224() -> Case {
    bad(
        "syntax/truncated/03",
        "diagnostic",
        b"SELECT a FROM",
        "Syntax",
        "Parse",
        [13, 13],
    )
}

fn case_0225() -> Case {
    bad(
        "syntax/truncated/04",
        "diagnostic",
        b"SELECT a FROM public.",
        "Syntax",
        "Parse",
        [21, 21],
    )
}

fn case_0226() -> Case {
    bad(
        "syntax/truncated/05",
        "diagnostic",
        b"SELECT a FROM t WHERE",
        "Syntax",
        "Parse",
        [21, 21],
    )
}

fn case_0227() -> Case {
    bad(
        "syntax/truncated/06",
        "diagnostic",
        b"SELECT a FROM t ORDER",
        "Syntax",
        "Parse",
        [21, 21],
    )
}

fn case_0228() -> Case {
    bad(
        "syntax/truncated/07",
        "diagnostic",
        b"SELECT a FROM t ORDER BY",
        "Syntax",
        "Parse",
        [24, 24],
    )
}

fn case_0229() -> Case {
    bad(
        "syntax/truncated/08",
        "diagnostic",
        b"SELECT a FROM t ORDER BY a NULLS",
        "Syntax",
        "Parse",
        [32, 32],
    )
}

fn case_0230() -> Case {
    bad(
        "syntax/truncated/09",
        "diagnostic",
        b"SELECT a FROM t LIMIT",
        "Syntax",
        "Parse",
        [21, 21],
    )
}

fn case_0231() -> Case {
    bad(
        "syntax/truncated/10",
        "diagnostic",
        b"UPDATE t",
        "Syntax",
        "Parse",
        [8, 8],
    )
}

fn case_0232() -> Case {
    bad(
        "syntax/truncated/11",
        "diagnostic",
        b"UPDATE t SET",
        "Syntax",
        "Parse",
        [12, 12],
    )
}

fn case_0233() -> Case {
    bad(
        "syntax/truncated/12",
        "diagnostic",
        b"UPDATE t SET a",
        "Syntax",
        "Parse",
        [14, 14],
    )
}

fn case_0234() -> Case {
    bad(
        "syntax/truncated/13",
        "diagnostic",
        b"UPDATE t SET a=",
        "Syntax",
        "Parse",
        [15, 15],
    )
}

fn case_0235() -> Case {
    bad(
        "syntax/truncated/14",
        "diagnostic",
        b"UPDATE t SET a=1,",
        "Syntax",
        "Parse",
        [17, 17],
    )
}

fn case_0236() -> Case {
    bad(
        "syntax/truncated/15",
        "diagnostic",
        b"UPDATE t SET a=1 WHERE",
        "Syntax",
        "Parse",
        [22, 22],
    )
}

fn case_0237() -> Case {
    bad(
        "syntax/truncated/16",
        "diagnostic",
        b"DELETE FROM",
        "Syntax",
        "Parse",
        [11, 11],
    )
}

fn case_0238() -> Case {
    bad(
        "syntax/truncated/17",
        "diagnostic",
        b"DELETE FROM public.",
        "Syntax",
        "Parse",
        [19, 19],
    )
}

fn case_0239() -> Case {
    bad(
        "syntax/truncated/18",
        "diagnostic",
        b"DELETE FROM t WHERE",
        "Syntax",
        "Parse",
        [19, 19],
    )
}

fn case_0240() -> Case {
    bad(
        "syntax/truncated/19",
        "diagnostic",
        b"CREATE TABLE",
        "Syntax",
        "Parse",
        [12, 12],
    )
}

fn case_0241() -> Case {
    bad(
        "syntax/truncated/20",
        "diagnostic",
        b"CREATE TABLE t",
        "Syntax",
        "Parse",
        [14, 14],
    )
}

fn case_0242() -> Case {
    bad(
        "syntax/truncated/21",
        "diagnostic",
        b"CREATE TABLE t(",
        "Syntax",
        "Parse",
        [15, 15],
    )
}

fn case_0243() -> Case {
    bad(
        "syntax/truncated/22",
        "diagnostic",
        b"CREATE TABLE t(a",
        "Syntax",
        "Parse",
        [16, 16],
    )
}

fn case_0244() -> Case {
    bad(
        "syntax/truncated/23",
        "diagnostic",
        b"CREATE TABLE t(a VARCHAR",
        "Syntax",
        "Parse",
        [24, 24],
    )
}

fn case_0245() -> Case {
    bad(
        "syntax/truncated/24",
        "diagnostic",
        b"CREATE TABLE t(a VARCHAR(",
        "Syntax",
        "Parse",
        [25, 25],
    )
}

fn case_0246() -> Case {
    bad(
        "syntax/truncated/25",
        "diagnostic",
        b"CREATE TABLE t(a VARCHAR(1",
        "Syntax",
        "Parse",
        [26, 26],
    )
}

fn case_0247() -> Case {
    bad(
        "syntax/truncated/26",
        "diagnostic",
        b"CREATE TABLE t(a BIGINT",
        "Syntax",
        "Parse",
        [23, 23],
    )
}

fn case_0248() -> Case {
    bad(
        "syntax/truncated/27",
        "diagnostic",
        b"CREATE TABLE t(a BIGINT NOT",
        "Syntax",
        "Parse",
        [27, 27],
    )
}

fn case_0249() -> Case {
    bad(
        "syntax/truncated/28",
        "diagnostic",
        b"CREATE TABLE t(a BIGINT)",
        "Syntax",
        "Parse",
        [24, 24],
    )
}

fn case_0250() -> Case {
    bad(
        "syntax/truncated/29",
        "diagnostic",
        b"CREATE TABLE t(a BIGINT) STORAGE",
        "Syntax",
        "Parse",
        [32, 32],
    )
}

fn case_0251() -> Case {
    bad(
        "syntax/truncated/30",
        "diagnostic",
        b"CREATE TABLE t(a BIGINT,PRIMARY",
        "Syntax",
        "Parse",
        [31, 31],
    )
}

fn case_0252() -> Case {
    bad(
        "syntax/truncated/31",
        "diagnostic",
        b"CREATE TABLE t(a BIGINT,PRIMARY KEY",
        "Syntax",
        "Parse",
        [35, 35],
    )
}

fn case_0253() -> Case {
    bad(
        "syntax/truncated/32",
        "diagnostic",
        b"CREATE TABLE t(a BIGINT,PRIMARY KEY(",
        "Syntax",
        "Parse",
        [36, 36],
    )
}

fn case_0254() -> Case {
    bad(
        "syntax/truncated/33",
        "diagnostic",
        b"CREATE TABLE t(a BIGINT,UNIQUE(",
        "Syntax",
        "Parse",
        [31, 31],
    )
}

fn case_0255() -> Case {
    bad(
        "syntax/truncated/34",
        "diagnostic",
        b"CREATE INDEX",
        "Syntax",
        "Parse",
        [12, 12],
    )
}

fn case_0256() -> Case {
    bad(
        "syntax/truncated/35",
        "diagnostic",
        b"CREATE UNIQUE",
        "Syntax",
        "Parse",
        [13, 13],
    )
}

fn case_0257() -> Case {
    bad(
        "syntax/truncated/36",
        "diagnostic",
        b"CREATE UNIQUE INDEX",
        "Syntax",
        "Parse",
        [19, 19],
    )
}

fn case_0258() -> Case {
    bad(
        "syntax/truncated/37",
        "diagnostic",
        b"CREATE INDEX i",
        "Syntax",
        "Parse",
        [14, 14],
    )
}

fn case_0259() -> Case {
    bad(
        "syntax/truncated/38",
        "diagnostic",
        b"CREATE INDEX i ON",
        "Syntax",
        "Parse",
        [17, 17],
    )
}

fn case_0260() -> Case {
    bad(
        "syntax/truncated/39",
        "diagnostic",
        b"CREATE INDEX i ON t",
        "Syntax",
        "Parse",
        [19, 19],
    )
}

fn case_0261() -> Case {
    bad(
        "syntax/truncated/40",
        "diagnostic",
        b"CREATE INDEX i ON t(",
        "Syntax",
        "Parse",
        [20, 20],
    )
}

fn case_0262() -> Case {
    bad(
        "syntax/truncated/41",
        "diagnostic",
        b"CREATE INDEX i ON t(a,",
        "Syntax",
        "Parse",
        [22, 22],
    )
}

fn case_0263() -> Case {
    bad(
        "syntax/truncated/42",
        "diagnostic",
        b"DROP TABLE",
        "Syntax",
        "Parse",
        [10, 10],
    )
}

fn case_0264() -> Case {
    bad(
        "syntax/truncated/43",
        "diagnostic",
        b"DROP INDEX",
        "Syntax",
        "Parse",
        [10, 10],
    )
}

fn case_0265() -> Case {
    bad(
        "syntax/truncated/44",
        "diagnostic",
        b"DROP INDEX public.",
        "Syntax",
        "Parse",
        [18, 18],
    )
}

fn case_0266() -> Case {
    bad(
        "syntax/structure/00",
        "diagnostic",
        b"SELECT FROM t",
        "Syntax",
        "Parse",
        [7, 11],
    )
}

fn case_0267() -> Case {
    bad(
        "syntax/structure/01",
        "diagnostic",
        b"SELECT ,a FROM t",
        "Syntax",
        "Parse",
        [7, 8],
    )
}

fn case_0268() -> Case {
    bad(
        "syntax/structure/02",
        "diagnostic",
        b"SELECT a, FROM t",
        "Syntax",
        "Parse",
        [10, 14],
    )
}

fn case_0269() -> Case {
    bad(
        "syntax/structure/03",
        "diagnostic",
        b"SELECT a AS FROM t",
        "Syntax",
        "Parse",
        [12, 16],
    )
}

fn case_0270() -> Case {
    bad(
        "syntax/structure/04",
        "diagnostic",
        b"SELECT * FROM SELECT",
        "Syntax",
        "Parse",
        [14, 20],
    )
}

fn case_0271() -> Case {
    bad(
        "syntax/structure/05",
        "diagnostic",
        b"SELECT * FROM t WHERE TRUE",
        "Syntax",
        "Parse",
        [26, 26],
    )
}

fn case_0272() -> Case {
    bad(
        "syntax/structure/06",
        "diagnostic",
        b"SELECT * FROM t WHERE a=(1)",
        "Syntax",
        "Parse",
        [24, 25],
    )
}

fn case_0273() -> Case {
    bad(
        "syntax/structure/07",
        "diagnostic",
        b"SELECT * FROM t ORDER a",
        "Syntax",
        "Parse",
        [22, 23],
    )
}

fn case_0274() -> Case {
    bad(
        "syntax/structure/08",
        "diagnostic",
        b"SELECT * FROM t ORDER BY a ASC DESC",
        "Syntax",
        "Parse",
        [31, 35],
    )
}

fn case_0275() -> Case {
    bad(
        "syntax/structure/09",
        "diagnostic",
        b"SELECT * FROM t ORDER BY a NULLS LAST ASC",
        "Syntax",
        "Parse",
        [38, 41],
    )
}

fn case_0276() -> Case {
    bad(
        "syntax/structure/10",
        "diagnostic",
        b"SELECT * FROM t ORDER BY a NULLS MIDDLE",
        "Syntax",
        "Parse",
        [33, 39],
    )
}

fn case_0277() -> Case {
    bad(
        "syntax/structure/11",
        "diagnostic",
        b"SELECT * FROM t ORDER BY a NULLS FIRST NULLS LAST",
        "Syntax",
        "Parse",
        [39, 44],
    )
}

fn case_0278() -> Case {
    bad(
        "syntax/structure/12",
        "diagnostic",
        b"SELECT * FROM t ORDER BY a WHERE a=1",
        "Syntax",
        "Parse",
        [27, 32],
    )
}

fn case_0279() -> Case {
    bad(
        "syntax/structure/13",
        "diagnostic",
        b"SELECT * FROM t LIMIT 1 ORDER BY a",
        "Syntax",
        "Parse",
        [24, 29],
    )
}

fn case_0280() -> Case {
    bad(
        "syntax/structure/14",
        "diagnostic",
        b"SELECT * FROM t LIMIT 1 LIMIT 2",
        "Syntax",
        "Parse",
        [24, 29],
    )
}

fn case_0281() -> Case {
    bad(
        "syntax/structure/15",
        "diagnostic",
        b"SELECT * FROM t WHERE a=1 WHERE b=2",
        "Syntax",
        "Parse",
        [26, 31],
    )
}

fn case_0282() -> Case {
    bad(
        "syntax/structure/16",
        "diagnostic",
        b"SELECT * FROM t ORDER BY a ORDER BY b",
        "Syntax",
        "Parse",
        [27, 32],
    )
}

fn case_0283() -> Case {
    bad(
        "syntax/structure/17",
        "diagnostic",
        b"UPDATE SET a=1",
        "Syntax",
        "Parse",
        [7, 10],
    )
}

fn case_0284() -> Case {
    bad(
        "syntax/structure/18",
        "diagnostic",
        b"UPDATE t a=1",
        "Unsupported",
        "Parse",
        [9, 10],
    )
}

fn case_0285() -> Case {
    bad(
        "syntax/structure/19",
        "diagnostic",
        b"UPDATE t SET WHERE",
        "Syntax",
        "Parse",
        [13, 18],
    )
}

fn case_0286() -> Case {
    bad(
        "syntax/structure/20",
        "diagnostic",
        b"UPDATE t SET a 1",
        "Syntax",
        "Parse",
        [15, 16],
    )
}

fn case_0287() -> Case {
    bad(
        "syntax/structure/21",
        "diagnostic",
        b"UPDATE t SET a=1,,b=2",
        "Syntax",
        "Parse",
        [17, 18],
    )
}

fn case_0288() -> Case {
    bad(
        "syntax/structure/22",
        "diagnostic",
        b"DELETE t",
        "Syntax",
        "Parse",
        [7, 8],
    )
}

fn case_0289() -> Case {
    bad(
        "syntax/structure/23",
        "diagnostic",
        b"DELETE FROM WHERE",
        "Syntax",
        "Parse",
        [12, 17],
    )
}

fn case_0290() -> Case {
    bad(
        "syntax/structure/24",
        "diagnostic",
        b"CREATE UNIQUE TABLE t(a BIGINT) STORAGE DISK",
        "Syntax",
        "Parse",
        [14, 19],
    )
}

fn case_0291() -> Case {
    bad(
        "syntax/structure/25",
        "diagnostic",
        b"CREATE TABLE t() STORAGE DISK",
        "Syntax",
        "Parse",
        [15, 16],
    )
}

fn case_0292() -> Case {
    bad(
        "syntax/structure/26",
        "diagnostic",
        b"CREATE TABLE t(a BIGINT,) STORAGE DISK",
        "Syntax",
        "Parse",
        [24, 25],
    )
}

fn case_0293() -> Case {
    bad(
        "syntax/structure/27",
        "diagnostic",
        b"CREATE TABLE t(a VARCHAR) STORAGE DISK",
        "Syntax",
        "Parse",
        [24, 25],
    )
}

fn case_0294() -> Case {
    bad(
        "syntax/structure/28",
        "diagnostic",
        b"CREATE TABLE t(a BIGINT(1)) STORAGE DISK",
        "Syntax",
        "Parse",
        [23, 24],
    )
}

fn case_0295() -> Case {
    bad(
        "syntax/structure/29",
        "diagnostic",
        b"CREATE TABLE t(a BOOLEAN(1)) STORAGE DISK",
        "Syntax",
        "Parse",
        [24, 25],
    )
}

fn case_0296() -> Case {
    bad(
        "syntax/structure/30",
        "diagnostic",
        b"CREATE TABLE t(a BIGINT NULL NOT NULL) STORAGE DISK",
        "Syntax",
        "Parse",
        [29, 32],
    )
}

fn case_0297() -> Case {
    bad(
        "syntax/structure/31",
        "diagnostic",
        b"CREATE TABLE t(a BIGINT NOT TRUE) STORAGE DISK",
        "Syntax",
        "Parse",
        [28, 32],
    )
}

fn case_0298() -> Case {
    bad(
        "syntax/structure/32",
        "diagnostic",
        b"CREATE TABLE t(a BIGINT,PRIMARY KEY(a),b BIGINT) STORAGE DISK",
        "Syntax",
        "Parse",
        [39, 40],
    )
}

fn case_0299() -> Case {
    bad(
        "syntax/structure/33",
        "diagnostic",
        b"CREATE TABLE t(a BIGINT,UNIQUE(a),PRIMARY KEY(a)) STORAGE DISK",
        "Syntax",
        "Parse",
        [34, 41],
    )
}

fn case_0300() -> Case {
    bad(
        "syntax/structure/34",
        "diagnostic",
        b"CREATE TABLE t(a BIGINT,PRIMARY KEY(a),PRIMARY KEY(a)) STORAGE DISK",
        "Syntax",
        "Parse",
        [39, 46],
    )
}

fn case_0301() -> Case {
    bad(
        "syntax/structure/35",
        "diagnostic",
        b"CREATE TABLE t(PRIMARY KEY(a)) STORAGE DISK",
        "Syntax",
        "Parse",
        [15, 22],
    )
}

fn case_0302() -> Case {
    bad(
        "syntax/structure/36",
        "diagnostic",
        b"CREATE TABLE t(UNIQUE(a)) STORAGE DISK",
        "Syntax",
        "Parse",
        [15, 21],
    )
}

fn case_0303() -> Case {
    bad(
        "syntax/structure/37",
        "diagnostic",
        b"CREATE TABLE t(a BIGINT,PRIMARY KEY()) STORAGE DISK",
        "Syntax",
        "Parse",
        [36, 37],
    )
}

fn case_0304() -> Case {
    bad(
        "syntax/structure/38",
        "diagnostic",
        b"CREATE TABLE t(a BIGINT,UNIQUE(a,)) STORAGE DISK",
        "Syntax",
        "Parse",
        [33, 34],
    )
}

fn case_0305() -> Case {
    bad(
        "syntax/structure/39",
        "diagnostic",
        b"CREATE TABLE t(a BIGINT) DISK",
        "Syntax",
        "Parse",
        [25, 29],
    )
}

fn case_0306() -> Case {
    bad(
        "syntax/structure/40",
        "diagnostic",
        b"CREATE TABLE t(a BIGINT) STORAGE ON",
        "Syntax",
        "Parse",
        [33, 35],
    )
}

fn case_0307() -> Case {
    bad(
        "syntax/structure/41",
        "diagnostic",
        b"CREATE INDEX i t(a)",
        "Syntax",
        "Parse",
        [15, 16],
    )
}

fn case_0308() -> Case {
    bad(
        "syntax/structure/42",
        "diagnostic",
        b"CREATE INDEX i ON t()",
        "Syntax",
        "Parse",
        [20, 21],
    )
}

fn case_0309() -> Case {
    bad(
        "syntax/structure/43",
        "diagnostic",
        b"CREATE INDEX i ON t(a,)",
        "Syntax",
        "Parse",
        [22, 23],
    )
}

fn case_0310() -> Case {
    bad(
        "syntax/structure/44",
        "diagnostic",
        b"DROP TABLE t extra",
        "Syntax",
        "Parse",
        [13, 18],
    )
}

fn case_0311() -> Case {
    bad(
        "syntax/structure/45",
        "diagnostic",
        b"DROP INDEX i extra",
        "Syntax",
        "Parse",
        [13, 18],
    )
}

fn case_0312() -> Case {
    bad(
        "syntax/single-statement/53454c45435420612046524f4d2074/3b3b",
        "diagnostic",
        b"SELECT a FROM t;;",
        "Syntax",
        "Parse",
        [16, 17],
    )
}

fn case_0313() -> Case {
    bad(
        "syntax/single-statement/53454c45435420612046524f4d2074/3b2053454c45435420612046524f4d2074",
        "diagnostic",
        b"SELECT a FROM t; SELECT a FROM t",
        "Syntax",
        "Parse",
        [17, 23],
    )
}

fn case_0314() -> Case {
    bad(
        "syntax/single-statement/55504441544520742053455420613d31/3b3b",
        "diagnostic",
        b"UPDATE t SET a=1;;",
        "Syntax",
        "Parse",
        [17, 18],
    )
}

fn case_0315() -> Case {
    bad(
        "syntax/single-statement/55504441544520742053455420613d31/3b2053454c45435420612046524f4d2074",
        "diagnostic",
        b"UPDATE t SET a=1; SELECT a FROM t",
        "Syntax",
        "Parse",
        [18, 24],
    )
}

fn case_0316() -> Case {
    bad(
        "syntax/single-statement/44454c4554452046524f4d2074/3b3b",
        "diagnostic",
        b"DELETE FROM t;;",
        "Syntax",
        "Parse",
        [14, 15],
    )
}

fn case_0317() -> Case {
    bad(
        "syntax/single-statement/44454c4554452046524f4d2074/3b2053454c45435420612046524f4d2074",
        "diagnostic",
        b"DELETE FROM t; SELECT a FROM t",
        "Syntax",
        "Parse",
        [15, 21],
    )
}

fn case_0318() -> Case {
    bad(
        "syntax/single-statement/435245415445205441424c452074286120424947494e54292053544f52414745204449534b/3b3b",
        "diagnostic",
        b"CREATE TABLE t(a BIGINT) STORAGE DISK;;",
        "Syntax",
        "Parse",
        [38, 39],
    )
}

fn case_0319() -> Case {
    bad(
        "syntax/single-statement/435245415445205441424c452074286120424947494e54292053544f52414745204449534b/3b2053454c45435420612046524f4d2074",
        "diagnostic",
        b"CREATE TABLE t(a BIGINT) STORAGE DISK; SELECT a FROM t",
        "Syntax",
        "Parse",
        [39, 45],
    )
}

fn case_0320() -> Case {
    bad(
        "syntax/single-statement/43524541544520494e4445582069204f4e2074286129/3b3b",
        "diagnostic",
        b"CREATE INDEX i ON t(a);;",
        "Syntax",
        "Parse",
        [23, 24],
    )
}

fn case_0321() -> Case {
    bad(
        "syntax/single-statement/43524541544520494e4445582069204f4e2074286129/3b2053454c45435420612046524f4d2074",
        "diagnostic",
        b"CREATE INDEX i ON t(a); SELECT a FROM t",
        "Syntax",
        "Parse",
        [24, 30],
    )
}

fn case_0322() -> Case {
    bad(
        "syntax/single-statement/44524f50205441424c452074/3b3b",
        "diagnostic",
        b"DROP TABLE t;;",
        "Syntax",
        "Parse",
        [13, 14],
    )
}

fn case_0323() -> Case {
    bad(
        "syntax/single-statement/44524f50205441424c452074/3b2053454c45435420612046524f4d2074",
        "diagnostic",
        b"DROP TABLE t; SELECT a FROM t",
        "Syntax",
        "Parse",
        [14, 20],
    )
}

fn case_0324() -> Case {
    bad(
        "syntax/single-statement/44524f5020494e4445582069/3b3b",
        "diagnostic",
        b"DROP INDEX i;;",
        "Syntax",
        "Parse",
        [13, 14],
    )
}

fn case_0325() -> Case {
    bad(
        "syntax/single-statement/44524f5020494e4445582069/3b2053454c45435420612046524f4d2074",
        "diagnostic",
        b"DROP INDEX i; SELECT a FROM t",
        "Syntax",
        "Parse",
        [14, 20],
    )
}

fn case_0326() -> Case {
    bad(
        "unsupported/projection/31",
        "diagnostic",
        b"SELECT 1 FROM t",
        "Unsupported",
        "Parse",
        [7, 8],
    )
}

fn case_0327() -> Case {
    bad(
        "unsupported/projection/277827",
        "diagnostic",
        b"SELECT 'x' FROM t",
        "Unsupported",
        "Parse",
        [7, 10],
    )
}

fn case_0328() -> Case {
    bad(
        "unsupported/projection/5827303027",
        "diagnostic",
        b"SELECT X'00' FROM t",
        "Unsupported",
        "Parse",
        [7, 12],
    )
}

fn case_0329() -> Case {
    bad(
        "unsupported/projection/54525545",
        "diagnostic",
        b"SELECT TRUE FROM t",
        "Unsupported",
        "Parse",
        [7, 11],
    )
}

fn case_0330() -> Case {
    bad(
        "unsupported/projection/46414c5345",
        "diagnostic",
        b"SELECT FALSE FROM t",
        "Unsupported",
        "Parse",
        [7, 12],
    )
}

fn case_0331() -> Case {
    bad(
        "unsupported/projection/4e554c4c",
        "diagnostic",
        b"SELECT NULL FROM t",
        "Unsupported",
        "Parse",
        [7, 11],
    )
}

fn case_0332() -> Case {
    bad(
        "unsupported/projection/3f",
        "diagnostic",
        b"SELECT ? FROM t",
        "Unsupported",
        "Parse",
        [7, 8],
    )
}

fn case_0333() -> Case {
    bad(
        "unsupported/projection/2b31",
        "diagnostic",
        b"SELECT +1 FROM t",
        "Unsupported",
        "Parse",
        [7, 8],
    )
}

fn case_0334() -> Case {
    bad(
        "unsupported/projection/2d31",
        "diagnostic",
        b"SELECT -1 FROM t",
        "Unsupported",
        "Parse",
        [7, 8],
    )
}

fn case_0335() -> Case {
    bad(
        "unsupported/projection/286129",
        "diagnostic",
        b"SELECT (a) FROM t",
        "Unsupported",
        "Parse",
        [7, 8],
    )
}

fn case_0336() -> Case {
    bad(
        "unsupported/order/31",
        "diagnostic",
        b"SELECT * FROM t ORDER BY 1",
        "Unsupported",
        "Parse",
        [25, 26],
    )
}

fn case_0337() -> Case {
    bad(
        "unsupported/order/277827",
        "diagnostic",
        b"SELECT * FROM t ORDER BY 'x'",
        "Unsupported",
        "Parse",
        [25, 28],
    )
}

fn case_0338() -> Case {
    bad(
        "unsupported/order/5827303027",
        "diagnostic",
        b"SELECT * FROM t ORDER BY X'00'",
        "Unsupported",
        "Parse",
        [25, 30],
    )
}

fn case_0339() -> Case {
    bad(
        "unsupported/order/54525545",
        "diagnostic",
        b"SELECT * FROM t ORDER BY TRUE",
        "Unsupported",
        "Parse",
        [25, 29],
    )
}

fn case_0340() -> Case {
    bad(
        "unsupported/order/46414c5345",
        "diagnostic",
        b"SELECT * FROM t ORDER BY FALSE",
        "Unsupported",
        "Parse",
        [25, 30],
    )
}

fn case_0341() -> Case {
    bad(
        "unsupported/order/4e554c4c",
        "diagnostic",
        b"SELECT * FROM t ORDER BY NULL",
        "Unsupported",
        "Parse",
        [25, 29],
    )
}

fn case_0342() -> Case {
    bad(
        "unsupported/order/3f",
        "diagnostic",
        b"SELECT * FROM t ORDER BY ?",
        "Unsupported",
        "Parse",
        [25, 26],
    )
}

fn case_0343() -> Case {
    bad(
        "unsupported/order/2b31",
        "diagnostic",
        b"SELECT * FROM t ORDER BY +1",
        "Unsupported",
        "Parse",
        [25, 26],
    )
}

fn case_0344() -> Case {
    bad(
        "unsupported/order/2d31",
        "diagnostic",
        b"SELECT * FROM t ORDER BY -1",
        "Unsupported",
        "Parse",
        [25, 26],
    )
}

fn case_0345() -> Case {
    bad(
        "unsupported/order/286129",
        "diagnostic",
        b"SELECT * FROM t ORDER BY (a)",
        "Unsupported",
        "Parse",
        [25, 26],
    )
}

fn case_0346() -> Case {
    bad(
        "unsupported/star-first-mixed",
        "diagnostic",
        b"SELECT *,a FROM t",
        "Unsupported",
        "Parse",
        [8, 9],
    )
}

fn case_0347() -> Case {
    bad(
        "unsupported/star-later-mixed",
        "diagnostic",
        b"SELECT a,* FROM t",
        "Unsupported",
        "Parse",
        [9, 10],
    )
}

fn case_0348() -> Case {
    bad(
        "unsupported/order-star",
        "diagnostic",
        b"SELECT * FROM t ORDER BY *",
        "Unsupported",
        "Parse",
        [25, 26],
    )
}

fn case_0349() -> Case {
    bad(
        "syntax/required-name-kind/53454c454354206120415320/31",
        "diagnostic",
        b"SELECT a AS 1 FROM t",
        "Syntax",
        "Parse",
        [12, 13],
    )
}

fn case_0350() -> Case {
    bad(
        "syntax/required-name-kind/55504441544520742053455420/31",
        "diagnostic",
        b"UPDATE t SET 1=1",
        "Syntax",
        "Parse",
        [13, 14],
    )
}

fn case_0351() -> Case {
    bad(
        "syntax/required-name-kind/53454c454354206120415320/277827",
        "diagnostic",
        b"SELECT a AS 'x' FROM t",
        "Syntax",
        "Parse",
        [12, 15],
    )
}

fn case_0352() -> Case {
    bad(
        "syntax/required-name-kind/55504441544520742053455420/277827",
        "diagnostic",
        b"UPDATE t SET 'x'=1",
        "Syntax",
        "Parse",
        [13, 16],
    )
}

fn case_0353() -> Case {
    bad(
        "syntax/required-name-kind/53454c454354206120415320/54525545",
        "diagnostic",
        b"SELECT a AS TRUE FROM t",
        "Syntax",
        "Parse",
        [12, 16],
    )
}

fn case_0354() -> Case {
    bad(
        "syntax/required-name-kind/55504441544520742053455420/54525545",
        "diagnostic",
        b"UPDATE t SET TRUE=1",
        "Syntax",
        "Parse",
        [13, 17],
    )
}

fn case_0355() -> Case {
    bad(
        "syntax/required-name-kind/53454c454354206120415320/4e554c4c",
        "diagnostic",
        b"SELECT a AS NULL FROM t",
        "Syntax",
        "Parse",
        [12, 16],
    )
}

fn case_0356() -> Case {
    bad(
        "syntax/required-name-kind/55504441544520742053455420/4e554c4c",
        "diagnostic",
        b"UPDATE t SET NULL=1",
        "Syntax",
        "Parse",
        [13, 17],
    )
}

fn case_0357() -> Case {
    bad(
        "syntax/required-name-kind/53454c454354206120415320/3f",
        "diagnostic",
        b"SELECT a AS ? FROM t",
        "Syntax",
        "Parse",
        [12, 13],
    )
}

fn case_0358() -> Case {
    bad(
        "syntax/required-name-kind/55504441544520742053455420/3f",
        "diagnostic",
        b"UPDATE t SET ?=1",
        "Syntax",
        "Parse",
        [13, 14],
    )
}

fn case_0359() -> Case {
    bad(
        "syntax/required-name-kind/53454c454354206120415320/2b31",
        "diagnostic",
        b"SELECT a AS +1 FROM t",
        "Syntax",
        "Parse",
        [12, 13],
    )
}

fn case_0360() -> Case {
    bad(
        "syntax/required-name-kind/55504441544520742053455420/2b31",
        "diagnostic",
        b"UPDATE t SET +1=1",
        "Syntax",
        "Parse",
        [13, 14],
    )
}

fn case_0361() -> Case {
    bad(
        "syntax/required-name-kind/53454c454354206120415320/286129",
        "diagnostic",
        b"SELECT a AS (a) FROM t",
        "Syntax",
        "Parse",
        [12, 13],
    )
}

fn case_0362() -> Case {
    bad(
        "syntax/required-name-kind/55504441544520742053455420/286129",
        "diagnostic",
        b"UPDATE t SET (a)=1",
        "Syntax",
        "Parse",
        [13, 14],
    )
}

fn case_0363() -> Case {
    bad(
        "syntax/required-name-kind/53454c454354206120415320/2a",
        "diagnostic",
        b"SELECT a AS * FROM t",
        "Syntax",
        "Parse",
        [12, 13],
    )
}

fn case_0364() -> Case {
    bad(
        "syntax/required-name-kind/55504441544520742053455420/2a",
        "diagnostic",
        b"UPDATE t SET *=1",
        "Syntax",
        "Parse",
        [13, 14],
    )
}

fn case_0365() -> Case {
    bad(
        "unsupported/suffix/projection/2e",
        "diagnostic",
        b"SELECT a.1 FROM t",
        "Unsupported",
        "Parse",
        [8, 9],
    )
}

fn case_0366() -> Case {
    bad(
        "unsupported/suffix/projection/28",
        "diagnostic",
        b"SELECT a(1 FROM t",
        "Unsupported",
        "Parse",
        [8, 9],
    )
}

fn case_0367() -> Case {
    bad(
        "unsupported/suffix/projection/2b",
        "diagnostic",
        b"SELECT a+1 FROM t",
        "Unsupported",
        "Parse",
        [8, 9],
    )
}

fn case_0368() -> Case {
    bad(
        "unsupported/suffix/projection/2d",
        "diagnostic",
        b"SELECT a-1 FROM t",
        "Unsupported",
        "Parse",
        [8, 9],
    )
}

fn case_0369() -> Case {
    bad(
        "unsupported/suffix/projection/2a",
        "diagnostic",
        b"SELECT a*1 FROM t",
        "Unsupported",
        "Parse",
        [8, 9],
    )
}

fn case_0370() -> Case {
    bad(
        "unsupported/suffix/alias/2e",
        "diagnostic",
        b"SELECT a AS x.1 FROM t",
        "Unsupported",
        "Parse",
        [13, 14],
    )
}

fn case_0371() -> Case {
    bad(
        "unsupported/suffix/alias/28",
        "diagnostic",
        b"SELECT a AS x(1 FROM t",
        "Unsupported",
        "Parse",
        [13, 14],
    )
}

fn case_0372() -> Case {
    bad(
        "unsupported/suffix/alias/2b",
        "diagnostic",
        b"SELECT a AS x+1 FROM t",
        "Unsupported",
        "Parse",
        [13, 14],
    )
}

fn case_0373() -> Case {
    bad(
        "unsupported/suffix/alias/2d",
        "diagnostic",
        b"SELECT a AS x-1 FROM t",
        "Unsupported",
        "Parse",
        [13, 14],
    )
}

fn case_0374() -> Case {
    bad(
        "unsupported/suffix/alias/2a",
        "diagnostic",
        b"SELECT a AS x*1 FROM t",
        "Unsupported",
        "Parse",
        [13, 14],
    )
}

fn case_0375() -> Case {
    bad(
        "unsupported/suffix/order/2e",
        "diagnostic",
        b"SELECT a FROM t ORDER BY a.1",
        "Unsupported",
        "Parse",
        [26, 27],
    )
}

fn case_0376() -> Case {
    bad(
        "unsupported/suffix/order/28",
        "diagnostic",
        b"SELECT a FROM t ORDER BY a(1",
        "Unsupported",
        "Parse",
        [26, 27],
    )
}

fn case_0377() -> Case {
    bad(
        "unsupported/suffix/order/2b",
        "diagnostic",
        b"SELECT a FROM t ORDER BY a+1",
        "Unsupported",
        "Parse",
        [26, 27],
    )
}

fn case_0378() -> Case {
    bad(
        "unsupported/suffix/order/2d",
        "diagnostic",
        b"SELECT a FROM t ORDER BY a-1",
        "Unsupported",
        "Parse",
        [26, 27],
    )
}

fn case_0379() -> Case {
    bad(
        "unsupported/suffix/order/2a",
        "diagnostic",
        b"SELECT a FROM t ORDER BY a*1",
        "Unsupported",
        "Parse",
        [26, 27],
    )
}

fn case_0380() -> Case {
    bad(
        "unsupported/suffix/update/2e",
        "diagnostic",
        b"UPDATE t SET a=1.1",
        "Unsupported",
        "Parse",
        [16, 17],
    )
}

fn case_0381() -> Case {
    bad(
        "unsupported/suffix/update/28",
        "diagnostic",
        b"UPDATE t SET a=1(1",
        "Unsupported",
        "Parse",
        [16, 17],
    )
}

fn case_0382() -> Case {
    bad(
        "unsupported/suffix/update/2b",
        "diagnostic",
        b"UPDATE t SET a=1+1",
        "Unsupported",
        "Parse",
        [16, 17],
    )
}

fn case_0383() -> Case {
    bad(
        "unsupported/suffix/update/2d",
        "diagnostic",
        b"UPDATE t SET a=1-1",
        "Unsupported",
        "Parse",
        [16, 17],
    )
}

fn case_0384() -> Case {
    bad(
        "unsupported/suffix/update/2a",
        "diagnostic",
        b"UPDATE t SET a=1*1",
        "Unsupported",
        "Parse",
        [16, 17],
    )
}

fn case_0385() -> Case {
    bad(
        "unsupported/suffix/index-key/2e",
        "diagnostic",
        b"CREATE INDEX i ON t(a.1)",
        "Unsupported",
        "Parse",
        [21, 22],
    )
}

fn case_0386() -> Case {
    bad(
        "unsupported/suffix/index-key/28",
        "diagnostic",
        b"CREATE INDEX i ON t(a(1)",
        "Unsupported",
        "Parse",
        [21, 22],
    )
}

fn case_0387() -> Case {
    bad(
        "unsupported/suffix/index-key/2b",
        "diagnostic",
        b"CREATE INDEX i ON t(a+1)",
        "Unsupported",
        "Parse",
        [21, 22],
    )
}

fn case_0388() -> Case {
    bad(
        "unsupported/suffix/index-key/2d",
        "diagnostic",
        b"CREATE INDEX i ON t(a-1)",
        "Unsupported",
        "Parse",
        [21, 22],
    )
}

fn case_0389() -> Case {
    bad(
        "unsupported/suffix/index-key/2a",
        "diagnostic",
        b"CREATE INDEX i ON t(a*1)",
        "Unsupported",
        "Parse",
        [21, 22],
    )
}

fn case_0390() -> Case {
    bad(
        "unsupported/target-alias/select/41532078",
        "diagnostic",
        b"SELECT * FROM t AS x",
        "Unsupported",
        "Parse",
        [16, 18],
    )
}

fn case_0391() -> Case {
    bad(
        "unsupported/target-alias/select/78",
        "diagnostic",
        b"SELECT * FROM t x",
        "Unsupported",
        "Parse",
        [16, 17],
    )
}

fn case_0392() -> Case {
    bad(
        "unsupported/target-alias/update/41532078",
        "diagnostic",
        b"UPDATE t AS x",
        "Unsupported",
        "Parse",
        [9, 11],
    )
}

fn case_0393() -> Case {
    bad(
        "unsupported/target-alias/update/78",
        "diagnostic",
        b"UPDATE t x",
        "Unsupported",
        "Parse",
        [9, 10],
    )
}

fn case_0394() -> Case {
    bad(
        "unsupported/target-alias/delete/41532078",
        "diagnostic",
        b"DELETE FROM t AS x",
        "Unsupported",
        "Parse",
        [14, 16],
    )
}

fn case_0395() -> Case {
    bad(
        "unsupported/target-alias/delete/78",
        "diagnostic",
        b"DELETE FROM t x",
        "Unsupported",
        "Parse",
        [14, 15],
    )
}

fn case_0396() -> Case {
    bad(
        "unsupported/projection-bare-alias",
        "diagnostic",
        b"SELECT a x FROM t",
        "Unsupported",
        "Parse",
        [9, 10],
    )
}

fn case_0397() -> Case {
    bad(
        "unsupported/join/2c75",
        "diagnostic",
        b"SELECT * FROM t ,u",
        "Unsupported",
        "Parse",
        [16, 17],
    )
}

fn case_0398() -> Case {
    bad(
        "unsupported/join/4a4f494e2075",
        "diagnostic",
        b"SELECT * FROM t JOIN u",
        "Unsupported",
        "Parse",
        [16, 20],
    )
}

fn case_0399() -> Case {
    bad(
        "unsupported/join/494e4e4552204a4f494e2075",
        "diagnostic",
        b"SELECT * FROM t INNER JOIN u",
        "Unsupported",
        "Parse",
        [16, 21],
    )
}

fn case_0400() -> Case {
    bad(
        "unsupported/join/4c454654204a4f494e2075",
        "diagnostic",
        b"SELECT * FROM t LEFT JOIN u",
        "Unsupported",
        "Parse",
        [16, 20],
    )
}

fn case_0401() -> Case {
    bad(
        "unsupported/join/5249474854204a4f494e2075",
        "diagnostic",
        b"SELECT * FROM t RIGHT JOIN u",
        "Unsupported",
        "Parse",
        [16, 21],
    )
}

fn case_0402() -> Case {
    bad(
        "unsupported/join/46554c4c204a4f494e2075",
        "diagnostic",
        b"SELECT * FROM t FULL JOIN u",
        "Unsupported",
        "Parse",
        [16, 20],
    )
}

fn case_0403() -> Case {
    bad(
        "unsupported/join/43524f5353204a4f494e2075",
        "diagnostic",
        b"SELECT * FROM t CROSS JOIN u",
        "Unsupported",
        "Parse",
        [16, 21],
    )
}

fn case_0404() -> Case {
    bad(
        "unsupported/join/4e41545552414c204a4f494e2075",
        "diagnostic",
        b"SELECT * FROM t NATURAL JOIN u",
        "Unsupported",
        "Parse",
        [16, 23],
    )
}

fn case_0405() -> Case {
    bad(
        "unsupported/select-extension/53454c454354202a2046524f4d207420/GROUP",
        "diagnostic",
        b"SELECT * FROM t GROUP",
        "Unsupported",
        "Parse",
        [16, 21],
    )
}

fn case_0406() -> Case {
    bad(
        "unsupported/select-extension/53454c454354202a2046524f4d207420574845524520613d3120/GROUP",
        "diagnostic",
        b"SELECT * FROM t WHERE a=1 GROUP",
        "Unsupported",
        "Parse",
        [26, 31],
    )
}

fn case_0407() -> Case {
    bad(
        "unsupported/select-extension/53454c454354202a2046524f4d207420/HAVING",
        "diagnostic",
        b"SELECT * FROM t HAVING",
        "Unsupported",
        "Parse",
        [16, 22],
    )
}

fn case_0408() -> Case {
    bad(
        "unsupported/select-extension/53454c454354202a2046524f4d207420574845524520613d3120/HAVING",
        "diagnostic",
        b"SELECT * FROM t WHERE a=1 HAVING",
        "Unsupported",
        "Parse",
        [26, 32],
    )
}

fn case_0409() -> Case {
    bad(
        "unsupported/select-extension/53454c454354202a2046524f4d207420/WINDOW",
        "diagnostic",
        b"SELECT * FROM t WINDOW",
        "Unsupported",
        "Parse",
        [16, 22],
    )
}

fn case_0410() -> Case {
    bad(
        "unsupported/select-extension/53454c454354202a2046524f4d207420574845524520613d3120/WINDOW",
        "diagnostic",
        b"SELECT * FROM t WHERE a=1 WINDOW",
        "Unsupported",
        "Parse",
        [26, 32],
    )
}

fn case_0411() -> Case {
    bad(
        "unsupported/select-extension/53454c454354202a2046524f4d207420/UNION",
        "diagnostic",
        b"SELECT * FROM t UNION",
        "Unsupported",
        "Parse",
        [16, 21],
    )
}

fn case_0412() -> Case {
    bad(
        "unsupported/select-extension/53454c454354202a2046524f4d207420574845524520613d3120/UNION",
        "diagnostic",
        b"SELECT * FROM t WHERE a=1 UNION",
        "Unsupported",
        "Parse",
        [26, 31],
    )
}

fn case_0413() -> Case {
    bad(
        "unsupported/select-extension/53454c454354202a2046524f4d207420/INTERSECT",
        "diagnostic",
        b"SELECT * FROM t INTERSECT",
        "Unsupported",
        "Parse",
        [16, 25],
    )
}

fn case_0414() -> Case {
    bad(
        "unsupported/select-extension/53454c454354202a2046524f4d207420574845524520613d3120/INTERSECT",
        "diagnostic",
        b"SELECT * FROM t WHERE a=1 INTERSECT",
        "Unsupported",
        "Parse",
        [26, 35],
    )
}

fn case_0415() -> Case {
    bad(
        "unsupported/select-extension/53454c454354202a2046524f4d207420/EXCEPT",
        "diagnostic",
        b"SELECT * FROM t EXCEPT",
        "Unsupported",
        "Parse",
        [16, 22],
    )
}

fn case_0416() -> Case {
    bad(
        "unsupported/select-extension/53454c454354202a2046524f4d207420574845524520613d3120/EXCEPT",
        "diagnostic",
        b"SELECT * FROM t WHERE a=1 EXCEPT",
        "Unsupported",
        "Parse",
        [26, 32],
    )
}

fn case_0417() -> Case {
    bad(
        "unsupported/select-extension/53454c454354202a2046524f4d207420/FETCH",
        "diagnostic",
        b"SELECT * FROM t FETCH",
        "Unsupported",
        "Parse",
        [16, 21],
    )
}

fn case_0418() -> Case {
    bad(
        "unsupported/select-extension/53454c454354202a2046524f4d207420574845524520613d3120/FETCH",
        "diagnostic",
        b"SELECT * FROM t WHERE a=1 FETCH",
        "Unsupported",
        "Parse",
        [26, 31],
    )
}

fn case_0419() -> Case {
    bad(
        "unsupported/select-extension/53454c454354202a2046524f4d207420/OFFSET",
        "diagnostic",
        b"SELECT * FROM t OFFSET",
        "Unsupported",
        "Parse",
        [16, 22],
    )
}

fn case_0420() -> Case {
    bad(
        "unsupported/select-extension/53454c454354202a2046524f4d207420574845524520613d3120/OFFSET",
        "diagnostic",
        b"SELECT * FROM t WHERE a=1 OFFSET",
        "Unsupported",
        "Parse",
        [26, 32],
    )
}

fn case_0421() -> Case {
    bad(
        "unsupported/select-extension/53454c454354202a2046524f4d207420/FOR",
        "diagnostic",
        b"SELECT * FROM t FOR",
        "Unsupported",
        "Parse",
        [16, 19],
    )
}

fn case_0422() -> Case {
    bad(
        "unsupported/select-extension/53454c454354202a2046524f4d207420574845524520613d3120/FOR",
        "diagnostic",
        b"SELECT * FROM t WHERE a=1 FOR",
        "Unsupported",
        "Parse",
        [26, 29],
    )
}

fn case_0423() -> Case {
    bad(
        "unsupported/dml-extension/55504441544520742053455420613d3120/RETURNING",
        "diagnostic",
        b"UPDATE t SET a=1 RETURNING",
        "Unsupported",
        "Parse",
        [17, 26],
    )
}

fn case_0424() -> Case {
    bad(
        "unsupported/dml-extension/55504441544520742053455420613d3120/FROM",
        "diagnostic",
        b"UPDATE t SET a=1 FROM",
        "Unsupported",
        "Parse",
        [17, 21],
    )
}

fn case_0425() -> Case {
    bad(
        "unsupported/dml-extension/55504441544520742053455420613d3120/ORDER",
        "diagnostic",
        b"UPDATE t SET a=1 ORDER",
        "Unsupported",
        "Parse",
        [17, 22],
    )
}

fn case_0426() -> Case {
    bad(
        "unsupported/dml-extension/55504441544520742053455420613d3120/LIMIT",
        "diagnostic",
        b"UPDATE t SET a=1 LIMIT",
        "Unsupported",
        "Parse",
        [17, 22],
    )
}

fn case_0427() -> Case {
    bad(
        "unsupported/dml-extension/44454c4554452046524f4d207420/RETURNING",
        "diagnostic",
        b"DELETE FROM t RETURNING",
        "Unsupported",
        "Parse",
        [14, 23],
    )
}

fn case_0428() -> Case {
    bad(
        "unsupported/dml-extension/44454c4554452046524f4d207420/USING",
        "diagnostic",
        b"DELETE FROM t USING",
        "Unsupported",
        "Parse",
        [14, 19],
    )
}

fn case_0429() -> Case {
    bad(
        "unsupported/dml-extension/44454c4554452046524f4d207420/ORDER",
        "diagnostic",
        b"DELETE FROM t ORDER",
        "Unsupported",
        "Parse",
        [14, 19],
    )
}

fn case_0430() -> Case {
    bad(
        "unsupported/dml-extension/44454c4554452046524f4d207420/LIMIT",
        "diagnostic",
        b"DELETE FROM t LIMIT",
        "Unsupported",
        "Parse",
        [14, 19],
    )
}

fn case_0431() -> Case {
    bad(
        "unsupported/dml-extension/55504441544520742053455420613d3120574845524520613d3120/RETURNING",
        "diagnostic",
        b"UPDATE t SET a=1 WHERE a=1 RETURNING",
        "Unsupported",
        "Parse",
        [27, 36],
    )
}

fn case_0432() -> Case {
    bad(
        "unsupported/dml-extension/55504441544520742053455420613d3120574845524520613d3120/FROM",
        "diagnostic",
        b"UPDATE t SET a=1 WHERE a=1 FROM",
        "Unsupported",
        "Parse",
        [27, 31],
    )
}

fn case_0433() -> Case {
    bad(
        "unsupported/dml-extension/44454c4554452046524f4d207420574845524520613d3120/RETURNING",
        "diagnostic",
        b"DELETE FROM t WHERE a=1 RETURNING",
        "Unsupported",
        "Parse",
        [24, 33],
    )
}

fn case_0434() -> Case {
    bad(
        "unsupported/dml-extension/44454c4554452046524f4d207420574845524520613d3120/USING",
        "diagnostic",
        b"DELETE FROM t WHERE a=1 USING",
        "Unsupported",
        "Parse",
        [24, 29],
    )
}

fn case_0435() -> Case {
    bad(
        "unsupported/update-rhs/6e616d65",
        "diagnostic",
        b"UPDATE t SET a=name",
        "Unsupported",
        "Parse",
        [15, 19],
    )
}

fn case_0436() -> Case {
    bad(
        "unsupported/update-rhs/226e616d6522",
        "diagnostic",
        b"UPDATE t SET a=\"name\"",
        "Unsupported",
        "Parse",
        [15, 21],
    )
}

fn case_0437() -> Case {
    bad(
        "unsupported/update-rhs/44454641554c54",
        "diagnostic",
        b"UPDATE t SET a=DEFAULT",
        "Unsupported",
        "Parse",
        [15, 22],
    )
}

fn case_0438() -> Case {
    bad(
        "unsupported/update-rhs/4e277827",
        "diagnostic",
        b"UPDATE t SET a=N'x'",
        "Unsupported",
        "Parse",
        [15, 16],
    )
}

fn case_0439() -> Case {
    bad(
        "unsupported/update-rhs/71277827",
        "diagnostic",
        b"UPDATE t SET a=q'x'",
        "Unsupported",
        "Parse",
        [15, 16],
    )
}

fn case_0440() -> Case {
    bad(
        "rhs/lookahead/55504441544520742053455420613d2853454c454354206129",
        "diagnostic",
        b"UPDATE t SET a=(SELECT a)",
        "Unsupported",
        "Parse",
        [16, 22],
    )
}

fn case_0441() -> Case {
    bad(
        "rhs/lookahead/55504441544520742053455420613d283129",
        "diagnostic",
        b"UPDATE t SET a=(1)",
        "Syntax",
        "Parse",
        [15, 16],
    )
}

fn case_0442() -> Case {
    bad(
        "rhs/lookahead/55504441544520742053455420613d2821",
        "diagnostic",
        b"UPDATE t SET a=(!",
        "Syntax",
        "Lex",
        [16, 17],
    )
}

fn case_0443() -> Case {
    bad(
        "rhs/lookahead/53454c45435420612046524f4d207420574845524520613d2821",
        "diagnostic",
        b"SELECT a FROM t WHERE a=(!",
        "Syntax",
        "Lex",
        [25, 26],
    )
}

fn case_0444() -> Case {
    bad(
        "unsupported/create/TEMP",
        "diagnostic",
        b"CREATE TEMP",
        "Unsupported",
        "Parse",
        [7, 11],
    )
}

fn case_0445() -> Case {
    bad(
        "unsupported/create/TEMPORARY",
        "diagnostic",
        b"CREATE TEMPORARY",
        "Unsupported",
        "Parse",
        [7, 16],
    )
}

fn case_0446() -> Case {
    bad(
        "unsupported/create/VIEW",
        "diagnostic",
        b"CREATE VIEW",
        "Unsupported",
        "Parse",
        [7, 11],
    )
}

fn case_0447() -> Case {
    bad(
        "unsupported/create/SCHEMA",
        "diagnostic",
        b"CREATE SCHEMA",
        "Unsupported",
        "Parse",
        [7, 13],
    )
}

fn case_0448() -> Case {
    bad(
        "unsupported/create/TRIGGER",
        "diagnostic",
        b"CREATE TRIGGER",
        "Unsupported",
        "Parse",
        [7, 14],
    )
}

fn case_0449() -> Case {
    bad(
        "unsupported/create/SEQUENCE",
        "diagnostic",
        b"CREATE SEQUENCE",
        "Unsupported",
        "Parse",
        [7, 15],
    )
}

fn case_0450() -> Case {
    bad(
        "unsupported/type/494e54",
        "diagnostic",
        b"CREATE TABLE t(a INT) STORAGE DISK",
        "Unsupported",
        "Parse",
        [17, 20],
    )
}

fn case_0451() -> Case {
    bad(
        "unsupported/type/494e5445474552",
        "diagnostic",
        b"CREATE TABLE t(a INTEGER) STORAGE DISK",
        "Unsupported",
        "Parse",
        [17, 24],
    )
}

fn case_0452() -> Case {
    bad(
        "unsupported/type/54455854",
        "diagnostic",
        b"CREATE TABLE t(a TEXT) STORAGE DISK",
        "Unsupported",
        "Parse",
        [17, 21],
    )
}

fn case_0453() -> Case {
    bad(
        "unsupported/type/4e554d45524943",
        "diagnostic",
        b"CREATE TABLE t(a NUMERIC) STORAGE DISK",
        "Unsupported",
        "Parse",
        [17, 24],
    )
}

fn case_0454() -> Case {
    bad(
        "unsupported/type/22424947494e5422",
        "diagnostic",
        b"CREATE TABLE t(a \"BIGINT\") STORAGE DISK",
        "Unsupported",
        "Parse",
        [17, 25],
    )
}

fn case_0455() -> Case {
    bad(
        "unsupported/inline/DEFAULT",
        "diagnostic",
        b"CREATE TABLE t(a BIGINT DEFAULT) STORAGE DISK",
        "Unsupported",
        "Parse",
        [24, 31],
    )
}

fn case_0456() -> Case {
    bad(
        "unsupported/inline/PRIMARY",
        "diagnostic",
        b"CREATE TABLE t(a BIGINT PRIMARY) STORAGE DISK",
        "Unsupported",
        "Parse",
        [24, 31],
    )
}

fn case_0457() -> Case {
    bad(
        "unsupported/inline/UNIQUE",
        "diagnostic",
        b"CREATE TABLE t(a BIGINT UNIQUE) STORAGE DISK",
        "Unsupported",
        "Parse",
        [24, 30],
    )
}

fn case_0458() -> Case {
    bad(
        "unsupported/inline/CHECK",
        "diagnostic",
        b"CREATE TABLE t(a BIGINT CHECK) STORAGE DISK",
        "Unsupported",
        "Parse",
        [24, 29],
    )
}

fn case_0459() -> Case {
    bad(
        "unsupported/inline/REFERENCES",
        "diagnostic",
        b"CREATE TABLE t(a BIGINT REFERENCES) STORAGE DISK",
        "Unsupported",
        "Parse",
        [24, 34],
    )
}

fn case_0460() -> Case {
    bad(
        "unsupported/inline/CONSTRAINT",
        "diagnostic",
        b"CREATE TABLE t(a BIGINT CONSTRAINT) STORAGE DISK",
        "Unsupported",
        "Parse",
        [24, 34],
    )
}

fn case_0461() -> Case {
    bad(
        "unsupported/inline/COLLATE",
        "diagnostic",
        b"CREATE TABLE t(a BIGINT COLLATE) STORAGE DISK",
        "Unsupported",
        "Parse",
        [24, 31],
    )
}

fn case_0462() -> Case {
    bad(
        "unsupported/inline/GENERATED",
        "diagnostic",
        b"CREATE TABLE t(a BIGINT GENERATED) STORAGE DISK",
        "Unsupported",
        "Parse",
        [24, 33],
    )
}

fn case_0463() -> Case {
    bad(
        "unsupported/index-direction/ASC",
        "diagnostic",
        b"CREATE INDEX i ON t(a ASC)",
        "Unsupported",
        "Parse",
        [22, 25],
    )
}

fn case_0464() -> Case {
    bad(
        "unsupported/index-direction/DESC",
        "diagnostic",
        b"CREATE INDEX i ON t(a DESC)",
        "Unsupported",
        "Parse",
        [22, 26],
    )
}

fn case_0465() -> Case {
    bad(
        "unsupported/index-direction/NULLS",
        "diagnostic",
        b"CREATE INDEX i ON t(a NULLS)",
        "Unsupported",
        "Parse",
        [22, 27],
    )
}

fn case_0466() -> Case {
    bad(
        "unsupported/index-key-start//31",
        "diagnostic",
        b"CREATE INDEX i ON t(1)",
        "Unsupported",
        "Parse",
        [20, 21],
    )
}

fn case_0467() -> Case {
    bad(
        "syntax/constraint-key-start/5052494d415259204b4559//31",
        "diagnostic",
        b"CREATE TABLE t(a BIGINT,PRIMARY KEY(1)) STORAGE DISK",
        "Syntax",
        "Parse",
        [36, 37],
    )
}

fn case_0468() -> Case {
    bad(
        "syntax/constraint-key-start/554e49515545//31",
        "diagnostic",
        b"CREATE TABLE t(a BIGINT,UNIQUE(1)) STORAGE DISK",
        "Syntax",
        "Parse",
        [31, 32],
    )
}

fn case_0469() -> Case {
    bad(
        "unsupported/index-key-start//277827",
        "diagnostic",
        b"CREATE INDEX i ON t('x')",
        "Unsupported",
        "Parse",
        [20, 23],
    )
}

fn case_0470() -> Case {
    bad(
        "syntax/constraint-key-start/5052494d415259204b4559//277827",
        "diagnostic",
        b"CREATE TABLE t(a BIGINT,PRIMARY KEY('x')) STORAGE DISK",
        "Syntax",
        "Parse",
        [36, 39],
    )
}

fn case_0471() -> Case {
    bad(
        "syntax/constraint-key-start/554e49515545//277827",
        "diagnostic",
        b"CREATE TABLE t(a BIGINT,UNIQUE('x')) STORAGE DISK",
        "Syntax",
        "Parse",
        [31, 34],
    )
}

fn case_0472() -> Case {
    bad(
        "unsupported/index-key-start//5827303027",
        "diagnostic",
        b"CREATE INDEX i ON t(X'00')",
        "Unsupported",
        "Parse",
        [20, 25],
    )
}

fn case_0473() -> Case {
    bad(
        "syntax/constraint-key-start/5052494d415259204b4559//5827303027",
        "diagnostic",
        b"CREATE TABLE t(a BIGINT,PRIMARY KEY(X'00')) STORAGE DISK",
        "Syntax",
        "Parse",
        [36, 41],
    )
}

fn case_0474() -> Case {
    bad(
        "syntax/constraint-key-start/554e49515545//5827303027",
        "diagnostic",
        b"CREATE TABLE t(a BIGINT,UNIQUE(X'00')) STORAGE DISK",
        "Syntax",
        "Parse",
        [31, 36],
    )
}

fn case_0475() -> Case {
    bad(
        "unsupported/index-key-start//54525545",
        "diagnostic",
        b"CREATE INDEX i ON t(TRUE)",
        "Unsupported",
        "Parse",
        [20, 24],
    )
}

fn case_0476() -> Case {
    bad(
        "syntax/constraint-key-start/5052494d415259204b4559//54525545",
        "diagnostic",
        b"CREATE TABLE t(a BIGINT,PRIMARY KEY(TRUE)) STORAGE DISK",
        "Syntax",
        "Parse",
        [36, 40],
    )
}

fn case_0477() -> Case {
    bad(
        "syntax/constraint-key-start/554e49515545//54525545",
        "diagnostic",
        b"CREATE TABLE t(a BIGINT,UNIQUE(TRUE)) STORAGE DISK",
        "Syntax",
        "Parse",
        [31, 35],
    )
}

fn case_0478() -> Case {
    bad(
        "unsupported/index-key-start//46414c5345",
        "diagnostic",
        b"CREATE INDEX i ON t(FALSE)",
        "Unsupported",
        "Parse",
        [20, 25],
    )
}

fn case_0479() -> Case {
    bad(
        "syntax/constraint-key-start/5052494d415259204b4559//46414c5345",
        "diagnostic",
        b"CREATE TABLE t(a BIGINT,PRIMARY KEY(FALSE)) STORAGE DISK",
        "Syntax",
        "Parse",
        [36, 41],
    )
}

fn case_0480() -> Case {
    bad(
        "syntax/constraint-key-start/554e49515545//46414c5345",
        "diagnostic",
        b"CREATE TABLE t(a BIGINT,UNIQUE(FALSE)) STORAGE DISK",
        "Syntax",
        "Parse",
        [31, 36],
    )
}

fn case_0481() -> Case {
    bad(
        "unsupported/index-key-start//4e554c4c",
        "diagnostic",
        b"CREATE INDEX i ON t(NULL)",
        "Unsupported",
        "Parse",
        [20, 24],
    )
}

fn case_0482() -> Case {
    bad(
        "syntax/constraint-key-start/5052494d415259204b4559//4e554c4c",
        "diagnostic",
        b"CREATE TABLE t(a BIGINT,PRIMARY KEY(NULL)) STORAGE DISK",
        "Syntax",
        "Parse",
        [36, 40],
    )
}

fn case_0483() -> Case {
    bad(
        "syntax/constraint-key-start/554e49515545//4e554c4c",
        "diagnostic",
        b"CREATE TABLE t(a BIGINT,UNIQUE(NULL)) STORAGE DISK",
        "Syntax",
        "Parse",
        [31, 35],
    )
}

fn case_0484() -> Case {
    bad(
        "unsupported/index-key-start//3f",
        "diagnostic",
        b"CREATE INDEX i ON t(?)",
        "Unsupported",
        "Parse",
        [20, 21],
    )
}

fn case_0485() -> Case {
    bad(
        "syntax/constraint-key-start/5052494d415259204b4559//3f",
        "diagnostic",
        b"CREATE TABLE t(a BIGINT,PRIMARY KEY(?)) STORAGE DISK",
        "Syntax",
        "Parse",
        [36, 37],
    )
}

fn case_0486() -> Case {
    bad(
        "syntax/constraint-key-start/554e49515545//3f",
        "diagnostic",
        b"CREATE TABLE t(a BIGINT,UNIQUE(?)) STORAGE DISK",
        "Syntax",
        "Parse",
        [31, 32],
    )
}

fn case_0487() -> Case {
    bad(
        "unsupported/index-key-start//2b31",
        "diagnostic",
        b"CREATE INDEX i ON t(+1)",
        "Unsupported",
        "Parse",
        [20, 21],
    )
}

fn case_0488() -> Case {
    bad(
        "syntax/constraint-key-start/5052494d415259204b4559//2b31",
        "diagnostic",
        b"CREATE TABLE t(a BIGINT,PRIMARY KEY(+1)) STORAGE DISK",
        "Syntax",
        "Parse",
        [36, 37],
    )
}

fn case_0489() -> Case {
    bad(
        "syntax/constraint-key-start/554e49515545//2b31",
        "diagnostic",
        b"CREATE TABLE t(a BIGINT,UNIQUE(+1)) STORAGE DISK",
        "Syntax",
        "Parse",
        [31, 32],
    )
}

fn case_0490() -> Case {
    bad(
        "unsupported/index-key-start//2d31",
        "diagnostic",
        b"CREATE INDEX i ON t(-1)",
        "Unsupported",
        "Parse",
        [20, 21],
    )
}

fn case_0491() -> Case {
    bad(
        "syntax/constraint-key-start/5052494d415259204b4559//2d31",
        "diagnostic",
        b"CREATE TABLE t(a BIGINT,PRIMARY KEY(-1)) STORAGE DISK",
        "Syntax",
        "Parse",
        [36, 37],
    )
}

fn case_0492() -> Case {
    bad(
        "syntax/constraint-key-start/554e49515545//2d31",
        "diagnostic",
        b"CREATE TABLE t(a BIGINT,UNIQUE(-1)) STORAGE DISK",
        "Syntax",
        "Parse",
        [31, 32],
    )
}

fn case_0493() -> Case {
    bad(
        "unsupported/index-key-start//286229",
        "diagnostic",
        b"CREATE INDEX i ON t((b))",
        "Unsupported",
        "Parse",
        [20, 21],
    )
}

fn case_0494() -> Case {
    bad(
        "syntax/constraint-key-start/5052494d415259204b4559//286229",
        "diagnostic",
        b"CREATE TABLE t(a BIGINT,PRIMARY KEY((b))) STORAGE DISK",
        "Syntax",
        "Parse",
        [36, 37],
    )
}

fn case_0495() -> Case {
    bad(
        "syntax/constraint-key-start/554e49515545//286229",
        "diagnostic",
        b"CREATE TABLE t(a BIGINT,UNIQUE((b))) STORAGE DISK",
        "Syntax",
        "Parse",
        [31, 32],
    )
}

fn case_0496() -> Case {
    bad(
        "unsupported/index-key-start//2a",
        "diagnostic",
        b"CREATE INDEX i ON t(*)",
        "Unsupported",
        "Parse",
        [20, 21],
    )
}

fn case_0497() -> Case {
    bad(
        "syntax/constraint-key-start/5052494d415259204b4559//2a",
        "diagnostic",
        b"CREATE TABLE t(a BIGINT,PRIMARY KEY(*)) STORAGE DISK",
        "Syntax",
        "Parse",
        [36, 37],
    )
}

fn case_0498() -> Case {
    bad(
        "syntax/constraint-key-start/554e49515545//2a",
        "diagnostic",
        b"CREATE TABLE t(a BIGINT,UNIQUE(*)) STORAGE DISK",
        "Syntax",
        "Parse",
        [31, 32],
    )
}

fn case_0499() -> Case {
    bad(
        "unsupported/index-key-start/612c/31",
        "diagnostic",
        b"CREATE INDEX i ON t(a,1)",
        "Unsupported",
        "Parse",
        [22, 23],
    )
}

fn case_0500() -> Case {
    bad(
        "syntax/constraint-key-start/5052494d415259204b4559/612c/31",
        "diagnostic",
        b"CREATE TABLE t(a BIGINT,PRIMARY KEY(a,1)) STORAGE DISK",
        "Syntax",
        "Parse",
        [38, 39],
    )
}

fn case_0501() -> Case {
    bad(
        "syntax/constraint-key-start/554e49515545/612c/31",
        "diagnostic",
        b"CREATE TABLE t(a BIGINT,UNIQUE(a,1)) STORAGE DISK",
        "Syntax",
        "Parse",
        [33, 34],
    )
}

fn case_0502() -> Case {
    bad(
        "unsupported/index-key-start/612c/277827",
        "diagnostic",
        b"CREATE INDEX i ON t(a,'x')",
        "Unsupported",
        "Parse",
        [22, 25],
    )
}

fn case_0503() -> Case {
    bad(
        "syntax/constraint-key-start/5052494d415259204b4559/612c/277827",
        "diagnostic",
        b"CREATE TABLE t(a BIGINT,PRIMARY KEY(a,'x')) STORAGE DISK",
        "Syntax",
        "Parse",
        [38, 41],
    )
}

fn case_0504() -> Case {
    bad(
        "syntax/constraint-key-start/554e49515545/612c/277827",
        "diagnostic",
        b"CREATE TABLE t(a BIGINT,UNIQUE(a,'x')) STORAGE DISK",
        "Syntax",
        "Parse",
        [33, 36],
    )
}

fn case_0505() -> Case {
    bad(
        "unsupported/index-key-start/612c/5827303027",
        "diagnostic",
        b"CREATE INDEX i ON t(a,X'00')",
        "Unsupported",
        "Parse",
        [22, 27],
    )
}

fn case_0506() -> Case {
    bad(
        "syntax/constraint-key-start/5052494d415259204b4559/612c/5827303027",
        "diagnostic",
        b"CREATE TABLE t(a BIGINT,PRIMARY KEY(a,X'00')) STORAGE DISK",
        "Syntax",
        "Parse",
        [38, 43],
    )
}

fn case_0507() -> Case {
    bad(
        "syntax/constraint-key-start/554e49515545/612c/5827303027",
        "diagnostic",
        b"CREATE TABLE t(a BIGINT,UNIQUE(a,X'00')) STORAGE DISK",
        "Syntax",
        "Parse",
        [33, 38],
    )
}

fn case_0508() -> Case {
    bad(
        "unsupported/index-key-start/612c/54525545",
        "diagnostic",
        b"CREATE INDEX i ON t(a,TRUE)",
        "Unsupported",
        "Parse",
        [22, 26],
    )
}

fn case_0509() -> Case {
    bad(
        "syntax/constraint-key-start/5052494d415259204b4559/612c/54525545",
        "diagnostic",
        b"CREATE TABLE t(a BIGINT,PRIMARY KEY(a,TRUE)) STORAGE DISK",
        "Syntax",
        "Parse",
        [38, 42],
    )
}

fn case_0510() -> Case {
    bad(
        "syntax/constraint-key-start/554e49515545/612c/54525545",
        "diagnostic",
        b"CREATE TABLE t(a BIGINT,UNIQUE(a,TRUE)) STORAGE DISK",
        "Syntax",
        "Parse",
        [33, 37],
    )
}

fn case_0511() -> Case {
    bad(
        "unsupported/index-key-start/612c/46414c5345",
        "diagnostic",
        b"CREATE INDEX i ON t(a,FALSE)",
        "Unsupported",
        "Parse",
        [22, 27],
    )
}

fn case_0512() -> Case {
    bad(
        "syntax/constraint-key-start/5052494d415259204b4559/612c/46414c5345",
        "diagnostic",
        b"CREATE TABLE t(a BIGINT,PRIMARY KEY(a,FALSE)) STORAGE DISK",
        "Syntax",
        "Parse",
        [38, 43],
    )
}

fn case_0513() -> Case {
    bad(
        "syntax/constraint-key-start/554e49515545/612c/46414c5345",
        "diagnostic",
        b"CREATE TABLE t(a BIGINT,UNIQUE(a,FALSE)) STORAGE DISK",
        "Syntax",
        "Parse",
        [33, 38],
    )
}

fn case_0514() -> Case {
    bad(
        "unsupported/index-key-start/612c/4e554c4c",
        "diagnostic",
        b"CREATE INDEX i ON t(a,NULL)",
        "Unsupported",
        "Parse",
        [22, 26],
    )
}

fn case_0515() -> Case {
    bad(
        "syntax/constraint-key-start/5052494d415259204b4559/612c/4e554c4c",
        "diagnostic",
        b"CREATE TABLE t(a BIGINT,PRIMARY KEY(a,NULL)) STORAGE DISK",
        "Syntax",
        "Parse",
        [38, 42],
    )
}

fn case_0516() -> Case {
    bad(
        "syntax/constraint-key-start/554e49515545/612c/4e554c4c",
        "diagnostic",
        b"CREATE TABLE t(a BIGINT,UNIQUE(a,NULL)) STORAGE DISK",
        "Syntax",
        "Parse",
        [33, 37],
    )
}

fn case_0517() -> Case {
    bad(
        "unsupported/index-key-start/612c/3f",
        "diagnostic",
        b"CREATE INDEX i ON t(a,?)",
        "Unsupported",
        "Parse",
        [22, 23],
    )
}

fn case_0518() -> Case {
    bad(
        "syntax/constraint-key-start/5052494d415259204b4559/612c/3f",
        "diagnostic",
        b"CREATE TABLE t(a BIGINT,PRIMARY KEY(a,?)) STORAGE DISK",
        "Syntax",
        "Parse",
        [38, 39],
    )
}

fn case_0519() -> Case {
    bad(
        "syntax/constraint-key-start/554e49515545/612c/3f",
        "diagnostic",
        b"CREATE TABLE t(a BIGINT,UNIQUE(a,?)) STORAGE DISK",
        "Syntax",
        "Parse",
        [33, 34],
    )
}

fn case_0520() -> Case {
    bad(
        "unsupported/index-key-start/612c/2b31",
        "diagnostic",
        b"CREATE INDEX i ON t(a,+1)",
        "Unsupported",
        "Parse",
        [22, 23],
    )
}

fn case_0521() -> Case {
    bad(
        "syntax/constraint-key-start/5052494d415259204b4559/612c/2b31",
        "diagnostic",
        b"CREATE TABLE t(a BIGINT,PRIMARY KEY(a,+1)) STORAGE DISK",
        "Syntax",
        "Parse",
        [38, 39],
    )
}

fn case_0522() -> Case {
    bad(
        "syntax/constraint-key-start/554e49515545/612c/2b31",
        "diagnostic",
        b"CREATE TABLE t(a BIGINT,UNIQUE(a,+1)) STORAGE DISK",
        "Syntax",
        "Parse",
        [33, 34],
    )
}

fn case_0523() -> Case {
    bad(
        "unsupported/index-key-start/612c/2d31",
        "diagnostic",
        b"CREATE INDEX i ON t(a,-1)",
        "Unsupported",
        "Parse",
        [22, 23],
    )
}

fn case_0524() -> Case {
    bad(
        "syntax/constraint-key-start/5052494d415259204b4559/612c/2d31",
        "diagnostic",
        b"CREATE TABLE t(a BIGINT,PRIMARY KEY(a,-1)) STORAGE DISK",
        "Syntax",
        "Parse",
        [38, 39],
    )
}

fn case_0525() -> Case {
    bad(
        "syntax/constraint-key-start/554e49515545/612c/2d31",
        "diagnostic",
        b"CREATE TABLE t(a BIGINT,UNIQUE(a,-1)) STORAGE DISK",
        "Syntax",
        "Parse",
        [33, 34],
    )
}

fn case_0526() -> Case {
    bad(
        "unsupported/index-key-start/612c/286229",
        "diagnostic",
        b"CREATE INDEX i ON t(a,(b))",
        "Unsupported",
        "Parse",
        [22, 23],
    )
}

fn case_0527() -> Case {
    bad(
        "syntax/constraint-key-start/5052494d415259204b4559/612c/286229",
        "diagnostic",
        b"CREATE TABLE t(a BIGINT,PRIMARY KEY(a,(b))) STORAGE DISK",
        "Syntax",
        "Parse",
        [38, 39],
    )
}

fn case_0528() -> Case {
    bad(
        "syntax/constraint-key-start/554e49515545/612c/286229",
        "diagnostic",
        b"CREATE TABLE t(a BIGINT,UNIQUE(a,(b))) STORAGE DISK",
        "Syntax",
        "Parse",
        [33, 34],
    )
}

fn case_0529() -> Case {
    bad(
        "unsupported/index-key-start/612c/2a",
        "diagnostic",
        b"CREATE INDEX i ON t(a,*)",
        "Unsupported",
        "Parse",
        [22, 23],
    )
}

fn case_0530() -> Case {
    bad(
        "syntax/constraint-key-start/5052494d415259204b4559/612c/2a",
        "diagnostic",
        b"CREATE TABLE t(a BIGINT,PRIMARY KEY(a,*)) STORAGE DISK",
        "Syntax",
        "Parse",
        [38, 39],
    )
}

fn case_0531() -> Case {
    bad(
        "syntax/constraint-key-start/554e49515545/612c/2a",
        "diagnostic",
        b"CREATE TABLE t(a BIGINT,UNIQUE(a,*)) STORAGE DISK",
        "Syntax",
        "Parse",
        [33, 34],
    )
}

fn case_0532() -> Case {
    bad(
        "unsupported/index-extension/WHERE",
        "diagnostic",
        b"CREATE INDEX i ON t(a) WHERE",
        "Unsupported",
        "Parse",
        [23, 28],
    )
}

fn case_0533() -> Case {
    bad(
        "unsupported/index-extension/INCLUDE",
        "diagnostic",
        b"CREATE INDEX i ON t(a) INCLUDE",
        "Unsupported",
        "Parse",
        [23, 30],
    )
}

fn case_0534() -> Case {
    bad(
        "unsupported/index-extension/USING",
        "diagnostic",
        b"CREATE INDEX i ON t(a) USING",
        "Unsupported",
        "Parse",
        [23, 28],
    )
}

fn case_0535() -> Case {
    bad(
        "unsupported/drop-extension/44524f50205441424c45207420/43415343414445",
        "diagnostic",
        b"DROP TABLE t CASCADE",
        "Unsupported",
        "Parse",
        [13, 20],
    )
}

fn case_0536() -> Case {
    bad(
        "unsupported/drop-extension/44524f50205441424c45207420/5245535452494354",
        "diagnostic",
        b"DROP TABLE t RESTRICT",
        "Unsupported",
        "Parse",
        [13, 21],
    )
}

fn case_0537() -> Case {
    bad(
        "unsupported/drop-extension/44524f50205441424c45207420/2c",
        "diagnostic",
        b"DROP TABLE t ,",
        "Unsupported",
        "Parse",
        [13, 14],
    )
}

fn case_0538() -> Case {
    bad(
        "unsupported/drop-extension/44524f5020494e444558206920/43415343414445",
        "diagnostic",
        b"DROP INDEX i CASCADE",
        "Unsupported",
        "Parse",
        [13, 20],
    )
}

fn case_0539() -> Case {
    bad(
        "unsupported/drop-extension/44524f5020494e444558206920/5245535452494354",
        "diagnostic",
        b"DROP INDEX i RESTRICT",
        "Unsupported",
        "Parse",
        [13, 21],
    )
}

fn case_0540() -> Case {
    bad(
        "unsupported/drop-extension/44524f5020494e444558206920/2c",
        "diagnostic",
        b"DROP INDEX i ,",
        "Unsupported",
        "Parse",
        [13, 14],
    )
}

fn case_0541() -> Case {
    bad(
        "unsupported/storage/4d454d4f5259",
        "diagnostic",
        b"CREATE TABLE t(a BIGINT) STORAGE MEMORY",
        "Unsupported",
        "Parse",
        [33, 39],
    )
}

fn case_0542() -> Case {
    bad(
        "unsupported/storage/6f74686572",
        "diagnostic",
        b"CREATE TABLE t(a BIGINT) STORAGE other",
        "Unsupported",
        "Parse",
        [33, 38],
    )
}

fn case_0543() -> Case {
    bad(
        "unsupported/storage/224449534b22",
        "diagnostic",
        b"CREATE TABLE t(a BIGINT) STORAGE \"DISK\"",
        "Unsupported",
        "Parse",
        [33, 39],
    )
}

fn case_0544() -> Case {
    bad(
        "unsupported/target-dot/53454c45435420612046524f4d20",
        "diagnostic",
        b"SELECT a FROM PUBLIC.t.c",
        "Unsupported",
        "Parse",
        [22, 23],
    )
}

fn case_0545() -> Case {
    bad(
        "unsupported/schema/53454c45435420612046524f4d20",
        "diagnostic",
        b"SELECT a FROM other.t",
        "Unsupported",
        "Parse",
        [14, 19],
    )
}

fn case_0546() -> Case {
    bad(
        "unsupported/target-dot/55504441544520",
        "diagnostic",
        b"UPDATE PUBLIC.t.c SET a=1",
        "Unsupported",
        "Parse",
        [15, 16],
    )
}

fn case_0547() -> Case {
    bad(
        "unsupported/schema/55504441544520",
        "diagnostic",
        b"UPDATE other.t SET a=1",
        "Unsupported",
        "Parse",
        [7, 12],
    )
}

fn case_0548() -> Case {
    bad(
        "unsupported/target-dot/44454c4554452046524f4d20",
        "diagnostic",
        b"DELETE FROM PUBLIC.t.c",
        "Unsupported",
        "Parse",
        [20, 21],
    )
}

fn case_0549() -> Case {
    bad(
        "unsupported/schema/44454c4554452046524f4d20",
        "diagnostic",
        b"DELETE FROM other.t",
        "Unsupported",
        "Parse",
        [12, 17],
    )
}

fn case_0550() -> Case {
    bad(
        "unsupported/target-dot/435245415445205441424c4520",
        "diagnostic",
        b"CREATE TABLE PUBLIC.t.c(a BIGINT) STORAGE DISK",
        "Unsupported",
        "Parse",
        [21, 22],
    )
}

fn case_0551() -> Case {
    bad(
        "unsupported/schema/435245415445205441424c4520",
        "diagnostic",
        b"CREATE TABLE other.t(a BIGINT) STORAGE DISK",
        "Unsupported",
        "Parse",
        [13, 18],
    )
}

fn case_0552() -> Case {
    bad(
        "unsupported/target-dot/43524541544520494e44455820",
        "diagnostic",
        b"CREATE INDEX PUBLIC.t.c ON t(a)",
        "Unsupported",
        "Parse",
        [21, 22],
    )
}

fn case_0553() -> Case {
    bad(
        "unsupported/schema/43524541544520494e44455820",
        "diagnostic",
        b"CREATE INDEX other.t ON t(a)",
        "Unsupported",
        "Parse",
        [13, 18],
    )
}

fn case_0554() -> Case {
    bad(
        "unsupported/target-dot/43524541544520494e4445582069204f4e20",
        "diagnostic",
        b"CREATE INDEX i ON PUBLIC.t.c(a)",
        "Unsupported",
        "Parse",
        [26, 27],
    )
}

fn case_0555() -> Case {
    bad(
        "unsupported/schema/43524541544520494e4445582069204f4e20",
        "diagnostic",
        b"CREATE INDEX i ON other.t(a)",
        "Unsupported",
        "Parse",
        [18, 23],
    )
}

fn case_0556() -> Case {
    bad(
        "unsupported/target-dot/44524f50205441424c4520",
        "diagnostic",
        b"DROP TABLE PUBLIC.t.c",
        "Unsupported",
        "Parse",
        [19, 20],
    )
}

fn case_0557() -> Case {
    bad(
        "unsupported/schema/44524f50205441424c4520",
        "diagnostic",
        b"DROP TABLE other.t",
        "Unsupported",
        "Parse",
        [11, 16],
    )
}

fn case_0558() -> Case {
    bad(
        "unsupported/target-dot/44524f5020494e44455820",
        "diagnostic",
        b"DROP INDEX PUBLIC.t.c",
        "Unsupported",
        "Parse",
        [19, 20],
    )
}

fn case_0559() -> Case {
    bad(
        "unsupported/schema/44524f5020494e44455820",
        "diagnostic",
        b"DROP INDEX other.t",
        "Unsupported",
        "Parse",
        [11, 16],
    )
}

fn case_0560() -> Case {
    bad(
        "syntax/signed-required-digits/55504441544520742053455420613d/2b/6e616d65",
        "diagnostic",
        b"UPDATE t SET a=+name",
        "Syntax",
        "Parse",
        [16, 20],
    )
}

fn case_0561() -> Case {
    bad(
        "syntax/signed-required-digits/55504441544520742053455420613d/2b/44454641554c54",
        "diagnostic",
        b"UPDATE t SET a=+DEFAULT",
        "Syntax",
        "Parse",
        [16, 23],
    )
}

fn case_0562() -> Case {
    bad(
        "syntax/signed-required-digits/55504441544520742053455420613d/2b/53454c454354",
        "diagnostic",
        b"UPDATE t SET a=+SELECT",
        "Syntax",
        "Parse",
        [16, 22],
    )
}

fn case_0563() -> Case {
    bad(
        "syntax/signed-required-digits/55504441544520742053455420613d/2b/3f",
        "diagnostic",
        b"UPDATE t SET a=+?",
        "Syntax",
        "Parse",
        [16, 17],
    )
}

fn case_0564() -> Case {
    bad(
        "syntax/signed-required-digits/55504441544520742053455420613d/2b/4e554c4c",
        "diagnostic",
        b"UPDATE t SET a=+NULL",
        "Syntax",
        "Parse",
        [16, 20],
    )
}

fn case_0565() -> Case {
    bad(
        "syntax/signed-required-digits/55504441544520742053455420613d/2b/2b31",
        "diagnostic",
        b"UPDATE t SET a=++1",
        "Syntax",
        "Parse",
        [16, 17],
    )
}

fn case_0566() -> Case {
    bad(
        "syntax/signed-required-digits/55504441544520742053455420613d/2b/277827",
        "diagnostic",
        b"UPDATE t SET a=+'x'",
        "Syntax",
        "Parse",
        [16, 19],
    )
}

fn case_0567() -> Case {
    bad(
        "syntax/signed-required-digits/55504441544520742053455420613d/2b/5827303027",
        "diagnostic",
        b"UPDATE t SET a=+X'00'",
        "Syntax",
        "Parse",
        [16, 21],
    )
}

fn case_0568() -> Case {
    bad(
        "syntax/signed-required-digits/55504441544520742053455420613d/2d20/6e616d65",
        "diagnostic",
        b"UPDATE t SET a=- name",
        "Syntax",
        "Parse",
        [17, 21],
    )
}

fn case_0569() -> Case {
    bad(
        "syntax/signed-required-digits/55504441544520742053455420613d/2d20/44454641554c54",
        "diagnostic",
        b"UPDATE t SET a=- DEFAULT",
        "Syntax",
        "Parse",
        [17, 24],
    )
}

fn case_0570() -> Case {
    bad(
        "syntax/signed-required-digits/55504441544520742053455420613d/2d20/53454c454354",
        "diagnostic",
        b"UPDATE t SET a=- SELECT",
        "Syntax",
        "Parse",
        [17, 23],
    )
}

fn case_0571() -> Case {
    bad(
        "syntax/signed-required-digits/55504441544520742053455420613d/2d20/3f",
        "diagnostic",
        b"UPDATE t SET a=- ?",
        "Syntax",
        "Parse",
        [17, 18],
    )
}

fn case_0572() -> Case {
    bad(
        "syntax/signed-required-digits/55504441544520742053455420613d/2d20/4e554c4c",
        "diagnostic",
        b"UPDATE t SET a=- NULL",
        "Syntax",
        "Parse",
        [17, 21],
    )
}

fn case_0573() -> Case {
    bad(
        "syntax/signed-required-digits/55504441544520742053455420613d/2d20/2b31",
        "diagnostic",
        b"UPDATE t SET a=- +1",
        "Syntax",
        "Parse",
        [17, 18],
    )
}

fn case_0574() -> Case {
    bad(
        "syntax/signed-required-digits/55504441544520742053455420613d/2d20/277827",
        "diagnostic",
        b"UPDATE t SET a=- 'x'",
        "Syntax",
        "Parse",
        [17, 20],
    )
}

fn case_0575() -> Case {
    bad(
        "syntax/signed-required-digits/55504441544520742053455420613d/2d20/5827303027",
        "diagnostic",
        b"UPDATE t SET a=- X'00'",
        "Syntax",
        "Parse",
        [17, 22],
    )
}

fn case_0576() -> Case {
    bad(
        "syntax/signed-required-digits/435245415445205441424c4520742861205641524348415228/2b/6e616d65",
        "diagnostic",
        b"CREATE TABLE t(a VARCHAR(+name) STORAGE DISK",
        "Syntax",
        "Parse",
        [26, 30],
    )
}

fn case_0577() -> Case {
    bad(
        "syntax/signed-required-digits/435245415445205441424c4520742861205641524348415228/2b/44454641554c54",
        "diagnostic",
        b"CREATE TABLE t(a VARCHAR(+DEFAULT) STORAGE DISK",
        "Syntax",
        "Parse",
        [26, 33],
    )
}

fn case_0578() -> Case {
    bad(
        "syntax/signed-required-digits/435245415445205441424c4520742861205641524348415228/2b/53454c454354",
        "diagnostic",
        b"CREATE TABLE t(a VARCHAR(+SELECT) STORAGE DISK",
        "Syntax",
        "Parse",
        [26, 32],
    )
}

fn case_0579() -> Case {
    bad(
        "syntax/signed-required-digits/435245415445205441424c4520742861205641524348415228/2b/3f",
        "diagnostic",
        b"CREATE TABLE t(a VARCHAR(+?) STORAGE DISK",
        "Syntax",
        "Parse",
        [26, 27],
    )
}

fn case_0580() -> Case {
    bad(
        "syntax/signed-required-digits/435245415445205441424c4520742861205641524348415228/2b/4e554c4c",
        "diagnostic",
        b"CREATE TABLE t(a VARCHAR(+NULL) STORAGE DISK",
        "Syntax",
        "Parse",
        [26, 30],
    )
}

fn case_0581() -> Case {
    bad(
        "syntax/signed-required-digits/435245415445205441424c4520742861205641524348415228/2b/2b31",
        "diagnostic",
        b"CREATE TABLE t(a VARCHAR(++1) STORAGE DISK",
        "Syntax",
        "Parse",
        [26, 27],
    )
}

fn case_0582() -> Case {
    bad(
        "syntax/signed-required-digits/435245415445205441424c4520742861205641524348415228/2b/277827",
        "diagnostic",
        b"CREATE TABLE t(a VARCHAR(+'x') STORAGE DISK",
        "Syntax",
        "Parse",
        [26, 29],
    )
}

fn case_0583() -> Case {
    bad(
        "syntax/signed-required-digits/435245415445205441424c4520742861205641524348415228/2b/5827303027",
        "diagnostic",
        b"CREATE TABLE t(a VARCHAR(+X'00') STORAGE DISK",
        "Syntax",
        "Parse",
        [26, 31],
    )
}

fn case_0584() -> Case {
    bad(
        "syntax/signed-required-digits/435245415445205441424c4520742861205641524348415228/2d20/6e616d65",
        "diagnostic",
        b"CREATE TABLE t(a VARCHAR(- name) STORAGE DISK",
        "Syntax",
        "Parse",
        [27, 31],
    )
}

fn case_0585() -> Case {
    bad(
        "syntax/signed-required-digits/435245415445205441424c4520742861205641524348415228/2d20/44454641554c54",
        "diagnostic",
        b"CREATE TABLE t(a VARCHAR(- DEFAULT) STORAGE DISK",
        "Syntax",
        "Parse",
        [27, 34],
    )
}

fn case_0586() -> Case {
    bad(
        "syntax/signed-required-digits/435245415445205441424c4520742861205641524348415228/2d20/53454c454354",
        "diagnostic",
        b"CREATE TABLE t(a VARCHAR(- SELECT) STORAGE DISK",
        "Syntax",
        "Parse",
        [27, 33],
    )
}

fn case_0587() -> Case {
    bad(
        "syntax/signed-required-digits/435245415445205441424c4520742861205641524348415228/2d20/3f",
        "diagnostic",
        b"CREATE TABLE t(a VARCHAR(- ?) STORAGE DISK",
        "Syntax",
        "Parse",
        [27, 28],
    )
}

fn case_0588() -> Case {
    bad(
        "syntax/signed-required-digits/435245415445205441424c4520742861205641524348415228/2d20/4e554c4c",
        "diagnostic",
        b"CREATE TABLE t(a VARCHAR(- NULL) STORAGE DISK",
        "Syntax",
        "Parse",
        [27, 31],
    )
}

fn case_0589() -> Case {
    bad(
        "syntax/signed-required-digits/435245415445205441424c4520742861205641524348415228/2d20/2b31",
        "diagnostic",
        b"CREATE TABLE t(a VARCHAR(- +1) STORAGE DISK",
        "Syntax",
        "Parse",
        [27, 28],
    )
}

fn case_0590() -> Case {
    bad(
        "syntax/signed-required-digits/435245415445205441424c4520742861205641524348415228/2d20/277827",
        "diagnostic",
        b"CREATE TABLE t(a VARCHAR(- 'x') STORAGE DISK",
        "Syntax",
        "Parse",
        [27, 30],
    )
}

fn case_0591() -> Case {
    bad(
        "syntax/signed-required-digits/435245415445205441424c4520742861205641524348415228/2d20/5827303027",
        "diagnostic",
        b"CREATE TABLE t(a VARCHAR(- X'00') STORAGE DISK",
        "Syntax",
        "Parse",
        [27, 32],
    )
}

fn case_0592() -> Case {
    bad(
        "syntax/type-length-kind/3f",
        "diagnostic",
        b"CREATE TABLE t(a VARCHAR(?)) STORAGE DISK",
        "Syntax",
        "Parse",
        [25, 26],
    )
}

fn case_0593() -> Case {
    bad(
        "syntax/type-length-kind/4e554c4c",
        "diagnostic",
        b"CREATE TABLE t(a VARCHAR(NULL)) STORAGE DISK",
        "Syntax",
        "Parse",
        [25, 29],
    )
}

fn case_0594() -> Case {
    bad(
        "syntax/type-length-kind/277827",
        "diagnostic",
        b"CREATE TABLE t(a VARCHAR('x')) STORAGE DISK",
        "Syntax",
        "Parse",
        [25, 28],
    )
}

fn case_0595() -> Case {
    bad(
        "syntax/type-length-kind/5827303027",
        "diagnostic",
        b"CREATE TABLE t(a VARCHAR(X'00')) STORAGE DISK",
        "Syntax",
        "Parse",
        [25, 30],
    )
}

fn case_0596() -> Case {
    bad(
        "syntax/type-length-kind/6e616d65",
        "diagnostic",
        b"CREATE TABLE t(a VARCHAR(name)) STORAGE DISK",
        "Syntax",
        "Parse",
        [25, 29],
    )
}

fn case_0597() -> Case {
    bad(
        "syntax/type-length-kind/54525545",
        "diagnostic",
        b"CREATE TABLE t(a VARCHAR(TRUE)) STORAGE DISK",
        "Syntax",
        "Parse",
        [25, 29],
    )
}

fn case_0598() -> Case {
    bad(
        "unsupported/numeric-form/55504441544520742053455420613d/2e35",
        "diagnostic",
        b"UPDATE t SET a=.5",
        "Unsupported",
        "Parse",
        [15, 16],
    )
}

fn case_0599() -> Case {
    bad(
        "unsupported/numeric-form/55504441544520742053455420613d/2b2e35",
        "diagnostic",
        b"UPDATE t SET a=+.5",
        "Unsupported",
        "Parse",
        [16, 17],
    )
}

fn case_0600() -> Case {
    bad(
        "unsupported/numeric-form/55504441544520742053455420613d/2d202e35",
        "diagnostic",
        b"UPDATE t SET a=- .5",
        "Unsupported",
        "Parse",
        [17, 18],
    )
}

fn case_0601() -> Case {
    bad(
        "unsupported/numeric-form/55504441544520742053455420613d/312e35",
        "diagnostic",
        b"UPDATE t SET a=1.5",
        "Unsupported",
        "Parse",
        [16, 17],
    )
}

fn case_0602() -> Case {
    bad(
        "unsupported/numeric-form/55504441544520742053455420613d/316532",
        "diagnostic",
        b"UPDATE t SET a=1e2",
        "Unsupported",
        "Parse",
        [16, 18],
    )
}

fn case_0603() -> Case {
    bad(
        "unsupported/numeric-form/55504441544520742053455420613d/3165",
        "diagnostic",
        b"UPDATE t SET a=1e",
        "Unsupported",
        "Parse",
        [16, 17],
    )
}

fn case_0604() -> Case {
    bad(
        "unsupported/numeric-form/55504441544520742053455420613d/31652b32",
        "diagnostic",
        b"UPDATE t SET a=1e+2",
        "Unsupported",
        "Parse",
        [16, 17],
    )
}

fn case_0605() -> Case {
    bad(
        "syntax/numeric-form/55504441544520742053455420613d/2e",
        "diagnostic",
        b"UPDATE t SET a=.",
        "Syntax",
        "Parse",
        [15, 16],
    )
}

fn case_0606() -> Case {
    bad(
        "syntax/numeric-form/55504441544520742053455420613d/2b202e",
        "diagnostic",
        b"UPDATE t SET a=+ .",
        "Syntax",
        "Parse",
        [17, 18],
    )
}

fn case_0607() -> Case {
    bad(
        "syntax/numeric-form/55504441544520742053455420613d/3165666f6f",
        "diagnostic",
        b"UPDATE t SET a=1efoo",
        "Syntax",
        "Parse",
        [16, 20],
    )
}

fn case_0608() -> Case {
    bad(
        "syntax/numeric-form/55504441544520742053455420613d/31206532",
        "diagnostic",
        b"UPDATE t SET a=1 e2",
        "Syntax",
        "Parse",
        [17, 19],
    )
}

fn case_0609() -> Case {
    bad(
        "unsupported/numeric-form/435245415445205441424c4520742861205641524348415228/2e35",
        "diagnostic",
        b"CREATE TABLE t(a VARCHAR(.5)) STORAGE DISK",
        "Unsupported",
        "Parse",
        [25, 26],
    )
}

fn case_0610() -> Case {
    bad(
        "unsupported/numeric-form/435245415445205441424c4520742861205641524348415228/2b2e35",
        "diagnostic",
        b"CREATE TABLE t(a VARCHAR(+.5)) STORAGE DISK",
        "Unsupported",
        "Parse",
        [26, 27],
    )
}

fn case_0611() -> Case {
    bad(
        "unsupported/numeric-form/435245415445205441424c4520742861205641524348415228/2d202e35",
        "diagnostic",
        b"CREATE TABLE t(a VARCHAR(- .5)) STORAGE DISK",
        "Unsupported",
        "Parse",
        [27, 28],
    )
}

fn case_0612() -> Case {
    bad(
        "unsupported/numeric-form/435245415445205441424c4520742861205641524348415228/312e35",
        "diagnostic",
        b"CREATE TABLE t(a VARCHAR(1.5)) STORAGE DISK",
        "Unsupported",
        "Parse",
        [26, 27],
    )
}

fn case_0613() -> Case {
    bad(
        "unsupported/numeric-form/435245415445205441424c4520742861205641524348415228/316532",
        "diagnostic",
        b"CREATE TABLE t(a VARCHAR(1e2)) STORAGE DISK",
        "Unsupported",
        "Parse",
        [26, 28],
    )
}

fn case_0614() -> Case {
    bad(
        "unsupported/numeric-form/435245415445205441424c4520742861205641524348415228/3165",
        "diagnostic",
        b"CREATE TABLE t(a VARCHAR(1e)) STORAGE DISK",
        "Unsupported",
        "Parse",
        [26, 27],
    )
}

fn case_0615() -> Case {
    bad(
        "unsupported/numeric-form/435245415445205441424c4520742861205641524348415228/31652b32",
        "diagnostic",
        b"CREATE TABLE t(a VARCHAR(1e+2)) STORAGE DISK",
        "Unsupported",
        "Parse",
        [26, 27],
    )
}

fn case_0616() -> Case {
    bad(
        "syntax/numeric-form/435245415445205441424c4520742861205641524348415228/2e",
        "diagnostic",
        b"CREATE TABLE t(a VARCHAR(.)) STORAGE DISK",
        "Syntax",
        "Parse",
        [25, 26],
    )
}

fn case_0617() -> Case {
    bad(
        "syntax/numeric-form/435245415445205441424c4520742861205641524348415228/2b202e",
        "diagnostic",
        b"CREATE TABLE t(a VARCHAR(+ .)) STORAGE DISK",
        "Syntax",
        "Parse",
        [27, 28],
    )
}

fn case_0618() -> Case {
    bad(
        "syntax/numeric-form/435245415445205441424c4520742861205641524348415228/3165666f6f",
        "diagnostic",
        b"CREATE TABLE t(a VARCHAR(1efoo)) STORAGE DISK",
        "Syntax",
        "Parse",
        [26, 30],
    )
}

fn case_0619() -> Case {
    bad(
        "syntax/numeric-form/435245415445205441424c4520742861205641524348415228/31206532",
        "diagnostic",
        b"CREATE TABLE t(a VARCHAR(1 e2)) STORAGE DISK",
        "Syntax",
        "Parse",
        [27, 29],
    )
}

fn case_0620() -> Case {
    bad(
        "syntax/limit-sign/2b31",
        "diagnostic",
        b"SELECT a FROM t LIMIT +1",
        "Syntax",
        "Parse",
        [22, 23],
    )
}

fn case_0621() -> Case {
    bad(
        "syntax/limit-sign/2d31",
        "diagnostic",
        b"SELECT a FROM t LIMIT -1",
        "Syntax",
        "Parse",
        [22, 23],
    )
}

fn case_0622() -> Case {
    bad(
        "syntax/limit-sign/2b2e35",
        "diagnostic",
        b"SELECT a FROM t LIMIT +.5",
        "Syntax",
        "Parse",
        [22, 23],
    )
}

fn case_0623() -> Case {
    bad(
        "syntax/limit-sign/2d2e35",
        "diagnostic",
        b"SELECT a FROM t LIMIT -.5",
        "Syntax",
        "Parse",
        [22, 23],
    )
}

fn case_0624() -> Case {
    bad(
        "syntax/limit-sign/2b3f",
        "diagnostic",
        b"SELECT a FROM t LIMIT +?",
        "Syntax",
        "Parse",
        [22, 23],
    )
}

fn case_0625() -> Case {
    bad(
        "syntax/limit-sign/2d4e554c4c",
        "diagnostic",
        b"SELECT a FROM t LIMIT -NULL",
        "Syntax",
        "Parse",
        [22, 23],
    )
}

fn case_0626() -> Case {
    bad(
        "syntax/limit-kind/54525545",
        "diagnostic",
        b"SELECT a FROM t LIMIT TRUE",
        "Syntax",
        "Parse",
        [22, 26],
    )
}

fn case_0627() -> Case {
    bad(
        "syntax/limit-kind/46414c5345",
        "diagnostic",
        b"SELECT a FROM t LIMIT FALSE",
        "Syntax",
        "Parse",
        [22, 27],
    )
}

fn case_0628() -> Case {
    bad(
        "syntax/limit-kind/4e554c4c",
        "diagnostic",
        b"SELECT a FROM t LIMIT NULL",
        "Syntax",
        "Parse",
        [22, 26],
    )
}

fn case_0629() -> Case {
    bad(
        "syntax/limit-kind/277827",
        "diagnostic",
        b"SELECT a FROM t LIMIT 'x'",
        "Syntax",
        "Parse",
        [22, 25],
    )
}

fn case_0630() -> Case {
    bad(
        "syntax/limit-kind/5827303027",
        "diagnostic",
        b"SELECT a FROM t LIMIT X'00'",
        "Syntax",
        "Parse",
        [22, 27],
    )
}

fn case_0631() -> Case {
    bad(
        "syntax/limit-kind/6e616d65",
        "diagnostic",
        b"SELECT a FROM t LIMIT name",
        "Syntax",
        "Parse",
        [22, 26],
    )
}

fn case_0632() -> Case {
    bad(
        "unsupported/limit/2e35",
        "diagnostic",
        b"SELECT a FROM t LIMIT .5",
        "Unsupported",
        "Parse",
        [22, 23],
    )
}

fn case_0633() -> Case {
    bad(
        "unsupported/limit/312e35",
        "diagnostic",
        b"SELECT a FROM t LIMIT 1.5",
        "Unsupported",
        "Parse",
        [23, 24],
    )
}

fn case_0634() -> Case {
    bad(
        "unsupported/limit/316532",
        "diagnostic",
        b"SELECT a FROM t LIMIT 1e2",
        "Unsupported",
        "Parse",
        [23, 25],
    )
}

fn case_0635() -> Case {
    bad(
        "unsupported/limit/31202b2032",
        "diagnostic",
        b"SELECT a FROM t LIMIT 1 + 2",
        "Unsupported",
        "Parse",
        [24, 25],
    )
}

fn case_0636() -> Case {
    bad(
        "unsupported/limit/3f2b31",
        "diagnostic",
        b"SELECT a FROM t LIMIT ?+1",
        "Unsupported",
        "Parse",
        [23, 24],
    )
}

fn case_0637() -> Case {
    bad(
        "unsupported/limit/31204f46465345542032",
        "diagnostic",
        b"SELECT a FROM t LIMIT 1 OFFSET 2",
        "Unsupported",
        "Parse",
        [24, 30],
    )
}

fn case_0638() -> Case {
    bad(
        "unsupported/limit/31204645544348",
        "diagnostic",
        b"SELECT a FROM t LIMIT 1 FETCH",
        "Unsupported",
        "Parse",
        [24, 29],
    )
}

fn case_0639() -> Case {
    bad(
        "unsupported/limit/312c32",
        "diagnostic",
        b"SELECT a FROM t LIMIT 1,2",
        "Unsupported",
        "Parse",
        [23, 24],
    )
}

fn case_0640() -> Case {
    bad(
        "syntax/limit-dot-alone",
        "diagnostic",
        b"SELECT a FROM t LIMIT .",
        "Syntax",
        "Parse",
        [22, 23],
    )
}

fn case_0641() -> Case {
    bad(
        "diagnostic/lex/53454c45435420612046524f4d20742057484552452061213d31",
        "diagnostic",
        b"SELECT a FROM t WHERE a!=1",
        "Unsupported",
        "Lex",
        [23, 25],
    )
}

fn case_0642() -> Case {
    bad(
        "diagnostic/lex/55504441544520742053455420613d3121",
        "diagnostic",
        b"UPDATE t SET a=1!",
        "Syntax",
        "Lex",
        [16, 17],
    )
}

fn case_0643() -> Case {
    bad(
        "diagnostic/lex/435245415445205441424c452074286120424947494e54292053544f52414745204449534b2f2a78",
        "diagnostic",
        b"CREATE TABLE t(a BIGINT) STORAGE DISK/*x",
        "Syntax",
        "Lex",
        [37, 40],
    )
}

fn case_0644() -> Case {
    bad(
        "diagnostic/lex/53454c45435420612046524f4d207420574845524520613d2778",
        "diagnostic",
        b"SELECT a FROM t WHERE a='x",
        "Syntax",
        "Lex",
        [24, 26],
    )
}

fn case_0645() -> Case {
    bad(
        "diagnostic/lex/44524f50205441424c45202274",
        "diagnostic",
        b"DROP TABLE \"t",
        "Syntax",
        "Lex",
        [11, 13],
    )
}

fn case_0646() -> Case {
    bad(
        "diagnostic/lex/53454c45435420612046524f4d2074ff",
        "diagnostic",
        b"SELECT a FROM t\xff",
        "Syntax",
        "Lex",
        [15, 16],
    )
}

fn case_0647() -> Case {
    bad(
        "diagnostic/name/53454c45435420/2222",
        "diagnostic",
        b"SELECT \"\" FROM t",
        "Name",
        "Name",
        [7, 9],
    )
}

fn case_0648() -> Case {
    bad(
        "diagnostic/name/53454c45435420/22610022",
        "diagnostic",
        b"SELECT \"a\x00\" FROM t",
        "Name",
        "Name",
        [7, 11],
    )
}

fn case_0649() -> Case {
    bad(
        "diagnostic/name/53454c45435420/22617f22",
        "diagnostic",
        b"SELECT \"a\x7f\" FROM t",
        "Name",
        "Name",
        [7, 11],
    )
}

fn case_0650() -> Case {
    bad(
        "diagnostic/name/55504441544520742053455420/2222",
        "diagnostic",
        b"UPDATE t SET \"\"=0",
        "Name",
        "Name",
        [13, 15],
    )
}

fn case_0651() -> Case {
    bad(
        "diagnostic/name/55504441544520742053455420/22610022",
        "diagnostic",
        b"UPDATE t SET \"a\x00\"=0",
        "Name",
        "Name",
        [13, 17],
    )
}

fn case_0652() -> Case {
    bad(
        "diagnostic/name/55504441544520742053455420/22617f22",
        "diagnostic",
        b"UPDATE t SET \"a\x7f\"=0",
        "Name",
        "Name",
        [13, 17],
    )
}

fn case_0653() -> Case {
    bad(
        "diagnostic/name/435245415445205441424c45207428/2222",
        "diagnostic",
        b"CREATE TABLE t(\"\" BIGINT) STORAGE DISK",
        "Name",
        "Name",
        [15, 17],
    )
}

fn case_0654() -> Case {
    bad(
        "diagnostic/name/435245415445205441424c45207428/22610022",
        "diagnostic",
        b"CREATE TABLE t(\"a\x00\" BIGINT) STORAGE DISK",
        "Name",
        "Name",
        [15, 19],
    )
}

fn case_0655() -> Case {
    bad(
        "diagnostic/name/435245415445205441424c45207428/22617f22",
        "diagnostic",
        b"CREATE TABLE t(\"a\x7f\" BIGINT) STORAGE DISK",
        "Name",
        "Name",
        [15, 19],
    )
}

fn case_0656() -> Case {
    bad(
        "diagnostic/name/44524f5020494e44455820/2222",
        "diagnostic",
        b"DROP INDEX \"\"",
        "Name",
        "Name",
        [11, 13],
    )
}

fn case_0657() -> Case {
    bad(
        "diagnostic/name/44524f5020494e44455820/22610022",
        "diagnostic",
        b"DROP INDEX \"a\x00\"",
        "Name",
        "Name",
        [11, 15],
    )
}

fn case_0658() -> Case {
    bad(
        "diagnostic/name/44524f5020494e44455820/22617f22",
        "diagnostic",
        b"DROP INDEX \"a\x7f\"",
        "Name",
        "Name",
        [11, 15],
    )
}

/// All statement roots, optional clauses, and explicit syntax payloads.
pub fn structural_cases() -> Vec<Case> {
    const CASES: [fn() -> Case; 70] = [
        case_0000, case_0001, case_0002, case_0003, case_0004, case_0005, case_0006, case_0007,
        case_0008, case_0009, case_0010, case_0011, case_0012, case_0013, case_0014, case_0015,
        case_0016, case_0017, case_0018, case_0019, case_0020, case_0021, case_0022, case_0023,
        case_0024, case_0025, case_0026, case_0027, case_0028, case_0029, case_0030, case_0031,
        case_0032, case_0033, case_0034, case_0035, case_0036, case_0037, case_0038, case_0039,
        case_0040, case_0041, case_0042, case_0043, case_0044, case_0045, case_0046, case_0047,
        case_0048, case_0049, case_0050, case_0051, case_0052, case_0053, case_0054, case_0055,
        case_0056, case_0057, case_0058, case_0059, case_0060, case_0061, case_0062, case_0063,
        case_0064, case_0065, case_0066, case_0067, case_0068, case_0069,
    ];
    CASES.iter().map(|make| make()).collect()
}

/// Raw scalar and DDL values whose semantic validation is deferred.
pub fn raw_payload_cases() -> Vec<Case> {
    const CASES: [fn() -> Case; 45] = [
        case_0070, case_0071, case_0072, case_0073, case_0074, case_0075, case_0076, case_0077,
        case_0078, case_0079, case_0080, case_0081, case_0082, case_0083, case_0084, case_0085,
        case_0086, case_0087, case_0088, case_0089, case_0090, case_0091, case_0092, case_0093,
        case_0094, case_0095, case_0096, case_0097, case_0098, case_0099, case_0100, case_0101,
        case_0102, case_0103, case_0104, case_0105, case_0106, case_0107, case_0108, case_0109,
        case_0110, case_0111, case_0112, case_0113, case_0114,
    ];
    CASES.iter().map(|make| make()).collect()
}

/// Contextual feature spelling and ordinary-name counterexamples.
pub fn contextual_cases() -> Vec<Case> {
    const CASES: [fn() -> Case; 95] = [
        case_0115, case_0116, case_0117, case_0118, case_0119, case_0120, case_0121, case_0122,
        case_0123, case_0124, case_0125, case_0126, case_0127, case_0128, case_0129, case_0130,
        case_0131, case_0132, case_0133, case_0134, case_0135, case_0136, case_0137, case_0138,
        case_0139, case_0140, case_0141, case_0142, case_0143, case_0144, case_0145, case_0146,
        case_0147, case_0148, case_0149, case_0150, case_0151, case_0152, case_0153, case_0154,
        case_0155, case_0156, case_0157, case_0158, case_0159, case_0160, case_0161, case_0162,
        case_0163, case_0164, case_0165, case_0166, case_0167, case_0168, case_0169, case_0170,
        case_0171, case_0172, case_0173, case_0174, case_0175, case_0176, case_0177, case_0178,
        case_0179, case_0180, case_0181, case_0182, case_0183, case_0184, case_0185, case_0186,
        case_0187, case_0188, case_0189, case_0190, case_0191, case_0192, case_0193, case_0194,
        case_0195, case_0196, case_0197, case_0198, case_0199, case_0200, case_0201, case_0202,
        case_0203, case_0204, case_0205, case_0206, case_0207, case_0208, case_0209,
    ];
    CASES.iter().map(|make| make()).collect()
}

/// Exact malformed, unsupported, lexical, and name diagnostics.
pub fn diagnostic_cases() -> Vec<Case> {
    const CASES: [fn() -> Case; 444] = [
        case_0210, case_0211, case_0212, case_0213, case_0214, case_0215, case_0221, case_0222,
        case_0223, case_0224, case_0225, case_0226, case_0227, case_0228, case_0229, case_0230,
        case_0231, case_0232, case_0233, case_0234, case_0235, case_0236, case_0237, case_0238,
        case_0239, case_0240, case_0241, case_0242, case_0243, case_0244, case_0245, case_0246,
        case_0247, case_0248, case_0249, case_0250, case_0251, case_0252, case_0253, case_0254,
        case_0255, case_0256, case_0257, case_0258, case_0259, case_0260, case_0261, case_0262,
        case_0263, case_0264, case_0265, case_0266, case_0267, case_0268, case_0269, case_0270,
        case_0271, case_0272, case_0273, case_0274, case_0275, case_0276, case_0277, case_0278,
        case_0279, case_0280, case_0281, case_0282, case_0283, case_0284, case_0285, case_0286,
        case_0287, case_0288, case_0289, case_0290, case_0291, case_0292, case_0293, case_0294,
        case_0295, case_0296, case_0297, case_0298, case_0299, case_0300, case_0301, case_0302,
        case_0303, case_0304, case_0305, case_0306, case_0307, case_0308, case_0309, case_0310,
        case_0311, case_0312, case_0313, case_0314, case_0315, case_0316, case_0317, case_0318,
        case_0319, case_0320, case_0321, case_0322, case_0323, case_0324, case_0325, case_0326,
        case_0327, case_0328, case_0329, case_0330, case_0331, case_0332, case_0333, case_0334,
        case_0335, case_0336, case_0337, case_0338, case_0339, case_0340, case_0341, case_0342,
        case_0343, case_0344, case_0345, case_0346, case_0347, case_0348, case_0349, case_0350,
        case_0351, case_0352, case_0353, case_0354, case_0355, case_0356, case_0357, case_0358,
        case_0359, case_0360, case_0361, case_0362, case_0363, case_0364, case_0365, case_0366,
        case_0367, case_0368, case_0369, case_0370, case_0371, case_0372, case_0373, case_0374,
        case_0375, case_0376, case_0377, case_0378, case_0379, case_0380, case_0381, case_0382,
        case_0383, case_0384, case_0385, case_0386, case_0387, case_0388, case_0389, case_0390,
        case_0391, case_0392, case_0393, case_0394, case_0395, case_0396, case_0397, case_0398,
        case_0399, case_0400, case_0401, case_0402, case_0403, case_0404, case_0405, case_0406,
        case_0407, case_0408, case_0409, case_0410, case_0411, case_0412, case_0413, case_0414,
        case_0415, case_0416, case_0417, case_0418, case_0419, case_0420, case_0421, case_0422,
        case_0423, case_0424, case_0425, case_0426, case_0427, case_0428, case_0429, case_0430,
        case_0431, case_0432, case_0433, case_0434, case_0435, case_0436, case_0437, case_0438,
        case_0439, case_0440, case_0441, case_0442, case_0443, case_0444, case_0445, case_0446,
        case_0447, case_0448, case_0449, case_0450, case_0451, case_0452, case_0453, case_0454,
        case_0455, case_0456, case_0457, case_0458, case_0459, case_0460, case_0461, case_0462,
        case_0463, case_0464, case_0465, case_0466, case_0467, case_0468, case_0469, case_0470,
        case_0471, case_0472, case_0473, case_0474, case_0475, case_0476, case_0477, case_0478,
        case_0479, case_0480, case_0481, case_0482, case_0483, case_0484, case_0485, case_0486,
        case_0487, case_0488, case_0489, case_0490, case_0491, case_0492, case_0493, case_0494,
        case_0495, case_0496, case_0497, case_0498, case_0499, case_0500, case_0501, case_0502,
        case_0503, case_0504, case_0505, case_0506, case_0507, case_0508, case_0509, case_0510,
        case_0511, case_0512, case_0513, case_0514, case_0515, case_0516, case_0517, case_0518,
        case_0519, case_0520, case_0521, case_0522, case_0523, case_0524, case_0525, case_0526,
        case_0527, case_0528, case_0529, case_0530, case_0531, case_0532, case_0533, case_0534,
        case_0535, case_0536, case_0537, case_0538, case_0539, case_0540, case_0541, case_0542,
        case_0543, case_0544, case_0545, case_0546, case_0547, case_0548, case_0549, case_0550,
        case_0551, case_0552, case_0553, case_0554, case_0555, case_0556, case_0557, case_0558,
        case_0559, case_0560, case_0561, case_0562, case_0563, case_0564, case_0565, case_0566,
        case_0567, case_0568, case_0569, case_0570, case_0571, case_0572, case_0573, case_0574,
        case_0575, case_0576, case_0577, case_0578, case_0579, case_0580, case_0581, case_0582,
        case_0583, case_0584, case_0585, case_0586, case_0587, case_0588, case_0589, case_0590,
        case_0591, case_0592, case_0593, case_0594, case_0595, case_0596, case_0597, case_0598,
        case_0599, case_0600, case_0601, case_0602, case_0603, case_0604, case_0605, case_0606,
        case_0607, case_0608, case_0609, case_0610, case_0611, case_0612, case_0613, case_0614,
        case_0615, case_0616, case_0617, case_0618, case_0619, case_0620, case_0621, case_0622,
        case_0623, case_0624, case_0625, case_0626, case_0627, case_0628, case_0629, case_0630,
        case_0631, case_0632, case_0633, case_0634, case_0635, case_0636, case_0637, case_0638,
        case_0639, case_0640, case_0641, case_0642, case_0643, case_0644, case_0645, case_0646,
        case_0647, case_0648, case_0649, case_0650, case_0651, case_0652, case_0653, case_0654,
        case_0655, case_0656, case_0657, case_0658,
    ];
    CASES.iter().map(|make| make()).collect()
}

/// Only the five original P2 inputs whose dispatch changes in P4.
pub fn changed_dispatch_cases() -> Vec<Case> {
    const CASES: [fn() -> Case; 5] = [case_0216, case_0217, case_0218, case_0219, case_0220];
    CASES.iter().map(|make| make()).collect()
}

pub(super) fn generated_templates() -> Vec<Case> {
    const CASES: [fn() -> Case; 36] = [
        case_0000, case_0001, case_0002, case_0003, case_0004, case_0005, case_0006, case_0007,
        case_0008, case_0009, case_0010, case_0011, case_0012, case_0013, case_0014, case_0015,
        case_0027, case_0028, case_0030, case_0031, case_0040, case_0053, case_0054, case_0055,
        case_0056, case_0058, case_0062, case_0066, case_0068, case_0077, case_0082, case_0074,
        case_0071, case_0096, case_0099, case_0089,
    ];
    CASES.iter().map(|make| make()).collect()
}
