// `/locatorheads ...` and `/lh ...`: status, toggle and the filter
// lists. The command arrives without its slash.
fn locator_heads_command(game: &mut crate::game::Game<'_>, command: &str, ran: &mut bool) {
    use locator_heads_engine as lh;
    if *ran || !game.native_on(lh::ID) {
        return;
    }
    {
        let game = &*game;
        lh::load(&|k| game.native_setting(lh::ID, k));
    }
    let Some(out) = lh::command(command) else {
        return;
    };
    *ran = true;
    for line in out.lines {
        game.native_chat(&line);
    }
    for (k, v) in lh::take_store() {
        game.native_set_setting(lh::ID, &k, &v);
    }
}
