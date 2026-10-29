// Haunted Hollow for the PlayStation: start-up and the main loop.

#include <stdint.h>
#include <stdio.h>
#include <psxapi.h>
#include <psxgpu.h>
#include <psxcd.h>
#include "gfx.h"
#include "cam.h"
#include "stream.h"
#include "audio.h"
#include "input.h"
#include "game.h"
#include "util.h"
#include <hwregs_c.h>

static int find_file(const char *name, int *size) {
	CdlFILE f;
	if (!CdSearchFile(&f, name)) {
		printf("missing %s\n", name);
		return -1;
	}
	if (size)
		*size = f.size;
	return CdPosToInt(&f.pos);
}

// Reads a whole file from the CD into `buf`; returns its size.
static int read_file(const char *name, void *buf, int max) {
	int size;
	int lba = find_file(name, &size);
	if (lba < 0 || size > max)
		return 0;
	CdlLOC pos;
	CdIntToPos(lba, &pos);
	CdControl(CdlSetloc, &pos, 0);
	CdRead((size + SECTOR - 1) / SECTOR, buf, CdlModeSpeed);
	CdReadSync(0, 0);
	return size;
}

int main(void) {
	util_init();
	gfx_init();
	CdInit();
	cam_init();
	input_init();

	// The texture atlas and the sound samples are read into the (not yet used)
	// world streaming memory, then handed to the GPU and the sound chip.
	void *scratch = stream_scratch();
	read_file("\\ATLAS.BIN;1", scratch, stream_scratch_size());
	gfx_load_atlas(scratch);
	int sound = read_file("\\SOUND.BIN;1", scratch, stream_scratch_size());
	audio_init(scratch, sound);

	stream_init(find_file("\\WORLD.BIN;1", 0));
	gfx_world_palette(stream_palette());
	// Everything seen from the station, before showing anything.
	while (!stream_ready(0)) {
		stream_update(0);
		VSync(0);
	}

	game_init();
	TIMER_CTRL(1) = 0x0100;   // timer 1 counts hblanks (for the HH_DEBUG overlay)
	int vblanks = 1;
	for (;;) {
		input_update(vblanks);
		int ms = vblanks * 50 / 3;
		game_frame(ms);
		vblanks = gfx_end();
		audio_update(vblanks * 50 / 3);
	}
}
