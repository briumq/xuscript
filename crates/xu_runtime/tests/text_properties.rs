use proptest::prelude::*;
use xu_runtime::Text;

proptest! {
    #[test]
    fn text_from_str_respects_inline_boundary(s in ".*") {
        let t = Text::from_str(&s);
        prop_assert_eq!(t.len(), s.len());
        if s.len() <= 22 {
            match t {
                Text::Inline { .. } => {},
                _ => prop_assert!(false, "expected Inline for len<=22"),
            }
        } else {
            match t {
                Text::Heap { .. } => {},
                _ => prop_assert!(false, "expected Heap for len>22"),
            }
        }
    }
}

proptest! {
    #[test]
    fn text_concat2_matches_string_concat(a in ".*", b in ".*") {
        let ta = Text::from_str(&a);
        let tb = Text::from_str(&b);
        let t = Text::concat2(&ta, &tb);
        let expected = format!("{}{}", a, b);
        prop_assert_eq!(t.as_str(), expected.as_str());
        let total = a.len() + b.len();
        if total <= 22 {
            match t {
                Text::Inline { .. } => {},
                _ => prop_assert!(false, "expected Inline for total<=22"),
            }
        } else {
            match t {
                Text::Heap { .. } => {},
                _ => prop_assert!(false, "expected Heap for total>22"),
            }
        }
    }
}

proptest! {
    #[test]
    fn text_push_i64_matches_std_to_string(i in any::<i64>()) {
        let mut t = Text::new();
        t.push_i64(i);
        let expected = i.to_string();
        prop_assert_eq!(t.as_str(), expected.as_str());
    }
}
