// Rendering: the streamed world, voxel models, boxes, sparks, the sky and the HUD.
// World positions passed in are "fine" units: 1/256 block (FINE per block).

#pragma once

#include <stdint.h>
#include "platform.h"
#include "data.h"

#define SCREEN_W 320
#define SCREEN_H 240
#define FINE     256
#define OT_LEN   1024

// Projection plane distance: a 70 degree vertical field of view on 240 lines.
#define PROJ_H   171

typedef struct { int32_t x, y, z; } V3;

enum {
	ALIGN_LEFT, ALIGN_CENTER, ALIGN_RIGHT
};

// Models, in models.bin order.
enum {
	M_GHOST, M_BAT_BODY, M_BAT_WING, M_PUMPKIN, M_SKULL, M_TORSO, M_ARM, M_SPIDER,
	M_BOOK0, M_BOOK1, M_BOOK2, M_CANDY, M_BOSS_HEAD, M_BOSS_HEAD_ANGRY, M_BOSS_CAPE,
	M_BOSS_HAND, M_DOOR_L, M_DOOR_R, M_COUNT
};

// Atlas tiles used by code (see src/atlas.rs).
#define T_WHITE     0
#define T_DARKPLANK 8
#define T_PORTRAIT  30
#define T_RING      23
#define T_SLIME     51
#define T_GOLD      55
#define T_PORTRAIT2 37

void gfx_init(void);
// Uploads the 256x256 15-bit atlas.
void gfx_load_atlas(const uint16_t *pixels);

void gfx_begin(void);
// Finishes the frame; returns the number of vertical blanks it took.
int  gfx_end(void);

// Camera: eye in fine units, look direction and up (4.12, need not be unit length).
void gfx_camera(const V3 *eye, const int16_t dir[3], const int16_t up[3]);
// The current camera's basis (4.12): right, down, forward.
const MATRIX *gfx_view(void);
const V3 *gfx_eye(void);
// Fog colour (0..255), fog start/end in blocks (8.8), lantern strength (0..256).
void gfx_fog(const uint8_t rgb[3], int start, int end, int lamp);
// Screen position of a world point; returns its depth in world units, or 0 if
// it is behind the camera.
int  gfx_project(const V3 *p, int *sx, int *sy);

// World.
void gfx_world_palette(const uint8_t *palette);
void gfx_draw_meshlet(const MeshletHeader *m);
void gfx_draw_sky(int with_moon);

// Light at a world point (fine units) from the light grid, as 4.12 per channel.
void gfx_light_at(const V3 *p, int light[3]);

// A voxel model. `m` rotates and scales model space (4.12, 1.0 = model size as
// built); `light` is 4.12 per channel; `flash` 0..4096 tints toward white;
// `tint` (or NULL) multiplies its colours (4.12).
void gfx_draw_model(int model, const MATRIX *m, const V3 *pos, const int light[3], int flash, const int *tint);
const ModelHeader *gfx_model(int model);
// A unit cube centred on `pos`, turned and sized by `m` (4.12, 1.0 = 1 block),
// coloured `rgb` (0..255) with lighting already applied. `tile` < 0 for plain.
void gfx_draw_box(const MATRIX *m, const V3 *pos, int tile, const uint8_t rgb[3]);
// A camera-facing square spark of `size` fine units.
void gfx_draw_spark(const V3 *pos, int size, const uint8_t rgb[3]);
// Soft camera-facing puffs (additive): call gfx_puffs_begin, then gfx_puff for
// each, at a world position and half size in world units.
void gfx_puffs_begin(void);
void gfx_puff(const SVECTOR *pos, int hw, int hh, const uint8_t rgb[3]);

// HUD (drawn on top, in call order). Scales are 4.12 (4096 = 8 pixel glyphs).
int  hud_text_width(const char *s, int scale);
void hud_text(const char *s, int x, int y, int scale, const uint8_t rgb[3], int align);
// Like hud_text, but shrunk to fit within max_w pixels.
void hud_text_fit(const char *s, int x, int y, int scale, int max_w, const uint8_t rgb[3], int align);
// alpha 0..255: at 160+ the rectangle is solid, below that it's half see-through.
void hud_rect(int x, int y, int w, int h, const uint8_t rgb[3], int alpha);
void hud_panel(int x, int y, int w, int h, const uint8_t fill[3], int alpha, const uint8_t border[3]);
void hud_sprite(int tile, int cx, int cy, int size, const uint8_t rgb[3]);
// Darkens the whole screen by `amount` (0..255).
void hud_darken(int amount);
// Fills the screen with white (so a light gun can see where it points).
void hud_flash(void);

// Primitives emitted last frame (for the debug overlay).
extern int gfx_stat_polys, gfx_stat_ml, gfx_stat_v, gfx_stat_q, gfx_stat_skip;
