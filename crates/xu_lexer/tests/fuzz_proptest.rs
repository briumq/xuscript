use proptest::prelude::*;
use proptest::test_runner::Config as ProptestConfig;
use xu_lexer::{Lexer, normalize_source};
use xu_syntax::TokenKind;

fn any_xu_like() -> impl Strategy<Value = String> {
    fn is_cjk(c: char) -> bool {
        (0x4E00u32..=0x9FFFu32).contains(&(c as u32))
    }

    let ascii =
        proptest::collection::vec(any::<char>().prop_filter("ascii", |c| c.is_ascii()), 0..40)
            .prop_map(|v| v.into_iter().collect::<String>());
    let unicode = proptest::collection::vec(
        any::<char>().prop_filter("non-ascii-non-cjk", |c| !c.is_ascii() && !is_cjk(*c)),
        0..40,
    )
    .prop_map(|v| v.into_iter().collect::<String>());
    let sym = "€ Ω … ، ؛ ۔ ,;()[]{}?/* */ // \"\\ \n \t . if else while for in return break continue throw try catch finally not and or match true false null"
        .to_string();
    (ascii, unicode, any::<bool>(), any::<bool>()).prop_map(move |(a, b, f1, f2)| {
        let mut s = String::new();
        s.push_str(&a);
        s.push_str(&b);
        if f1 {
            s.push_str(&sym);
        }
        if f2 {
            s.push_str(&sym);
        }
        s.chars().take(200).collect()
    })
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 16, max_shrink_iters: 200, .. ProptestConfig::default()
    })]
    #[ignore]
    #[test]
    fn lex_random_input_should_not_panic(s in any_xu_like()) {
        let normalized = normalize_source(&s);
        let result = Lexer::new(&normalized.text).lex();
        // Must end with EOF.
        assert!(matches!(result.tokens.last().map(|t| t.kind), Some(TokenKind::Eof)));
        // Diagnostics are allowed; this only checks robustness (no panic).
        assert!(!result.tokens.is_empty());
    }
}
