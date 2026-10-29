// Haunted Hollow for the Wii: start-up and the main loop. Everything (the world,
// textures, sound) is embedded in the program; the game is in ../shared.

#include <stdint.h>
#include <stdlib.h>
#include <gccore.h>
#include "gfx.h"
#include "cam.h"
#include "stream.h"
#include "audio.h"
#include "input.h"
#include "game.h"
#include "util.h"

extern const uint8_t atlas_bin[], wii_sound_bin[];
extern const uint32_t wii_sound_bin_size;
int wii_refresh_rate(void);

static volatile int quit, power_off;

static void on_reset(u32 irq, void *ctx) {
	(void) irq, (void) ctx;
	quit = 1;
}

static void on_power(void) {
	quit = power_off = 1;
}

int main(void) {
	util_init();
	gfx_init();
	input_init();
	cam_init();
	gfx_load_atlas((const uint16_t *) atlas_bin);
	audio_init(wii_sound_bin, wii_sound_bin_size);
	stream_init(0);
	gfx_world_palette(stream_palette());
	game_init();
	SYS_SetResetCallback(on_reset);
	SYS_SetPowerCallback(on_power);

	int hz = wii_refresh_rate();
	int vblanks = 1;
	while (!quit) {
		input_update(vblanks);
		game_frame(vblanks * 1000 / hz);
		vblanks = gfx_end();
		audio_update(vblanks * 1000 / hz);
	}
	if (power_off)
		SYS_ResetSystem(SYS_POWEROFF, 0, 0);
	return 0;   // back to the Homebrew Channel
}
