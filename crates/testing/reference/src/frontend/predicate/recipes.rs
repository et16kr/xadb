//! Source assembly from known fragments; this module never parses completed SQL.

use super::{Case, Comparison, Kind, Node, ScalarObservation, bad, good, name, node};

#[derive(Clone)]
pub(super) struct Value {
    raw: Vec<u8>,
    kind: Kind,
}

impl Value {
    pub(super) fn name(raw: &[u8], normalized: &[u8], quoted: bool) -> Self {
        Self {
            raw: raw.to_vec(),
            kind: name(normalized, quoted, [0, raw.len()], None).kind,
        }
    }

    pub(super) fn scalar(raw: &[u8], scalar: ScalarObservation) -> Self {
        Self {
            raw: raw.to_vec(),
            kind: Kind::Scalar(scalar),
        }
    }

    pub(super) fn integer(raw: &[u8], sign: &'static str, digits: [usize; 2]) -> Self {
        Self::scalar(raw, ScalarObservation::Integer { sign, digits })
    }

    pub(super) fn zero() -> Self {
        Self::integer(b"0", "None", [0, 1])
    }

    pub(super) fn marker() -> Self {
        Self::scalar(b"?", ScalarObservation::Parameter { ordinal: 1 })
    }
}

#[derive(Clone)]
pub(super) enum Expr {
    Compare(Value, Comparison, &'static [u8], Value),
    IsNull(Value, bool, &'static [u8]),
    Not(&'static [u8], Box<Expr>),
    Group(&'static [u8], Vec<Vec<Expr>>, &'static [u8]),
}

pub(super) fn eq(left: Value, right: Value) -> Expr {
    Expr::Compare(left, Comparison::Eq, b"=", right)
}

pub(super) fn atom() -> Expr {
    eq(Value::zero(), Value::zero())
}

pub(super) fn named_eq(raw: &[u8], normalized: &[u8], digit: u8) -> Expr {
    eq(
        Value::name(raw, normalized, false),
        Value::integer(&[digit], "None", [0, 1]),
    )
}

pub(super) fn not(child: Expr) -> Expr {
    Expr::Not(b"NOT ", Box::new(child))
}

pub(super) fn group(terms: Vec<Vec<Expr>>) -> Expr {
    Expr::Group(b"(", terms, b")")
}

// This is a printer for an already chosen expected tree, not a SQL recognizer.
// Both sibling indices and spans are obtained from fragment lengths and the
// explicit list shape; no production syntax or normalization helper is used.
struct Assembly {
    source: Vec<u8>,
    nodes: Vec<Node>,
    markers: u16,
}

impl Assembly {
    fn reserve_node(&mut self, kind: Kind) -> usize {
        let index = self.nodes.len();
        let at = self.source.len();
        self.nodes.push(node(kind, [at, at], None, None));
        index
    }

    fn children(&mut self, parent: usize, indices: &[usize]) {
        self.nodes[parent].first_child = indices.first().copied();
        for pair in indices.windows(2) {
            self.nodes[pair[0]].next_sibling = Some(pair[1]);
        }
        self.nodes[parent].span[1] = self.source.len();
    }

    fn value(&mut self, value: &Value) -> usize {
        let start = self.source.len();
        self.source.extend_from_slice(&value.raw);
        let end = self.source.len();
        let mut kind = value.kind.clone();
        match &mut kind {
            Kind::Name(value) => value.span = [start, end],
            Kind::Scalar(ScalarObservation::Integer { digits, .. }) => {
                digits[0] += start;
                digits[1] += start;
            }
            Kind::Scalar(ScalarObservation::Parameter { ordinal }) => {
                self.markers += 1;
                *ordinal = self.markers;
            }
            _ => {}
        }
        let index = self.nodes.len();
        self.nodes.push(node(kind, [start, end], None, None));
        index
    }

    fn expr(&mut self, expr: &Expr) -> usize {
        match expr {
            Expr::Compare(left, op, separator, right) => {
                let index = self.reserve_node(Kind::Compare(*op));
                let left = self.value(left);
                self.source.extend_from_slice(separator);
                let right = self.value(right);
                self.children(index, &[left, right]);
                index
            }
            Expr::IsNull(value, negated, suffix) => {
                let index = self.reserve_node(Kind::IsNull { negated: *negated });
                let child = self.value(value);
                self.source.extend_from_slice(suffix);
                self.children(index, &[child]);
                index
            }
            Expr::Not(prefix, child) => {
                let index = self.reserve_node(Kind::Not);
                self.source.extend_from_slice(prefix);
                let child = self.expr(child);
                self.children(index, &[child]);
                index
            }
            Expr::Group(prefix, terms, suffix) => {
                let index = self.reserve_node(Kind::Group);
                self.source.extend_from_slice(prefix);
                let child = self.disjunction(terms);
                self.source.extend_from_slice(suffix);
                self.children(index, &[child]);
                index
            }
        }
    }

    fn disjunction(&mut self, terms: &[Vec<Expr>]) -> usize {
        let index = self.reserve_node(Kind::Or);
        let mut children = Vec::new();
        for (or_index, term) in terms.iter().enumerate() {
            if or_index != 0 {
                self.source.extend_from_slice(b" OR ");
            }
            let conjunction = self.reserve_node(Kind::And);
            let mut values = Vec::new();
            for (and_index, expr) in term.iter().enumerate() {
                if and_index != 0 {
                    self.source.extend_from_slice(b" AND ");
                }
                values.push(self.expr(expr));
            }
            self.children(conjunction, &values);
            children.push(conjunction);
        }
        self.children(index, &children);
        index
    }
}

pub(super) fn assemble(
    id: impl Into<String>,
    terms: Vec<Vec<Expr>>,
    prefix: &[u8],
    suffix: &[u8],
) -> Case {
    let mut output = Assembly {
        source: prefix.to_vec(),
        nodes: Vec::new(),
        markers: 0,
    };
    let root = output.reserve_node(Kind::Predicate);
    let child = output.disjunction(&terms);
    output.children(root, &[child]);
    output.source.extend_from_slice(suffix);
    good(id, &output.source, output.nodes, output.markers)
}

pub(super) fn one(id: impl Into<String>, expr: Expr) -> Case {
    assemble(id, vec![vec![expr]], b"", b"")
}

fn nested(mut child: Expr, nots: usize, groups: usize) -> Expr {
    for _ in 0..groups {
        child = group(vec![vec![child]]);
    }
    for _ in 0..nots {
        child = not(child);
    }
    child
}

/// Independent source/depth/node/marker/name boundary recipes for this new entry.
pub fn boundary_cases() -> Vec<Case> {
    let mut out = Vec::new();
    for depth in [63, 64] {
        out.push(one(format!("depth-not-{depth}"), nested(atom(), depth, 0)));
        out.push(one(
            format!("depth-group-{depth}"),
            nested(atom(), 0, depth),
        ));
    }
    let mut source = b"NOT ".repeat(65);
    source.extend_from_slice(b"0=0");
    out.push(bad("depth-not-65", &source, "Limit", "Parse", [256, 256]));
    let mut source = vec![b'('; 65];
    source.extend_from_slice(b"0=0");
    source.extend_from_slice(&[b')'; 65]);
    out.push(bad("depth-group-65", &source, "Limit", "Parse", [64, 64]));
    for depth in [63usize, 64] {
        let mut expr = atom();
        for _ in 0..31 {
            expr = not(group(vec![vec![expr]]));
        }
        expr = not(expr);
        if depth == 64 {
            expr = group(vec![vec![expr]]);
        }
        out.push(one(format!("depth-mixed-{depth}"), expr));
    }
    let mut source = b"NOT (".repeat(32);
    source.extend_from_slice(b"NOT 0=0");
    source.extend_from_slice(&[b')'; 32]);
    out.push(bad("depth-mixed-65", &source, "Limit", "Parse", [160, 160]));
    out.push(assemble(
        "depth-sibling-and-restores",
        vec![vec![nested(atom(), 64, 0), nested(atom(), 0, 64)]],
        b"",
        b"",
    ));
    out.push(assemble(
        "depth-sibling-or-restores",
        vec![vec![nested(atom(), 0, 64)], vec![nested(atom(), 64, 0)]],
        b"",
        b"",
    ));
    for (label, count, disjunction) in [("and", 2729usize, false), ("or", 2047, true)] {
        for extra in [1, 2] {
            let mut terms = vec![atom(); count];
            terms[count - 1] = nested(atom(), extra, 0);
            let terms = if disjunction {
                terms.into_iter().map(|value| vec![value]).collect()
            } else {
                vec![terms]
            };
            out.push(assemble(
                format!("nodes-{label}-{}", 8190 + extra),
                terms,
                b"",
                b"",
            ));
        }
        let join: &[u8] = if disjunction { b" OR " } else { b" AND " };
        let mut source = Vec::new();
        for index in 0..count {
            if index != 0 {
                source.extend_from_slice(join);
            }
            if index == count - 1 {
                source.extend_from_slice(b"NOT NOT NOT ");
            }
            source.extend_from_slice(b"0=0");
        }
        let at = if disjunction { 14336 } else { 21838 };
        out.push(bad(
            format!("nodes-{label}-8193"),
            &source,
            "Limit",
            "Parse",
            [at, at],
        ));
    }
    for count in [255, 256] {
        out.push(assemble(
            format!("markers-{count}"),
            vec![vec![
                Expr::IsNull(Value::marker(), false, b" IS NULL");
                count
            ]],
            b"",
            b"",
        ));
    }
    let mut source = b"? IS NULL AND ".repeat(256);
    source.extend_from_slice(b"? IS NULL");
    out.push(bad("markers-257", &source, "Limit", "Parse", [3584, 3584]));
    for size in [1048575usize, 1048576, 1048577] {
        let mut suffix = b"/*".to_vec();
        suffix.resize(size - 5, b'x');
        suffix.extend_from_slice(b"*/");
        if size <= 1048576 {
            out.push(assemble(
                format!("source-{size}"),
                vec![vec![atom()]],
                b"",
                &suffix,
            ));
        } else {
            let mut source = b"0=0".to_vec();
            source.extend_from_slice(&suffix);
            out.push(bad(
                format!("source-{size}"),
                &source,
                "Limit",
                "Lex",
                [1048576, 1048577],
            ));
        }
    }
    for size in [127usize, 128, 129] {
        let raw = vec![b'a'; size];
        if size <= 128 {
            out.push(one(
                format!("name-unquoted-{size}"),
                eq(Value::name(&raw, &vec![b'A'; size], false), Value::zero()),
            ));
        } else {
            let mut source = raw;
            source.extend_from_slice(b"=0");
            out.push(bad(
                format!("name-unquoted-{size}"),
                &source,
                "Name",
                "Name",
                [0, size],
            ));
        }
        let mut raw = vec![b'"'];
        raw.extend_from_slice(&vec![b'a'; size]);
        raw.push(b'"');
        if size <= 128 {
            out.push(one(
                format!("name-quoted-{size}"),
                eq(Value::name(&raw, &vec![b'a'; size], true), Value::zero()),
            ));
        } else {
            raw.extend_from_slice(b"=0");
            out.push(bad(
                format!("name-quoted-{size}"),
                &raw,
                "Name",
                "Name",
                [0, size + 2],
            ));
        }
    }
    out
}

/// Assemble one valid bounded expression with exactly `fragments` atomic terms.
/// The required profile uses seeds 0..64 and 128 fragments, thus 64 parse calls.
/// Expanded callers split each 1,024-fragment seed into eight 128-fragment calls.
pub fn generated_case(seed: u64, fragments: usize) -> Case {
    assert!((1..=128).contains(&fragments));
    let mut state = seed;
    let mut blocks = vec![Vec::new()];
    for index in 0..fragments {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let choice = ((state >> 32) % 8) as usize;
        let (op, spelling): (Comparison, &[u8]) = match choice % 6 {
            0 => (Comparison::Eq, b"="),
            1 => (Comparison::Ne, b"<>"),
            2 => (Comparison::Lt, b"<"),
            3 => (Comparison::Le, b"<="),
            4 => (Comparison::Gt, b">"),
            _ => (Comparison::Ge, b">="),
        };
        let left = match (seed as usize + index) % 5 {
            0 => Value::name(b"a", b"A", false),
            1 => Value::name(b"\"a\"", b"a", true),
            2 => Value::marker(),
            3 => Value::scalar(b"'a'';?'", ScalarObservation::Text),
            _ => Value::integer(b"- /*?*/ 1", "Minus", [8, 9]),
        };
        let right = match index % 5 {
            0 => Value::marker(),
            1 => Value::scalar(b"NULL", ScalarObservation::Null),
            2 => Value::scalar(b"TRUE", ScalarObservation::Bool(true)),
            3 => Value::scalar(b"X'0g'", ScalarObservation::Binary),
            _ => Value::name("\"가\"".as_bytes(), "가".as_bytes(), true),
        };
        let mut expr = match choice {
            6 => Expr::IsNull(left, false, b" IS NULL"),
            7 => Expr::IsNull(left, true, b" IS NOT NULL"),
            _ => Expr::Compare(left, op, spelling, right),
        };
        if index % 7 == 0 {
            expr = not(expr);
        }
        if index % 11 == 0 {
            expr = group(vec![vec![expr]]);
        }
        if index != 0 && index % 4 == 0 {
            blocks.push(Vec::new());
        }
        blocks
            .last_mut()
            .expect("one block is always present")
            .push(expr);
    }
    assemble(
        format!("generated-{seed}-{fragments}"),
        blocks,
        b"/*?;*/ ",
        b" ; --?;\r\n",
    )
}
