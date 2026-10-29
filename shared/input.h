// Controllers, one per player: on the PS1 Namco GunCon light guns or pads (which
// steer a crosshair), on the Wii Wii Remotes. PORTS comes from platform.h.

#pragma once

#include <stdint.h>
#include "platform.h"

// DEV_GUN aims by itself (light gun, Wii Remote pointer); DEV_PAD steers a crosshair.
enum { DEV_NONE, DEV_PAD, DEV_GUN };

typedef struct {
	int dev;
	// Where it points (screen pixels), if it can tell.
	int aim_valid, x, y;
	int fire;            // trigger held
	int fire_pressed;    // a shot this frame (for guns: once the gun has seen the screen)
	int start_pressed;   // starts the ride, pauses it
	int back_pressed;    // back to the title screen while paused
	int any_pressed;     // anything pressed (to join)
} Input;

extern Input inputs[PORTS];

void input_init(void);
// Reads the controllers; `frames` is how many vertical blanks went by.
void input_update(int frames);
// True if a light gun needs the screen lit up this frame to see where it points.
int  input_wants_flash(void);
// Re-centres a gun so that its last shot lands on (x, y).
void input_calibrate(int port, int x, int y);
// Buzzes a controller that can (Wii Remotes).
void input_rumble(int port, int ms);
// What to tell a player who hasn't joined yet ("PRESS A"...).
const char *input_join_label(int port);
