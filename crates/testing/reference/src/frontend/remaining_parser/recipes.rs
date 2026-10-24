//! Explicit tree printing and relocation; no SQL recognition or production calls.

use super::{
    Case, DataType, Kind, NameObservation, Node, Nullability, ScalarObservation, bad, good, node,
};

struct Assembly {
    source: Vec<u8>,
    nodes: Vec<Node>,
    markers: u16,
}

impl Assembly {
    fn new() -> Self {
        Self {
            source: Vec::new(),
            nodes: Vec::new(),
            markers: 0,
        }
    }
    fn text(&mut self, value: &[u8]) {
        self.source.extend_from_slice(value);
    }
    fn begin(&mut self, kind: Kind) -> usize {
        let index = self.nodes.len();
        let at = self.source.len();
        self.nodes.push(node(kind, [at, at], None, None));
        index
    }
    fn end(&mut self, index: usize, children: &[usize]) -> usize {
        self.nodes[index].span[1] = self.source.len();
        self.nodes[index].first_child = children.first().copied();
        for pair in children.windows(2) {
            self.nodes[pair[0]].next_sibling = Some(pair[1]);
        }
        index
    }
    fn name(&mut self, raw: &[u8], normalized: &[u8], quoted: bool) -> usize {
        let start = self.source.len();
        self.text(raw);
        let span = [start, self.source.len()];
        let index = self.nodes.len();
        self.nodes.push(node(
            Kind::Name(NameObservation {
                bytes: normalized.to_vec(),
                quoted,
                span,
            }),
            span,
            None,
            None,
        ));
        index
    }
    fn a(&mut self) -> usize {
        self.name(b"a", b"A", false)
    }
    fn target(&mut self, qualified: bool) -> usize {
        let index = self.begin(Kind::Target);
        let mut children = Vec::new();
        if qualified {
            children.push(self.name(b"public", b"PUBLIC", false));
            self.text(b".");
        }
        children.push(self.name(b"t", b"T", false));
        self.end(index, &children)
    }
    fn scalar(&mut self, marker: bool) -> usize {
        let start = self.source.len();
        let kind = if marker {
            self.markers += 1;
            self.text(b"?");
            ScalarObservation::Parameter {
                ordinal: self.markers,
            }
        } else {
            self.text(b"0");
            ScalarObservation::Integer {
                sign: "None",
                digits: [start, start + 1],
            }
        };
        let index = self.nodes.len();
        self.nodes.push(node(
            Kind::Scalar(kind),
            [start, self.source.len()],
            None,
            None,
        ));
        index
    }
    fn compare(&mut self, marker: bool) -> usize {
        let index = self.begin(Kind::Compare(super::Comparison::Eq));
        let left = self.a();
        self.text(b"=");
        let right = self.scalar(marker);
        self.end(index, &[left, right])
    }
    fn nested_compare(&mut self, depth: usize, mode: usize) -> usize {
        let mut frames = Vec::new();
        for level in 0..depth {
            let is_group = mode == 1 || (mode == 2 && level % 2 == 1);
            if is_group {
                let group = self.begin(Kind::Group);
                self.text(b"(");
                let or = self.begin(Kind::Or);
                let and = self.begin(Kind::And);
                frames.push((group, Some((or, and))));
            } else {
                let not = self.begin(Kind::Not);
                self.text(b"NOT ");
                frames.push((not, None));
            }
        }
        let mut child = self.compare(false);
        for (outer, branch) in frames.into_iter().rev() {
            if let Some((or, and)) = branch {
                self.end(and, &[child]);
                self.end(or, &[and]);
                self.text(b")");
                child = self.end(outer, &[or]);
            } else {
                child = self.end(outer, &[child]);
            }
        }
        child
    }
    fn root(
        &mut self,
        statement: usize,
        assignments: usize,
        qualified: bool,
    ) -> (usize, Vec<usize>) {
        let root = self.begin(match statement {
            0 => Kind::Select,
            1 => Kind::Update,
            _ => Kind::Delete,
        });
        let mut children = Vec::new();
        if statement == 0 {
            self.text(b"SELECT ");
            let projection = self.begin(Kind::Projection);
            let star = self.begin(Kind::Wildcard);
            self.text(b"*");
            self.end(star, &[]);
            children.push(self.end(projection, &[star]));
            self.text(b" FROM ");
        } else {
            self.text(if statement == 1 {
                b"UPDATE "
            } else {
                b"DELETE FROM "
            });
        }
        children.push(self.target(qualified));
        if statement == 1 {
            self.text(b" ");
            let sets = self.begin(Kind::Assignments);
            self.text(b"SET ");
            let mut items = Vec::new();
            for index in 0..assignments {
                if index != 0 {
                    self.text(b",");
                }
                let assignment = self.begin(Kind::Assignment);
                let name = self.a();
                self.text(b"=");
                let scalar = self.scalar(false);
                items.push(self.end(assignment, &[name, scalar]));
            }
            children.push(self.end(sets, &items));
        }
        (root, children)
    }
    fn where_start(&mut self) -> (usize, usize, usize) {
        self.text(b" ");
        let clause = self.begin(Kind::Where);
        self.text(b"WHERE ");
        let or = self.begin(Kind::Or);
        let and = self.begin(Kind::And);
        (clause, or, and)
    }
    fn where_end(&mut self, ids: (usize, usize, usize), atoms: &[usize]) -> usize {
        self.end(ids.2, atoms);
        self.end(ids.1, &[ids.2]);
        self.end(ids.0, &[ids.1])
    }
    fn finish(self, id: String) -> Case {
        good(id, "boundary", &self.source, self.nodes, self.markers)
    }
}

