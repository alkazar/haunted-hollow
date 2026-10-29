// Controllers, read by the BIOS every vertical blank.
//
// A GunCon only knows where it points if it sees the TV's beam, and most of the
// ride is dark. So when the trigger is pulled and the gun can't see the screen,
// the next frame is drawn white (see input_wants_flash) and the shot lands
// wherever the gun saw it. Flashes are kept under three a second.
//
// Pads (digital or analog) move a crosshair instead.

#include <stdint.h>
#include <psxapi.h>
#include <psxpad.h>
#include "input.h"
#include "gfx.h"

// GunCon coordinates: X in 8 MHz clocks since the start of the line, Y in lines.
// These map them onto a 320x240 NTSC picture; each rider's gun is fine-tuned by
// the practice shot before the ride (input_calibrate).
#define GUN_X0     77
#define GUN_XSPAN  384
#define GUN_Y0     16
#define MAX_CALIB  48
// Game frames to wait for the gun to see a flash, and the shortest gap between
// flashes (vertical blanks).
#define FLASH_WAIT 2
#define FLASH_GAP  21
// Frames the crosshair stays up after the gun last saw the screen.
#define SEEN_HOLD  20

static uint8_t padbuf[PORTS][34];
Input inputs[PORTS];

typedef struct {
	uint16_t prev;
	int cx, cy;              // pad crosshair, 1/16 pixel
	int calib_x, calib_y;
	int gx, gy, unseen;      // last gun position and frames since it was seen
	int pending;             // frames left to wait for a flash
	int flash_cool;
} Port;
static Port ports[PORTS];
static int flash_now;

void input_init(void) {
	for (int p = 0; p < PORTS; p++) {
		padbuf[p][0] = padbuf[p][1] = 0xff;
		ports[p].cx = (SCREEN_W / 2) << 4;
		ports[p].cy = (SCREEN_H / 2) << 4;
		ports[p].unseen = 999;
	}
	InitPAD(padbuf[0], 34, padbuf[1], 34);
	StartPAD();
	ChangeClearPAD(0);
}

int input_wants_flash(void) {
	return flash_now;
}

void input_calibrate(int port, int x, int y) {
	Port *s = &ports[port];
	if (inputs[port].dev != DEV_GUN)
		return;
	int dx = x - (s->gx - s->calib_x), dy = y - (s->gy - s->calib_y);
	if (dx < -MAX_CALIB) dx = -MAX_CALIB;
	if (dx > MAX_CALIB) dx = MAX_CALIB;
	if (dy < -MAX_CALIB) dy = -MAX_CALIB;
	if (dy > MAX_CALIB) dy = MAX_CALIB;
	s->calib_x = dx, s->calib_y = dy;
}

static int clampi(int v, int lo, int hi) {
	return v < lo ? lo : v > hi ? hi : v;
}

void input_update(int frames) {
	flash_now = 0;
	for (int p = 0; p < PORTS; p++) {
		Input *in = &inputs[p];
		Port *s = &ports[p];
		const PADTYPE *pad = (const PADTYPE *) padbuf[p];
		in->fire_pressed = in->start_pressed = in->back_pressed = in->any_pressed = 0;
		if (pad->stat != 0) {
			in->dev = DEV_NONE;
			in->aim_valid = 0;
			in->fire = 0;
			s->prev = 0;
			continue;
		}
		uint16_t btn = ~pad->btn;
		uint16_t pressed = btn & ~s->prev;
		s->prev = btn;
		in->any_pressed = pressed != 0;
		if (pad->type == PAD_ID_GUNCON) {
			in->dev = DEV_GUN;
			in->fire = (btn & GCON_TRIGGER) != 0;
			in->start_pressed = (pressed & GCON_A) != 0;
			in->back_pressed = (pressed & GCON_B) != 0;
			// (1, 10) means it can't see the screen.
			int seen = !(pad->gun_x == 1 && pad->gun_y == 10) && pad->gun_x > 8 && pad->gun_y > 4;
			if (seen) {
				s->gx = ((pad->gun_x - GUN_X0) * SCREEN_W) / GUN_XSPAN + s->calib_x;
				s->gy = pad->gun_y - GUN_Y0 + s->calib_y;
				s->unseen = 0;
			} else {
				s->unseen += frames;
			}
			in->x = clampi(s->gx, 0, SCREEN_W - 1);
			in->y = clampi(s->gy, 0, SCREEN_H - 1);
			in->aim_valid = s->unseen < SEEN_HOLD;
			if (s->flash_cool > 0)
				s->flash_cool -= frames;
			if (pressed & GCON_TRIGGER) {
				if (seen) {
					in->fire_pressed = 1;
				} else if (s->flash_cool <= 0) {
					s->pending = FLASH_WAIT;
					s->flash_cool = FLASH_GAP;
					flash_now = 1;
				}
			} else if (s->pending > 0) {
				if (seen) {
					in->fire_pressed = 1;
					in->aim_valid = 1;
					s->pending = 0;
				} else if (--s->pending > 0) {
					flash_now = 1;
				}
			}
			continue;
		}
		// A pad: the d-pad or left stick moves the crosshair.
		in->dev = DEV_PAD;
		in->fire = (btn & (PAD_CROSS | PAD_SQUARE | PAD_R1 | PAD_R2 | PAD_L1 | PAD_L2)) != 0;
		in->fire_pressed = (pressed & (PAD_CROSS | PAD_SQUARE | PAD_R1 | PAD_R2 | PAD_L1 | PAD_L2)) != 0;
		in->start_pressed = (pressed & PAD_START) != 0;
		in->back_pressed = (pressed & (PAD_SELECT | PAD_TRIANGLE)) != 0;
		int dx = 0, dy = 0;
		if (btn & PAD_LEFT) dx -= 64;
		if (btn & PAD_RIGHT) dx += 64;
		if (btn & PAD_UP) dy -= 64;
		if (btn & PAD_DOWN) dy += 64;
		if ((pad->type == PAD_ID_ANALOG_STICK || pad->type == PAD_ID_ANALOG) && pad->len >= 3) {
			int ax = pad->ls_x - 128, ay = pad->ls_y - 128;
			if (ax > 24 || ax < -24) dx += (ax * 72) >> 7;
			if (ay > 24 || ay < -24) dy += (ay * 72) >> 7;
		}
		s->cx = clampi(s->cx + dx * frames, 0, (SCREEN_W - 1) << 4);
		s->cy = clampi(s->cy + dy * frames, 0, (SCREEN_H - 1) << 4);
		in->x = s->cx >> 4;
		in->y = s->cy >> 4;
		in->aim_valid = 1;
	}
}

void input_rumble(int port, int ms) {
	(void) port, (void) ms;   // (GunCons don't; DualShock motors aren't driven)
}

const char *input_join_label(int port) {
	switch (inputs[port].dev) {
	case DEV_GUN: return "PULL TRIGGER";
	case DEV_PAD: return "PRESS X";
	default: return "NO CONTROLLER";
	}
}
