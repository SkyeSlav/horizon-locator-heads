// The mod's row in Options > Mods.
fn locator_heads_listed(mods: &mut Vec<crate::game::native_hooks::NativeMod>) {
    use locator_heads_engine as lh;
    mods.push(crate::game::native_hooks::NativeMod {
        id: lh::ID.into(),
        title: format!("{} {}", lh::NAME, lh::VERSION),
        description: lh::DESCRIPTION.into(),
        keys: Vec::new(),
    });
}
