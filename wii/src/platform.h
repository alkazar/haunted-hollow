// What the shared game code (shared/) needs to know about the Wii.

#pragma once

#include <stdint.h>
#include <ogcsys.h>
#include <ogc/lwp_watchdog.h>

#define PLATFORM_WII 1

// Four Wii Remotes.
#define PORTS 4

// 427 wide when the Wii is set to 16:9 (the picture is the usual 640x480, stretched
// by the TV), else 320; set by gfx_init from the console's settings.
extern int wii_screen_w;
#define SCREEN_W wii_screen_w

// The shared code's fixed-point types (named as in the PS1's SDK).
typedef struct { int16_t vx, vy, vz, pad; } SVECTOR;
typedef struct { int16_t m[3][3]; int32_t t[3]; } MATRIX;

// A timer for the HH_DEBUG overlay, in TV lines (about 64 us) like the PS1's.
#define PLAT_TICKS() ((int) (gettime() / (TB_TIMER_CLOCK / 16)))

// Fog start and end (blocks) and lantern strength per zone (see zone_fog in
// game.c): the PC's, but ending by 46 blocks (the exporter keeps faces to 48).
#define PLAT_FOG { \
	{ 24, 46, 0.5f }, { 18, 46, 0.95f }, { 18, 46, 0.95f }, { 18, 46, 0.95f }, \
	{ 10, 44, 0.9f }, { 14, 46, 0.9f }, { 24, 46, 0.45f }, \
}
#define PLAT_FOG_VICTORY { 22, 46 }
#define PLAT_FOG_OUTRO   { 8, 36 }

#define TXT_JOIN  "PRESS A ON YOUR WII REMOTE TO JOIN"
#define TXT_START "PRESS + TO START"
#define TXT_KEEP  "PRESS HOME TO KEEP RIDING"
#define TXT_QUIT  "PRESS - TO GO BACK TO THE START"
