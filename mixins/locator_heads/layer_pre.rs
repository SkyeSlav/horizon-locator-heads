// `native_gui_layer_pre` (API 139) for `minecraft:contextual_info_bar`:
// hide the layer — background, level number and vanilla dots —
// exactly when the locator bar is the current bar, so the post hook
// redraws it with heads in place of the dots. The mechanism is the
// one the docs name: "hides a layer by setting `canceled` in `pre`".
// The post hook always runs after a cancel, so the redraw still
// happens. When any other bar is current, or the mod is off, the
// layer draws untouched: vanilla keeps its XP and jump bars, and the
// heads stay gated off in the post hook.

fn locator_heads_layer_pre(
    game: &crate::game::Game<'_>,
    layer: &str,
    batch: &mut horizon_client::hud::HudBatch,
    canceled: &mut bool,
) {
    use locator_heads_engine as lh;
    let _ = batch;
    if layer != "minecraft:contextual_info_bar" {
        return;
    }
    if !game.native_on(lh::ID) {
        return;
    }
    let cfg = lh::config(&|k| game.native_setting(lh::ID, k));
    if !cfg.enabled {
        return;
    }
    let spectator = game.spectator();
    let contextual = horizon_client::locator_bar::ContextualBar::next(
        game.waypoints.has_waypoints(),
        !spectator && game.jumpable_mount(),
        game.mount_jumping(),
        !game.creative && !spectator,
        game.experience_recent(),
    );
    if contextual == horizon_client::locator_bar::ContextualBar::Locator {
        *canceled = true;
    }
}
