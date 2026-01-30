use proptest::prelude::*;
use xu_runtime::Text;

const INLINE_CAP: usize = 22;

proptest! {
    #[test]
    fn text_from_str_respects_inline_boundary(s in ".*") {
        let t = Text::from_str(&s);
        prop_assert_eq!(t.len(), s.len());
        if s.len() <= INLINE_CAP {
            match t {
                Text::Inline { .. } => {},
                _ => prop_assert!(false, "expected Inline for len<=INLINE_CAP"),
            }
        } else {
            match t {
                Text::Heap { .. } => {},
                _ => prop_assert!(false, "expected Heap for len>INLINE_CAP"),
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
        if total <= INLINE_CAP {
            match t {
                Text::Inline { .. } => {},
                _ => prop_assert!(false, "expected Inline for total<=INLINE_CAP"),
            }
        } else {
            match t {
                Text::Heap { .. } => {},
                _ => prop_assert!(false, "expected Heap for total>INLINE_CAP"),
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
