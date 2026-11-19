<img src="icon.png" width="128" height="128" alt="Horizon Locator Heads icon">

# Horizon Locator Heads

[![Release](https://img.shields.io/github/v/release/SkyeSlav/horizon-locator-heads)](https://github.com/SkyeSlav/horizon-locator-heads/releases) [![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

Player heads on the locator bar for Horizon/Starling.

Every other player on the server shows as their head on the bar instead of a coloured dot. Heads slide along the bar as you turn and shrink with distance, the same way vanilla does. It works for far away players too, not just ones close by.

- Heads replace the dots completely. No dot shows under a head, at any distance.
- Heads only show when the locator bar is showing. Pick up XP and they hide for a few seconds while the XP bar is up, then come back.
- Compass overlay. N/E/S/W letters, or coordinates if you prefer, with a shadow toggle.
- Borders in team colour, or a colour you pick with `/lh bordercolor <hex>`.
- Names off, always, looking at, or while the player list is open. Names take the border colour and slide in.
- Include or exclude players by name.

## Install

Needs Starling 0.8.0 (game 26.3). Put `locator-heads-1.0.0.hnmod` in the native mods folder and compile in Starling.

## Settings

Options > Mods > Locator Heads. Commands: `/lh` for status, `/lh toggle`, `/lh filter include|exclude <names>`, `/lh filter clear`, `/lh bordercolor <hex>`.

## Credits

Based on the idea behind [Locator Heads](https://github.com/Haage001/locator-heads) by Haage001. This is a separate Rust implementation written from scratch, no Java source copied. This implementation's own code is MIT licensed, see LICENSE. Full credits in NOTICE.md.