fn from_nodes_limit(case: Case) -> Case {
    assert_eq!(case.expected.nodes.len(), 8193);
    let at = case.expected.nodes[8192].span[0];
    bad(
        case.id,
        "boundary",
        &case.source,
        "Limit",
        "Parse",
        [at, at],
    )
}

fn list_case(order: bool, count: usize) -> Case {
    let mut a = Assembly::new();
    let root = a.begin(Kind::Select);
    a.text(b"SELECT ");
    let projection = a.begin(Kind::Projection);
    let mut children = Vec::new();
    let mut items = Vec::new();
    let mut last_start = 0;
    if order {
        let star = a.begin(Kind::Wildcard);
        a.text(b"*");
        a.end(star, &[]);
        children.push(a.end(projection, &[star]));
        a.text(b" FROM ");
        children.push(a.target(false));
        a.text(b" ");
        let clause = a.begin(Kind::OrderBy);
        a.text(b"ORDER BY ");
        for i in 0..count {
            if i != 0 {
                a.text(b",");
            }
            last_start = a.source.len();
            let term = a.begin(Kind::OrderTerm {
                direction: None,
                nulls: None,
            });
            let name = a.a();
            items.push(a.end(term, &[name]));
        }
        children.push(a.end(clause, &items));
    } else {
        for i in 0..count {
            if i != 0 {
                a.text(b",");
            }
            last_start = a.source.len();
            let item = a.begin(Kind::ProjectionItem);
            let name = a.a();
            items.push(a.end(item, &[name]));
        }
        children.push(a.end(projection, &items));
        a.text(b" FROM ");
        children.push(a.target(false));
    }
    a.end(root, &children);
    let case = a.finish(format!("boundary/list/{order}/{count}"));
    if count <= 32 {
        case
    } else {
        bad(
            case.id,
            "boundary",
            &case.source,
            "Limit",
            "Parse",
            [last_start, last_start],
        )
    }
}

fn map_ranges(case: &mut Case, map: impl Fn([usize; 2]) -> [usize; 2]) {
    for node in &mut case.expected.nodes {
        node.span = map(node.span);
        match &mut node.kind {
            Kind::Name(name) => name.span = map(name.span),
            Kind::Scalar(ScalarObservation::Integer { digits, .. }) => *digits = map(*digits),
            Kind::DataType(DataType::Utf8(length) | DataType::Binary(length)) => {
                length.span = map(length.span);
                length.digits = map(length.digits);
            }
            _ => {}
        }
    }
}

fn replace_name(
    mut case: Case,
    ordinal: usize,
    raw: &[u8],
    normalized: &[u8],
    quoted: bool,
    id: String,
) -> Case {
    let index = case
        .expected
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| matches!(n.kind, Kind::Name(_)))
        .nth(ordinal)
        .expect("name literal ordinal")
        .0;
    let span = case.expected.nodes[index].span;
    let extra = raw.len() - (span[1] - span[0]);
    case.source.splice(span[0]..span[1], raw.iter().copied());
    map_ranges(&mut case, |range| {
        [
            range[0] + usize::from(range[0] >= span[1]) * extra,
            range[1] + usize::from(range[1] >= span[1]) * extra,
        ]
    });
    let new_span = [span[0], span[0] + raw.len()];
    case.expected.nodes[index].span = new_span;
    case.expected.nodes[index].kind = Kind::Name(NameObservation {
        bytes: normalized.to_vec(),
        quoted,
        span: new_span,
    });
    case.expected.source = case.source.clone();
    case.id = id;
    case.family = "boundary";
    if normalized.len() <= 128 {
        case
    } else {
        bad(case.id, "boundary", &case.source, "Name", "Name", new_span)
    }
}

