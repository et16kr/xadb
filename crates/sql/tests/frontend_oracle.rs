use test_reference::frontend::literals::{
    NameObservation, diagnostic, lexer_literals, name_literals, token,
};
use test_reference::frontend::{Verdict, check_lex, check_name};

#[test]
fn known_bad_lexical_observations_are_rejected() {
    let cases = lexer_literals();
    let good = cases
        .iter()
        .find(|case| case.id == "name-identity")
        .unwrap();
    let bad = cases
        .iter()
        .find(|case| case.id == "unsupported-not-equal")
        .unwrap();
    assert_eq!(check_lex(good, &good.expected), Verdict::Accepted);
    assert_eq!(check_lex(bad, &bad.expected), Verdict::Accepted);
    for mutation in 0..17 {
        let case = if (11..=14).contains(&mutation) || mutation == 16 {
            bad
        } else {
            good
        };
        let mut observed = case.expected.clone();
        match mutation {
            0 => observed.tokens[0].kind = "Digits",
            1 => observed.tokens[0].start += 1,
            2 => observed.tokens[0].end -= 1,
            3 => {
                observed.tokens.remove(0);
            }
            4 => observed.tokens.insert(0, observed.tokens[0].clone()),
            5 => observed.tokens.swap(0, 1),
            6 => {
                observed.tokens.pop();
            }
            7 => observed
                .tokens
                .push(observed.tokens.last().unwrap().clone()),
            8 => observed.tokens.last_mut().unwrap().start -= 1,
            9 => observed.complete = false,
            10 => observed.diagnostic = Some(diagnostic("Syntax", "Lex", 0, 1)),
            11 => observed.diagnostic.as_mut().unwrap().code = "Syntax",
            12 => observed.diagnostic.as_mut().unwrap().module = "Name",
            13 => observed.diagnostic.as_mut().unwrap().start += 1,
            14 => observed.diagnostic.as_mut().unwrap().end -= 1,
            15 => observed.tokens.push(token("Marker", 11, 12)),
            16 => observed.diagnostic = None,
            _ => unreachable!(),
        }
        assert_eq!(
            check_lex(case, &observed),
            Verdict::Rejected,
            "lexical mutant {mutation}"
        );
    }
}

#[test]
fn known_bad_name_observations_are_rejected() {
    let cases = name_literals();
    let good = cases.iter().find(|case| case.id == "lower").unwrap();
    let unicode = cases
        .iter()
        .find(|case| case.id == "no-unicode-normalization")
        .unwrap();
    let bad = cases.iter().find(|case| case.id == "empty-quoted").unwrap();
    for case in [good, unicode, bad] {
        assert_eq!(check_name(case, &case.expected), Verdict::Accepted);
    }
    for mutation in 0..13 {
        let case = if mutation >= 8 {
            bad
        } else if mutation == 7 {
            unicode
        } else {
            good
        };
        let mut observed = case.expected.clone();
        match mutation {
            0 => observed.as_mut().unwrap().bytes = b"t".to_vec(),
            1 => observed.as_mut().unwrap().bytes.push(b'T'),
            2 => {
                observed.as_mut().unwrap().bytes.pop();
            }
            3 => observed.as_mut().unwrap().quoted = true,
            4 => observed.as_mut().unwrap().start += 1,
            5 => observed.as_mut().unwrap().end -= 1,
            6 => observed = Err(diagnostic("Name", "Name", 0, 1)),
            7 => observed.as_mut().unwrap().bytes = "é".as_bytes().to_vec(),
            8 => observed.as_mut().unwrap_err().code = "Syntax",
            9 => observed.as_mut().unwrap_err().module = "Lex",
            10 => observed.as_mut().unwrap_err().start += 1,
            11 => observed.as_mut().unwrap_err().end -= 1,
            12 => {
                observed = Ok(NameObservation {
                    bytes: Vec::new(),
                    start: 0,
                    end: 2,
                    quoted: true,
                })
            }
            _ => unreachable!(),
        }
        assert_eq!(
            check_name(case, &observed),
            Verdict::Rejected,
            "name mutant {mutation}"
        );
    }
}
