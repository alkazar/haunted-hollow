// What the shared game code (shared/) needs to know about the PlayStation.

#pragma once

#include <psxgte.h>
#include <hwregs_c.h>

#define PLATFORM_PS1 1

// One rider per controller port.
#define PORTS 2

// A timer for the HH_DEBUG overlay (counts hblanks; main sets it up).
#define PLAT_TICKS() ((int) TIMER_VALUE(1))

// Fog start and end (blocks) and lantern strength per zone (see zone_fog in
// game.c): closer than the PC's, as the PS1 can't draw as far.
#define PLAT_FOG { \
	{ 9, 20, 0.5f }, { 7, 17, 0.95f }, { 7, 17, 0.95f }, { 7, 17, 0.95f }, \
	{ 6, 15, 0.9f }, { 7, 17, 0.9f }, { 12, 26, 0.45f }, \
}
#define PLAT_FOG_VICTORY { 18, 34 }
#define PLAT_FOG_OUTRO   { 8, 26 }

#define TXT_JOIN  "PULL THE TRIGGER OR PRESS X TO JOIN"
#define TXT_START "PRESS START (GUN: A) TO RIDE"
#define TXT_KEEP  "PRESS START (GUN: A) TO KEEP RIDING"
#define TXT_QUIT  "SELECT (GUN: B) GOES BACK TO THE START"
