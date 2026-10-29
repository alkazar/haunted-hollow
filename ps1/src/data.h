// Layouts of the data files written by `haunted_hollow --export-ps1` (see
// src/ps1export.rs in the main game, which must match). Everything is
// little-endian; world positions are in units of 1/64 block, directions 4.12.

#pragma once

#include <stdint.h>
#include <psxgte.h>

#define UNIT        64    // world units per block
#define MODEL_UNIT  256   // model units per block
#define SECTOR      2048

// ------------------------------------------------------------------ world (pvs.bin, WORLD.BIN)

typedef struct {
	char     tag[4];
	uint16_t n_meshlets, n_segs;
	uint32_t n_palette;
	uint16_t peak, lookahead;
	int32_t  seg_len;          // world units
	// uint8_t  palette[n_palette][4];   r, g, b (128 = 1.0), lantern response
	// uint16_t span[n_meshlets][2];     first and last segment that sees it
	// uint32_t seg_start[n_segs + 1];   into list
	// uint16_t list[];                  meshlets to draw from each segment
} PvsHeader;

typedef struct {
	int16_t ox, oy, oz;        // origin, world units
	uint8_t nv, nq;
	int16_t min[3], max[3];    // bounding box, relative to the origin
	uint8_t group[8];          // faces facing +x, -x, +y, -y, +z, -z, other (in that order)
	// MVert v[nv]; MQuad q[nq];
} MeshletHeader;

typedef struct {
	int16_t  x, y, z;
	uint16_t col;              // palette index
} MVert;

typedef struct {
	uint8_t v[4];              // corners in order around the face
	uint8_t tile;              // atlas tile (16 per row)
	uint8_t uv;                // per corner: bit 0 = right edge, bit 1 = bottom edge
	uint8_t rep;               // tile repeats across (low 4 bits) and down (merged faces)
	uint8_t pad;
} MQuad;

// ------------------------------------------------------------------ camera (track.bin)

typedef struct {
	char     tag[4];
	uint16_t n_ride, n_outro, hz, pad;
	// Pose title, stop, end, ride[n_ride], outro[n_outro];
} TrackHeader;

typedef struct {
	int32_t  eye[3];           // 1/256 block
	int16_t  dir[3], up[3];    // 4.12
	uint16_t seg;
	uint8_t  zone, speed;      // speed in m/s * 16
} Pose;

// ------------------------------------------------------------------ models (models.bin)

typedef struct {
	uint16_t nv, nq;
	int16_t  size[3];          // model units
	uint16_t pad;
	// SVECTOR v[nv]; MFace f[nq];
} ModelHeader;

typedef struct {
	uint16_t v[4];
	uint8_t  face;             // 0 +x, 1 -x, 2 +y, 3 -y, 4 +z, 5 -z
	uint8_t  glow;             // 255 = 1.0
	uint8_t  r, g, b, pad;
} MFace;

// ------------------------------------------------------------------ targets (spawns.bin)

typedef struct {
	char     tag[4];
	uint16_t n_one, n_two, n_summon, pad;
	// SpawnRec one[n_one], two[n_two], summon[n_summon];
} SpawnHeader;

typedef struct {
	uint32_t t_ms;             // ride time (summons: boss time)
	uint8_t  kind, variant;
	uint16_t phase;            // 4.12 radians
	uint32_t life_ms;
	int16_t  home[3], a[3], b[3];  // world units (summons: relative to the boss)
	uint16_t pad;
} SpawnRec;

// ------------------------------------------------------------------ light (light.bin)

typedef struct {
	char     tag[4];
	int16_t  x0;
	uint16_t nx;
	int16_t  y0;
	uint16_t ny;
	int16_t  z0;
	uint16_t nz;
	uint16_t step, pad;        // blocks
	// uint16_t grid[nz][ny][nx];  5:6:5, 1.0 = 16 / 32 / 16
} LightHeader;

// ------------------------------------------------------------------ sky (sky.bin)

typedef struct {
	SVECTOR p[4];
	uint8_t tile, uv, r, g, b, pad;
	uint16_t pad2;
} SkyQuad;

// ------------------------------------------------------------------ scenery (scenery.bin)

typedef struct {
	char     tag[4];
	uint16_t n_fire, n_cauldron, n_cloud, n_mist;
	// SVECTOR fire[n_fire], cauldron[n_cauldron]; Cloud cloud[n_cloud]; Mist mist[n_mist];
} SceneryHeader;

typedef struct {
	int16_t x, z, len, pad;    // blocks
} Cloud;

typedef struct {
	int16_t  pos[3], hx, hy;
	uint16_t ph;
	uint8_t  alpha, zones;
	uint8_t  tint[3], wander;  // wander in 1/64 block
	uint16_t rise, pad;
} Mist;

// ------------------------------------------------------------------ sound (sound.bin, SOUND.BIN)

typedef struct {
	char     tag[4];
	uint16_t n_samples, first_sfx;
	uint32_t blob_size;
	// SampleInfo sample[n_samples]; SongInfo song[5]; SongEvent events[];
} SoundHeader;

typedef struct {
	uint32_t off, len;         // in SOUND.BIN
	uint16_t pitch, pad;       // SPU pitch for the sample's own rate
	uint32_t gain;             // 16.16: volume scale (the sample was normalised)
} SampleInfo;

typedef struct {
	uint32_t off, n, len_ms, looping;
} SongInfo;

typedef struct {
	uint32_t t_ms;
	uint16_t pitch, dur_ms, vol_l, vol_r;
	uint8_t  sample, pad;
	uint16_t pad2;
} SongEvent;

// ------------------------------------------------------------------ embedded files

extern const uint8_t pvs_bin[], track_bin[], models_bin[], spawns_bin[];
extern const uint8_t light_bin[], sky_bin[], scenery_bin[], sound_bin[];
