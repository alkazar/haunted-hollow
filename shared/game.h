// Gameplay: the ride, spooky targets, the Pumpkin King, scoring and all screens
// (a port of src/game.rs). Nobody can lose: targets just pop into candy.

#pragma once

void game_init(void);
// Advances the game by `ms` milliseconds (reading `inputs`), then draws it.
void game_frame(int ms);
