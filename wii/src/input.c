// Wii Remotes: they point with their IR camera at the sensor bar, like on the PC.
// B (trigger) or A shoots and joins, + starts, HOME pauses, - (paused) quits.

#include <stdint.h>
#include <wiiuse/wpad.h>
#include "input.h"
#include "gfx.h"

Input inputs[PORTS];
static int rumble_left[PORTS];   // ms

void input_init(void) {
	WPAD_Init();
	WPAD_SetDataFormat(WPAD_CHAN_ALL, WPAD_FMT_BTNS_ACC_IR);
	// Pointer positions in the HUD's 320x240 (with a margin past the edges).
	WPAD_SetVRes(WPAD_CHAN_ALL, SCREEN_W, SCREEN_H);
}

void input_update(int frames) {
	WPAD_ScanPads();
	for (int p = 0; p < PORTS; p++) {
		Input *in = &inputs[p];
		u32 type;
		in->fire_pressed = in->start_pressed = in->back_pressed = in->any_pressed = 0;
		if (WPAD_Probe(p, &type) != WPAD_ERR_NONE) {
			in->dev = DEV_NONE;
			in->aim_valid = in->fire = 0;
			continue;
		}
		in->dev = DEV_GUN;
		u32 down = WPAD_ButtonsDown(p), held = WPAD_ButtonsHeld(p);
		const u32 shoot = WPAD_BUTTON_B | WPAD_BUTTON_A;
		in->fire = (held & shoot) != 0;
		in->fire_pressed = (down & shoot) != 0;
		in->start_pressed = (down & (WPAD_BUTTON_PLUS | WPAD_BUTTON_HOME)) != 0;
		in->back_pressed = (down & WPAD_BUTTON_MINUS) != 0;
		in->any_pressed = (down & ~(WPAD_BUTTON_PLUS | WPAD_BUTTON_HOME)) != 0;
		WPADData *d = WPAD_Data(p);
		in->aim_valid = d->ir.valid;
		if (d->ir.valid) {
			in->x = (int) d->ir.x;
			in->y = (int) d->ir.y;
		}
		if (rumble_left[p] > 0) {
			rumble_left[p] -= frames * 1000 / 60;
			if (rumble_left[p] <= 0)
				WPAD_Rumble(p, 0);
		}
	}
}

int input_wants_flash(void) {
	return 0;   // (the remote sees the sensor bar, not the picture)
}

void input_calibrate(int port, int x, int y) {
	(void) port, (void) x, (void) y;
}

void input_rumble(int port, int ms) {
	if (inputs[port].dev == DEV_NONE)
		return;
	WPAD_Rumble(port, 1);
	if (ms > rumble_left[port])
		rumble_left[port] = ms;
}

const char *input_join_label(int port) {
	return inputs[port].dev == DEV_NONE ? "PRESS 1+2" : "PRESS A";
}
