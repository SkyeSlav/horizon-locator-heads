// The mod's settings page. The filter lists are free text, so they are
// set with `/lh filter include <names>` / `/lh filter exclude <names>`
// instead (see command.rs).
fn locator_heads_page(game: &mut crate::game::Game<'_>, mod_id: &str, buttons: &mut Vec<String>) {
    use locator_heads_engine as lh;
    if mod_id != lh::ID {
        return;
    }
    {
        let game = &*game;
        lh::load(&|k| game.native_setting(lh::ID, k));
    }
    buttons.extend(lh::settings_buttons());
}
