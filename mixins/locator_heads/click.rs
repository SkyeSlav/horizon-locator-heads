// A click on the settings page: the button's setting moves to its next value.
fn locator_heads_click(game: &mut crate::game::Game<'_>, mod_id: &str, button: usize) {
    use locator_heads_engine as lh;
    if mod_id != lh::ID {
        return;
    }
    {
        let game = &*game;
        lh::load(&|k| game.native_setting(lh::ID, k));
    }
    lh::settings_click(button);
    for (k, v) in lh::take_store() {
        game.native_set_setting(lh::ID, &k, &v);
    }
}
