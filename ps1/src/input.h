// Controllers: Namco GunCon light guns and ordinary pads (which steer a
// crosshair), one per controller port.

#pragma once

#include <stdint.h>

#define PORTS 2

enum { DEV_NONE, DEV_PAD, DEV_GUN };

typedef struct {
	int dev;
	// Where it points (screen pixels), if it can tell.
	int aim_valid, x, y;
	int fire;            // trigger held
	int fire_pressed;    // a shot this frame (for guns: once the gun has seen the screen)
	int start_pressed;   // pad START / gun A
	int back_pressed;    // pad SELECT / gun B
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