/// Explicit boundary fragments for the new statement consumers and WHERE links.
pub fn boundary_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for order in [false, true] {
        for count in [31, 32, 33] {
            cases.push(list_case(order, count));
        }
        let mut prefix = if order {
            b"SELECT * FROM t ORDER BY ".to_vec()
        } else {
            b"SELECT ".to_vec()
        };
        for index in 0..32 {
            if index != 0 {
                prefix.push(b',');
            }
            prefix.push(b'a');
        }
        prefix.push(b',');
        for (label, token, code, zero_width) in [
            ("true", b"TRUE".as_slice(), "Unsupported", false),
            ("digits", b"1", "Unsupported", false),
            ("paren", b"(", "Unsupported", false),
            ("star", b"*", "Unsupported", false),
            ("keyword", b"FROM", "Syntax", false),
            ("eof", b"", "Syntax", false),
        ] {
            let mut source = prefix.clone();
            source.extend_from_slice(token);
            let at = prefix.len();
            let end = if zero_width { at } else { at + token.len() };
            cases.push(bad(
                format!("boundary/list-kind/{order}/{label}"),
                "boundary",
                &source,
                code,
                "Parse",
                [at, end],
            ));
        }
        for (label, token) in [
            ("long-name", vec![b'a'; 129]),
            ("empty-name", b"\"\"".to_vec()),
        ] {
            let mut source = prefix.clone();
            source.extend_from_slice(&token);
            let at = prefix.len();
            cases.push(bad(
                format!("boundary/list-kind/{order}/{label}"),
                "boundary",
                &source,
                "Limit",
                "Parse",
                [at, at],
            ));
        }
        let mut source = prefix.clone();
        source.push(b'!');
        let at = prefix.len();
        cases.push(bad(
            format!("boundary/list-kind/{order}/lex"),
            "boundary",
            &source,
            "Syntax",
            "Lex",
            [at, at + 1],
        ));
    }
    let shapes = super::literals::structural_cases();
    for (context, identity, name_index) in [
        ("projection", "select/options/0/0", 0),
        ("alias", "select/order/None/None", 1),
        ("order", "select/order/None/None", 3),
        ("assignment", "update/minimum", 1),
        ("delete-target", "delete/minimum", 0),
        ("definition", "ddl/type-null/Int64/Implicit", 1),
        ("index-target", "index/targets/false/false/false", 0),
        ("drop-target", "drop/true/false", 0),
    ] {
        // Python-authored booleans in these identities use their literal casing.
        let identity = identity.replace("false", "False").replace("true", "True");
        let original = shapes
            .iter()
            .find(|c| c.id == identity)
            .expect("fixed name boundary control");
        for length in [127, 128, 129] {
            cases.push(replace_name(
                original.clone(),
                name_index,
                &vec![b'a'; length],
                &vec![b'A'; length],
                false,
                format!("boundary/name/{context}/ascii/{length}"),
            ));
            let mut normalized = "é".as_bytes().repeat(length / 2);
            if length % 2 == 1 {
                normalized.push(b'a');
            }
            let mut raw = vec![b'"'];
            raw.extend_from_slice(&normalized);
            raw.push(b'"');
            cases.push(replace_name(
                original.clone(),
                name_index,
                &raw,
                &normalized,
                true,
                format!("boundary/name/{context}/utf8/{length}"),
            ));
        }
    }
    let source_control = shapes
        .iter()
        .find(|c| c.id == "drop/False/False")
        .expect("drop source control");
    for length in [1_048_575, 1_048_576, 1_048_577] {
        let mut case = source_control.clone();
        case.id = format!("boundary/source/{length}");
        case.family = "boundary";
        case.source.resize(length, b' ');
        case.expected.source = case.source.clone();
        cases.push(if length <= 1_048_576 {
            case
        } else {
            bad(
                case.id,
                "boundary",
                &case.source,
                "Limit",
                "Lex",
                [1_048_576, length],
            )
        });
    }
    for statement in 0..3 {
        for mode in 0..3 {
            for depth in [63, 64, 65] {
                let mut a = Assembly::new();
                let (root, mut children) = a.root(statement, 1, false);
                let clause = a.where_start();
                let first = a.source.len();
                let atom = a.nested_compare(depth, mode);
                children.push(a.where_end(clause, &[atom]));
                a.end(root, &children);
                let case = a.finish(format!("boundary/depth/{statement}/{mode}/{depth}"));
                let opener = first
                    + if mode == 0 {
                        64 * 4
                    } else if mode == 1 {
                        64
                    } else {
                        32 * 5
                    };
                cases.push(if depth <= 64 {
                    case
                } else {
                    bad(
                        case.id,
                        "boundary",
                        &case.source,
                        "Limit",
                        "Parse",
                        [opener, opener],
                    )
                });
            }
        }
        let mut a = Assembly::new();
        let (root, mut children) = a.root(statement, 1, false);
        let clause = a.where_start();
        let first = a.nested_compare(64, 0);
        a.text(b" AND ");
        let second = a.nested_compare(64, 1);
        children.push(a.where_end(clause, &[first, second]));
        a.end(root, &children);
        cases.push(a.finish(format!("boundary/depth-sibling/{statement}")));
    }
    for statement in 0..3 {
        for count in [255, 256, 257] {
            let mut a = Assembly::new();
            let (root, mut children) = a.root(statement, 1, false);
            if statement == 1 {
                // Replace the sole known SET scalar before adding WHERE markers.
                let scalar = a.nodes.last_mut().expect("SET scalar");
                scalar.kind = Kind::Scalar(ScalarObservation::Parameter { ordinal: 1 });
                *a.source.last_mut().expect("SET digit") = b'?';
                a.markers = 1;
            }
            let clause = a.where_start();
            let mut atoms = Vec::new();
            let tail = usize::from(statement == 0);
            for index in a.markers as usize..count - tail {
                if !atoms.is_empty() {
                    a.text(b" AND ");
                }
                let _ = index;
                atoms.push(a.compare(true));
            }
            children.push(a.where_end(clause, &atoms));
            if statement == 0 {
                a.text(b" ");
                let limit = a.begin(Kind::Limit);
                a.text(b"LIMIT ");
                let marker = a.scalar(true);
                children.push(a.end(limit, &[marker]));
            }
            a.end(root, &children);
            let case = a.finish(format!("boundary/markers/{statement}/{count}"));
            let at = case
                .expected
                .nodes
                .iter()
                .find(|n| {
                    matches!(
                        n.kind,
                        Kind::Scalar(ScalarObservation::Parameter { ordinal: 257 })
                    )
                })
                .map(|n| n.span[0]);
            cases.push(if let Some(at) = at {
                bad(
                    case.id,
                    "boundary",
                    &case.source,
                    "Limit",
                    "Parse",
                    [at, at],
                )
            } else {
                case
            });
        }
    }
    for extra in 1..=3 {
        let mut a = Assembly::new();
        let (root, mut children) = a.root(2, 0, false);
        let clause = a.where_start();
        let mut atoms = Vec::new();
        atoms.push(a.nested_compare(extra, 0));
        for _ in 1..2728 {
            a.text(b" AND ");
            atoms.push(a.compare(false));
        }
        children.push(a.where_end(clause, &atoms));
        a.end(root, &children);
        let case = a.finish(format!("boundary/nodes/delete/{}", 8190 + extra));
        assert_eq!(case.expected.nodes.len(), 8190 + extra);
        cases.push(if extra == 3 {
            from_nodes_limit(case)
        } else {
            case
        });
    }
    for count in [8191, 8192, 8193] {
        let mut a = Assembly::new();
        let (root, mut children) =
            a.root(1, if count == 8193 { 2728 } else { 2729 }, count == 8192);
        if count == 8193 {
            let clause = a.where_start();
            let atom = a.begin(Kind::IsNull { negated: false });
            let value = a.a();
            a.text(b" IS NULL");
            a.end(atom, &[value]);
            children.push(a.where_end(clause, &[atom]));
        }
        a.end(root, &children);
        let case = a.finish(format!("boundary/nodes/update/{count}"));
        assert_eq!(case.expected.nodes.len(), count);
        cases.push(if count == 8193 {
            from_nodes_limit(case)
        } else {
            case
        });
        let mut a = Assembly::new();
        let root = a.begin(Kind::CreateTable);
        a.text(b"CREATE TABLE ");
        let target = a.target(false);
        let table = a.begin(Kind::TableDefinition);
        a.text(b"(");
        let mut definitions = Vec::new();
        for index in 0..2727 {
            if index != 0 {
                a.text(b",");
            }
            let definition = a.begin(Kind::ColumnDefinition {
                nullability: Nullability::Implicit,
            });
            let name = a.a();
            a.text(b" ");
            let kind = a.begin(Kind::DataType(DataType::Int64));
            a.text(b"BIGINT");
            a.end(kind, &[]);
            definitions.push(a.end(definition, &[name, kind]));
        }
        a.text(b",");
        let pk = a.begin(Kind::PrimaryKey);
        a.text(b"PRIMARY KEY ");
        let columns = a.begin(Kind::Columns);
        a.text(b"(");
        let mut keys = Vec::new();
        for index in 0..count - 8188 {
            if index != 0 {
                a.text(b",");
            }
            keys.push(a.a());
        }
        a.text(b")");
        a.end(columns, &keys);
        definitions.push(a.end(pk, &[columns]));
        a.text(b")");
        a.end(table, &definitions);
        a.text(b" ");
        let storage = a.begin(Kind::StorageDisk);
        a.text(b"STORAGE DISK");
        a.end(storage, &[]);
        a.end(root, &[target, table, storage]);
        let case = a.finish(format!("boundary/nodes/create-table/{count}"));
        assert_eq!(case.expected.nodes.len(), count);
        cases.push(if count == 8193 {
            from_nodes_limit(case)
        } else {
            case
        });
    }
    for binary in [false, true] {
        for length in [65_535, 65_536, 65_537] {
            let mut a = Assembly::new();
            let (root, children) = a.root(1, 0, false);
            let sets = *children.last().expect("SET wrapper");
            let assignment = a.begin(Kind::Assignment);
            let name = a.a();
            a.text(b"=");
            let scalar = a.begin(Kind::Scalar(if binary {
                ScalarObservation::Binary
            } else {
                ScalarObservation::Text
            }));
            a.text(if binary { b"X'" } else { b"'" });
            a.source.extend(std::iter::repeat_n(
                b'a',
                length * if binary { 2 } else { 1 },
            ));
            a.text(b"'");
            a.end(scalar, &[]);
            a.end(assignment, &[name, scalar]);
            a.end(sets, &[assignment]);
            a.end(root, &children);
            cases.push(a.finish(format!("boundary/raw-payload/{binary}/{length}")));
        }
    }
    cases
}

