//! Probe which grammar forms kbnf accepts. Run: cargo run --example mask_probe -p roco-engine
use std::collections::HashMap;

fn load_vocab() -> Vec<Vec<u8>> {
    let raw = std::fs::read("assets/vocab/rwkv_vocab_v20230424.json").unwrap();
    let map: HashMap<String, serde_json::Value> = serde_json::from_slice(&raw).unwrap();
    let mut v: Vec<(usize, Vec<u8>)> = map
        .into_iter()
        .map(|(id, tok)| {
            let bytes = match tok {
                serde_json::Value::String(s) => s.into_bytes(),
                serde_json::Value::Array(a) => a.into_iter().map(|n| n.as_u64().unwrap() as u8).collect(),
                other => panic!("unexpected vocab entry: {other:?}"),
            };
            (id.parse::<usize>().unwrap(), bytes)
        })
        .collect();
    v.sort_by_key(|(id, _)| *id);
    v.into_iter().map(|(_, bytes)| bytes).collect()
}

fn main() {
    let vocab = load_vocab();
    println!("vocab size: {}", vocab.len());

    let crate_prims = roco_engine::grammar::json_schema::primitives_bnf();
    let full_gbnf = roco_engine::grammar::json_schema::schema_to_gbnf(
        "root",
        &serde_json::json!({
            "type": "object",
            "properties": {
                "title": {"type": "string"},
                "content": {"type": "string"}
            }
        }),
    ).unwrap();
    std::fs::write("/tmp/crate_full_grammar.gbnf", &full_gbnf).unwrap();
    let tests: Vec<(&str, &str)> = vec![
        ("crate-primitives", &crate_prims),
        ("full-schema-gbnf", &full_gbnf),
        ("plain-obj-complement", concat!(
            "string ::= \"\\\"\" {char | escape} \"\\\"\"\n",
            "char ::= #ex\"[\"\\\\]\"\n",
            "escape ::= \"\\\\\" (\"\\\"\" | \"\\\\\" | \"/\" | \"b\" | \"f\" | \"n\" | \"r\" | \"t\")\n",
            "root_obj ::= \"{\" \"\\\"title\\\"\" \":\" string \",\" \"\\\"content\\\"\" \":\" string \"}\"\n",
            "root ::= root_obj\n",
        )),
        ("simple-string", concat!(
            "string ::= \"\\\"\" {char | escape} \"\\\"\"\n",
            "char ::= #ex\"[\"\\\\]\"\n",
            "escape ::= \"\\\\\" (\"\\\"\" | \"\\\\\" | \"/\" | \"b\" | \"f\" | \"n\" | \"r\" | \"t\")\n",
            "root ::= \"zzz\" string\n",
        )),
        ("plain-obj-orig-char", concat!(
            "string ::= \"\\\"\" {char | escape} \"\\\"\"\n",
            "char ::= #'[ -~]'\n",
            "escape ::= \"\\\\\" (\"\\\"\" | \"\\\\\" | \"/\" | \"b\" | \"f\" | \"n\" | \"r\" | \"t\")\n",
            "root_obj ::= \"{\" \"\\\"title\\\"\" \":\" string \",\" \"\\\"content\\\"\" \":\" string \"}\"\n",
            "root ::= root_obj\n",
        )),
    ];

    for (name, g) in tests {
        let kbnf = roco_engine::grammar::gbnf_to_kbnf(g);
        match roco_engine::create_bnf_mask(&kbnf, &vocab) {
            Ok(mut m) => {
                println!("{name}: mask built OK");
                probe_mask(&mut *m, &vocab);
            }
            Err(e) => println!("{name}: ERROR {e}"),
        }
    }
}

#[allow(dead_code)]
fn _unused() {}

fn probe_mask(m: &mut dyn roco_engine::BnfMask, vocab: &[Vec<u8>]) {
    let find = |b: &[u8]| {
        vocab
            .iter()
            .position(|v| v == b)
            .unwrap_or_else(|| panic!("no vocab token for {b:?}")) as u32
    };
    let quote = find(b"\"");
    // For simple-string: z z z " h " then check what's allowed
    let toks: Vec<(&[u8], u32)> = vec![
        (b"z", find(b"z")), (b"z", find(b"z")), (b"z", find(b"z")),
        (b"\"", find(b"\"")), (b"h", find(b"h")), (b"\"", find(b"\"")),
    ];
    let mut accepted = String::new();
    for (bytes, tid) in toks {
        let mut logits = vec![0.0f32; vocab.len()];
        m.mask(&mut logits);
        let allowed: Vec<usize> = logits
            .iter()
            .enumerate()
            .filter(|(_, v)| v.is_finite())
            .map(|(i, _)| i)
            .collect();
        let quote_allowed = allowed.contains(&(quote as usize));
        accepted.push_str(&String::from_utf8_lossy(bytes));
        println!(
            "  after {accepted:?}: allowed={} quote_allowed={}",
            allowed.len(),
            quote_allowed
        );
        let cont = m.accept(tid);
        println!("    (accept id={tid} bytes={bytes:?} returned {cont})");
    }
}
