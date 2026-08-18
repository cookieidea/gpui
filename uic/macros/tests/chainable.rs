use uic_macros::Chainable;

#[derive(Clone, Copy, Debug, Default, PartialEq, Chainable)]
struct GenericAppearance<T>
where
    T: Default,
{
    pub background: T,
    pub foreground: T,
    pub enabled: bool,
    #[chain(skip)]
    internal_revision: u64,
}

#[test]
fn generates_consuming_setters_for_named_fields() {
    let appearance = GenericAppearance::<u32>::default()
        .background(12)
        .foreground(34)
        .enabled(true);

    assert_eq!(appearance.background, 12);
    assert_eq!(appearance.foreground, 34);
    assert!(appearance.enabled);
    assert_eq!(appearance.internal_revision, 0);
}
