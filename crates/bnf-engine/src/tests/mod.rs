use super::*;

#[test]
fn bnf_engine_loads() {
    // Basic check to ensure the module is linked.
}

#[test]
fn fuzz_random_grammar_strings_never_panic() {
    let vocab: Vec<Vec<u8>> = vec![
        b"".to_vec(),
        b"a".to_vec(),
        b"b".to_vec(),
        b"1".to_vec(),
        b"{".to_vec(),
        b"}".to_vec(),
    ];

    let random_inputs = &[
        "",
        "root ::= [a-z]+",
        "root ::= \"hello\"",
        "malformed {::: [",
        "root ::= root root",
        "root ::= (a | b)+ {invalid}",
        "\0\u{00FF}\u{00FE}random bytes",
    ];

    for g in random_inputs {
        let _ = create_bnf_mask(g, &vocab);
    }
}

#[test]
fn fuzz_random_token_sequences_never_panic() {
    let vocab: Vec<Vec<u8>> = vec![
        b"".to_vec(),
        b"a".to_vec(),
        b"b".to_vec(),
        b"1".to_vec(),
        b"{".to_vec(),
        b"}".to_vec(),
    ];

    if let Ok(mut mask) = create_bnf_mask("root ::= \"a\" \"b\" \"1\"", &vocab) {
        let mut logits = vec![0.0f32; vocab.len()];
        for token_id in [1u32, 2, 3, 999, 0] {
            mask.mask(&mut logits);
            mask.accept(token_id);
        }
    }
}