/// One parse per operation, using frozen trees and trivia at known node starts.
///
/// All payloads and grammar choices come from the literal template. Only byte
/// coordinates move; no lexer, parser, name normalizer, or evaluator is called.
pub fn generated_cases(seed: u64, operations: usize) -> Vec<Case> {
    let templates = super::literals::generated_templates();
    let seed_index = (seed % 1_000_000) as usize;
    let mut cases = Vec::with_capacity(operations);
    for operation in 0..operations {
        let mut case = templates[(seed_index * 17 + operation) % templates.len()].clone();
        let chosen = (seed_index * 31 + operation * 7) % case.expected.nodes.len();
        let cut = case.expected.nodes[chosen].span[0];
        let mut insertions = vec![
            (
                0,
                format!("/*seed={seed};operation={operation};?*/ ").into_bytes(),
            ),
            (
                cut,
                if operation % 2 == 0 {
                    b"/*gap?;*/ ".to_vec()
                } else {
                    b"--gap?;\n".to_vec()
                },
            ),
            (case.source.len(), b" /*tail?;*/\n".to_vec()),
        ];
        insertions.sort_by_key(|entry| entry.0);
        let mut source = Vec::new();
        let mut previous = 0;
        for (at, bytes) in &insertions {
            source.extend_from_slice(&case.source[previous..*at]);
            source.extend_from_slice(bytes);
            previous = *at;
        }
        source.extend_from_slice(&case.source[previous..]);
        map_ranges(&mut case, |range| {
            [
                range[0]
                    + insertions
                        .iter()
                        .filter(|(at, _)| *at <= range[0])
                        .map(|(_, bytes)| bytes.len())
                        .sum::<usize>(),
                range[1]
                    + insertions
                        .iter()
                        .filter(|(at, _)| *at < range[1])
                        .map(|(_, bytes)| bytes.len())
                        .sum::<usize>(),
            ]
        });
        case.id = format!("generated/{seed}/{operation}");
        case.family = "generated";
        case.source = source.clone();
        case.expected.source = source;
        cases.push(case);
    }
    cases
}
