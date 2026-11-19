# Notice

Horizon Locator Heads is an independent Rust reimplementation
of an idea, written from scratch for Horizon. No Java source from the
original mod was copied.

- The idea — player heads on the locator bar, with a compass, borders,
  name modes and include/exclude filters — comes from **Locator Heads**
  by **Haage001** (https://github.com/Haage001/locator-heads),
  licensed LGPL-3.0. The original is a Fabric mod for vanilla Minecraft;
  this is a separate implementation for Horizon/Starling, not a port.
- The head-rendering concept is credited in the original to
  **MCRcortex**.

One deliberate difference: the original shows only the waypoints the
server tracks for the player. This mod shows every loaded player mob,
and waypoint players past loaded range from the server's
`TrackedWaypoint` packets, so a head replaces the vanilla dot at
every distance. This implementation's own code is MIT licensed; see
`LICENSE`.
