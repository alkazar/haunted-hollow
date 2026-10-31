// Music and sound effects on the PS1's sound chip.

#pragma once

#include <stdint.h>

// Effects, in the PC game's order (src/audio.rs).
enum {
	SFX_POP, SFX_SHOT, SFX_BONUS, SFX_POOF, SFX_GHOST, SFX_SQUEAK, SFX_RATTLE, SFX_ZIP,
	SFX_BOING, SFX_FLUTTER, SFX_BEEP, SFX_GO, SFX_JOIN, SFX_WHOOSH, SFX_BONK, SFX_LAUGH,
	SFX_RUMBLE, SFX_BOOM, SFX_DING, SFX_DEEP_BOING, SFX_COUNT
};

enum { MUSIC_NONE = -1, MUSIC_TITLE, MUSIC_RIDE, MUSIC_BOSS, MUSIC_VICTORY, MUSIC_RESULTS };

// Uploads SOUND.BIN (already read into memory) to sound RAM.
void audio_init(const void *blob, int size);
// Advances the music by `ms` milliseconds.
void audio_update(int ms);
void audio_music(int song);
// vol 0..1, pan -1..1 (left..right), pitch 1 = as recorded.
void audio_sfx(int sfx, float vol, float pan, float pitch);
// Loudness of the cart's rattle, 0..1.
void audio_cart(float vol);
