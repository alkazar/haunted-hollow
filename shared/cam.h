// The camera path recorded from the PC game (track.bin).

#pragma once

#include <stdint.h>
#include "data.h"
#include "gfx.h"

typedef struct {
	V3      eye;        // fine units
	int16_t dir[3], up[3];
	int     seg, zone, speed;   // speed in m/s * 16
} Cam;

enum { POSE_TITLE, POSE_STOP, POSE_END };

void cam_init(void);
// The ride camera `t_ms` into the ride.
void cam_ride(int t_ms, Cam *c);
// The outro camera `t_ms` after it starts.
void cam_outro(int t_ms, Cam *c);
void cam_pose(int which, Cam *c);
