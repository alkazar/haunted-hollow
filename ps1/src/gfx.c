// Rendering. The GTE projects everything; the CPU adds the rider's lantern and the
// fog per vertex (by depth, from lookup tables), culls back faces and splits faces
// that come too close to the camera so they neither vanish nor warp.

#include <stdint.h>
#include <string.h>
#include <psxgpu.h>
#include <psxgte.h>
#include <inline_c.h>
#include "gfx.h"
#include "util.h"

#define PRIMBUF_SIZE (172 * 1024)
// Nothing nearer than this (world units) is drawn; faces crossing it are split.
#define NEAR_Z       12
// OT index = depth >> OT_SHIFT.
#define OT_SHIFT     1
// Faces bigger than this on screen (pixels) are split while they're near.
#define SPLIT_SIZE   400
#define MAX_SPLIT    1
// Small details further than this (world units) aren't drawn.
#define DETAIL_Z     (12 * 64)

// The SDK's GTE load macros don't tell the compiler they read memory, so make
// sure everything they load has really been stored first.
#define GTE_SYNC()   __asm__ volatile ("" ::: "memory")

// Depths of a meshlet's vertices live in the scratchpad (fast RAM; there's no data cache).
#define SCRATCH_Z    ((int32_t *) 0x1f800000)

#define ATLAS_X      320
#define PUFF_X       576

typedef struct {
	DISPENV  disp;
	DRAWENV  draw;
	uint32_t ot[OT_LEN];
	uint8_t  pri[PRIMBUF_SIZE];
} Frame;

static Frame frames[2];
static int cur;
static uint8_t *nextpri, *primend;
static uint32_t *ot;
static uint16_t tp_atlas, tp_puff;
static int last_vblank;

#define HUD_MAX 640
static void *hud_prims[HUD_MAX];
static int hud_n;

static MATRIX view;
static V3 eye, eye_u;
static int far_z;
static uint8_t fog_rgb[3];
static int16_t fog_tex[3];      // fog colour on the 128 = 1.0 scale, doubled for textures
static uint8_t fog_lut[256], lamp_lut[256];   // by depth >> 4
static uint16_t fog_keep[256];                // 256 - fog
static uint8_t fog_add[256][3];               // fog colour * fog (128 = 1.0 scale)
static int16_t fog_add16[256][3];             // the same * 16, for the GTE
static const uint8_t *palette;

int gfx_stat_polys, gfx_stat_ml, gfx_stat_v, gfx_stat_q, gfx_stat_skip;

// Scratch for transformed vertices.
#define MAX_VERTS 1536
static DVECTOR sxy[MAX_VERTS];
static int32_t sz[MAX_VERTS];
static uint32_t vcol[MAX_VERTS];   // r | g << 8 | b << 16

static const ModelHeader *models[M_COUNT];
// Distinct glow values of each model's faces (each face's `pad` holds its index).
#define MAX_GLOWS 8
static uint8_t glows[M_COUNT][MAX_GLOWS];
static uint8_t n_glows[M_COUNT];
static const LightHeader *light;
static const SkyQuad *sky;
static int sky_n;
static SVECTOR star_pos[512];
static uint8_t star_col[512];
static int star_n;

// For each face uv code, the corner offsets (u | v << 8) in the GPU's corner order.
static uint16_t uv_lut[256][4];

static void make_uv_lut(void) {
	static const uint8_t order[4] = { 0, 1, 3, 2 };
	for (int code = 0; code < 256; code++)
		for (int k = 0; k < 4; k++) {
			int c = order[k];
			uv_lut[code][k] = ((code >> (c * 2)) & 1) * 15 | (((code >> (c * 2 + 1)) & 1) * 15) << 8;
		}
}

static void *alloc(int size) {
	if (nextpri + size > primend)
		return 0;
	void *p = nextpri;
	nextpri += size;
	return p;
}

static void hud_add(void *p) {
	if (hud_n < HUD_MAX)
		hud_prims[hud_n++] = p;
}

// ------------------------------------------------------------------ setup

static void make_puff_texture(void) {
	// A soft round blob (STP bit set so it blends).
	static uint16_t px[16 * 16];
	for (int y = 0; y < 16; y++) {
		for (int x = 0; x < 16; x++) {
			int dx = x * 2 - 15, dy = y * 2 - 15;
			int d = 256 - (dx * dx + dy * dy);
			if (d <= 0) {
				px[y * 16 + x] = 0;
				continue;
			}
			int a = (d * d) >> 11;   // 0..32
			if (a > 31) a = 31;
			if (a < 1) a = 1;
			px[y * 16 + x] = 0x8000 | a | (a << 5) | (a << 10);
		}
	}
	RECT r = { PUFF_X, 0, 16, 16 };
	LoadImage(&r, (const uint32_t *) px);
}

void gfx_init(void) {
	ResetGraph(0);
	SetVideoMode(MODE_NTSC);
	for (int i = 0; i < 2; i++) {
		Frame *f = &frames[i];
		SetDefDispEnv(&f->disp, 0, i ? 0 : SCREEN_H, SCREEN_W, SCREEN_H);
		SetDefDrawEnv(&f->draw, 0, i ? SCREEN_H : 0, SCREEN_W, SCREEN_H);
		f->draw.isbg = 1;
		f->draw.dtd = 1;
		setRGB0(&f->draw, 0, 0, 0);
	}
	cur = 0;
	InitGeom();
	gte_SetGeomOffset(SCREEN_W / 2, SCREEN_H / 2);
	gte_SetGeomScreen(PROJ_H);
	tp_atlas = getTPage(2, 0, ATLAS_X, 0);
	tp_puff = getTPage(2, 1, PUFF_X, 0);
	make_puff_texture();
	make_uv_lut();

	const uint8_t *mb = models_bin;
	for (int i = 0; i < M_COUNT; i++) {
		models[i] = (const ModelHeader *) (mb + ((const uint32_t *) (mb + 8))[i]);
		MFace *f = (MFace *) ((const SVECTOR *) (models[i] + 1) + models[i]->nv);
		for (int k = 0; k < models[i]->nq; k++) {
			int g = 0;
			while (g < n_glows[i] && glows[i][g] != f[k].glow)
				g++;
			if (g == n_glows[i] && g < MAX_GLOWS)
				glows[i][n_glows[i]++] = f[k].glow;
			f[k].pad = g < MAX_GLOWS ? g : 0;
		}
	}
	light = (const LightHeader *) light_bin;
	sky_n = *(const uint32_t *) (sky_bin + 4);
	sky = (const SkyQuad *) (sky_bin + 8);
	// Stars (the small quads) are drawn as dots at their centres; positions are
	// quartered so they fit the GTE's range (projection doesn't mind the scale).
	star_n = 0;
	for (int i = 0; i < sky_n; i++) {
		const SkyQuad *q = &sky[i];
		int dx = q->p[0].vx - q->p[2].vx;
		if (dx < 0) dx = -dx;
		if (dx > 400 || star_n >= 512)
			continue;
		star_pos[star_n].vx = (q->p[0].vx + q->p[1].vx + q->p[2].vx + q->p[3].vx) / 16;
		star_pos[star_n].vy = (q->p[0].vy + q->p[1].vy + q->p[2].vy + q->p[3].vy) / 16;
		star_pos[star_n].vz = (q->p[0].vz + q->p[1].vz + q->p[2].vz + q->p[3].vz) / 16;
		star_col[star_n] = q->r;
		star_n++;
	}
	SetDispMask(1);
	last_vblank = VSync(-1);
}

void gfx_load_atlas(const uint16_t *pixels) {
	RECT r = { ATLAS_X, 0, 256, 256 };
	LoadImage(&r, (const uint32_t *) pixels);
	DrawSync(0);
}

void gfx_begin(void) {
	Frame *f = &frames[cur];
	ClearOTagR(f->ot, OT_LEN);
	ot = f->ot;
	nextpri = f->pri;
	primend = f->pri + PRIMBUF_SIZE;
	hud_n = 0;
	gfx_stat_polys = gfx_stat_ml = gfx_stat_v = gfx_stat_q = gfx_stat_skip = 0;
}

int gfx_end(void) {
	Frame *f = &frames[cur];
	for (int i = hud_n - 1; i >= 0; i--)
		addPrim(&ot[0], hud_prims[i]);
	DrawSync(0);
	VSync(0);
	int now = VSync(-1);
	int vb = now - last_vblank;
	last_vblank = now;
	setRGB0(&f->draw, fog_rgb[0], fog_rgb[1], fog_rgb[2]);
	PutDispEnv(&f->disp);
	PutDrawEnv(&f->draw);
	DrawOTag(&f->ot[OT_LEN - 1]);
	cur ^= 1;
	return vb < 1 ? 1 : vb;
}

// ------------------------------------------------------------------ camera, fog

static void cross(const int a[3], const int b[3], int out[3]) {
	out[0] = (a[1] * b[2] - a[2] * b[1]) >> 12;
	out[1] = (a[2] * b[0] - a[0] * b[2]) >> 12;
	out[2] = (a[0] * b[1] - a[1] * b[0]) >> 12;
}

void gfx_camera(const V3 *e, const int16_t d[3], const int16_t u[3]) {
	int f[3] = { d[0], d[1], d[2] }, up[3] = { u[0], u[1], u[2] }, r[3], dn[3];
	normalize3(f);
	cross(f, up, r);
	normalize3(r);
	cross(r, f, up);   // true up
	dn[0] = -up[0], dn[1] = -up[1], dn[2] = -up[2];
	for (int i = 0; i < 3; i++) {
		view.m[0][i] = r[i];
		view.m[1][i] = dn[i];
		view.m[2][i] = f[i];
	}
	eye = *e;
	eye_u.x = e->x >> 2, eye_u.y = e->y >> 2, eye_u.z = e->z >> 2;
}

const MATRIX *gfx_view(void) {
	return &view;
}

const V3 *gfx_eye(void) {
	return &eye;
}

void gfx_fog(const uint8_t rgb[3], int start, int end, int lamp) {
	for (int i = 0; i < 3; i++) {
		fog_rgb[i] = rgb[i];
		fog_tex[i] = rgb[i];
	}
	// Depths are in world units (64 per block); start/end are blocks in 8.8.
	int s = start >> 2, e = end >> 2;
	if (e <= s) e = s + 1;
	far_z = e;
	for (int i = 0; i < 256; i++) {
		int z = (i << 4) + 8;
		int t = z <= s ? 0 : z >= e ? 256 : ((z - s) << 8) / (e - s);
		fog_lut[i] = t >= 256 ? 255 : (t * t * (768 - 2 * t)) >> 16;   // smoothstep
		// Lantern: (1 - d / 20 blocks)^2.
		int k = 256 - (z << 8) / (20 * UNIT);
		if (k < 0) k = 0;
		lamp_lut[i] = (((k * k) >> 8) * lamp) >> 8;
		fog_keep[i] = 256 - fog_lut[i];
		for (int c = 0; c < 3; c++) {
			fog_add[i][c] = (fog_tex[c] * fog_lut[i]) >> 8;
			fog_add16[i][c] = fog_add[i][c] << 4;
		}
	}
}

// Fogs and lights a colour (r, g, b on the 128 = 1.0 scale, lantern response) at
// depth z, returning it packed with the GT4 command byte. The multiplying is done
// by the GTE's GPL (MAC += IR * IR0): first adding the lantern's warm light, then
// blending toward the fog colour. The GTE's RGBC register must hold the command
// byte (see gfx_draw_meshlet).
#define GPL "cop2 0x01A8003E\n"
static inline uint32_t shade_vertex(const uint8_t *in, int z) {
	int i = z >> 4;
	if (i > 255) i = 255;
	int lamp = lamp_lut[i], keep = fog_keep[i];
	int r = in[0] << 4, g = in[1] << 4, b = in[2] << 4;
	uint32_t out;
	if (lamp) {
		int lk = (lamp * in[3]) >> 4;
		// IR = the lantern's colour (1, 0.75, 0.5 of full), MAC = the colour.
		__asm__ volatile (
			"mtc2 %1, $9\n mtc2 %2, $10\n mtc2 %3, $11\n"
			"mtc2 %4, $25\n mtc2 %5, $26\n mtc2 %6, $27\n"
			"mtc2 %7, $8\n nop\n nop\n" GPL
			: "=r"(out) : "r"(2032), "r"(1524), "r"(1016), "r"(r), "r"(g), "r"(b), "r"(lk));
		if (keep == 256) {
			__asm__ volatile ("mfc2 %0, $22\n nop\n" : "=r"(out));
			return out;
		}
	} else {
		if (keep == 256) {
			r >>= 4, g >>= 4, b >>= 4;
			return (r > 255 ? 255 : r) | (g > 255 ? 255 : g) << 8 | (b > 255 ? 255 : b) << 16 | 0x3c000000;
		}
		__asm__ volatile ("mtc2 %0, $9\n mtc2 %1, $10\n mtc2 %2, $11\n" : : "r"(r), "r"(g), "r"(b));
	}
	// The (lit) colour is in IR now; MAC = fog colour * fog, IR0 = how much stays.
	__asm__ volatile (
		"mtc2 %1, $25\n mtc2 %2, $26\n mtc2 %3, $27\n"
		"mtc2 %4, $8\n nop\n nop\n" GPL
		"mfc2 %0, $22\n nop\n"
		: "=r"(out) : "r"(fog_add16[i][0]), "r"(fog_add16[i][1]), "r"(fog_add16[i][2]), "r"(keep << 4));
	return out;
}

// Same for untextured colours (255 = 1.0), with a lantern response 0..256.
static inline void shade_flat(int *c, int z, int lamp_resp) {
	int i = z >> 4;
	if (i > 255) i = 255;
	int lk = (lamp_lut[i] * lamp_resp) >> 8;
	c[0] += lk;
	c[1] += (lk * 200) >> 8;
	c[2] += lk >> 1;
	int f = fog_lut[i];
	for (int k = 0; k < 3; k++) {
		c[k] += ((fog_rgb[k] - c[k]) * f) >> 8;
		if (c[k] > 255) c[k] = 255;
		if (c[k] < 0) c[k] = 0;
	}
}

// Sets the GTE to draw things whose local origin is at `p` (fine units), with
// local axes `m` (4.12, NULL = world axes) and local units of 1/`div` world unit.
static void set_transform(const V3 *p, const MATRIX *m, int div_shift) {
	MATRIX c;
	if (m) {
		for (int i = 0; i < 3; i++)
			for (int j = 0; j < 3; j++)
				c.m[i][j] = (view.m[i][0] * m->m[0][j] + view.m[i][1] * m->m[1][j] + view.m[i][2] * m->m[2][j]) >> (12 + div_shift);
	} else {
		c = view;
	}
	int dx = p->x - eye.x, dy = p->y - eye.y, dz = p->z - eye.z;
	// (Differences can reach 100 blocks = 25600 fine units; keep products in range.)
	for (int i = 0; i < 3; i++)
		c.t[i] = ((view.m[i][0] * (dx >> 2)) + (view.m[i][1] * (dy >> 2)) + (view.m[i][2] * (dz >> 2))) >> 12;
	GTE_SYNC();
	gte_SetRotMatrix(&c);
	gte_SetTransMatrix(&c);
}

int gfx_project(const V3 *p, int *sx, int *sy) {
	set_transform(p, 0, 0);
	SVECTOR o = { 0, 0, 0, 0 };
	DVECTOR s;
	int z;
	GTE_SYNC();
	gte_ldv0(&o);
	gte_rtps();
	gte_stsxy(&s);
	gte_stsz(&z);
	VECTOR mac;
	gte_stlvnl(&mac);
	if (mac.vz <= 2)
		return 0;
	*sx = s.vx, *sy = s.vy;
	return z;
}

// ------------------------------------------------------------------ world

void gfx_world_palette(const uint8_t *p) {
	palette = p;
}

typedef struct {
	SVECTOR p;
	int16_t u, v;            // texels * 16
	uint8_t c[4];
} SubV;

// Links a primitive into the OT. With `win` (a texture window command, for faces
// whose tile repeats) the primitive is wrapped in setting and clearing the window.
static void link(void *p, int otz, uint32_t win) {
	if (otz < 1) otz = 1;
	if (otz > OT_LEN - 2) otz = OT_LEN - 2;
	if (win) {
		uint32_t *off = alloc(8);
		if (!off)
			return;
		off[1] = 0xe2000000;
		setlen(off, 1);
		addPrim(&ot[otz], off);
	}
	addPrim(&ot[otz], p);
	if (win) {
		uint32_t *on = alloc(8);
		if (!on)
			return;
		on[1] = win;
		setlen(on, 1);
		addPrim(&ot[otz], on);
	}
	gfx_stat_polys++;
}

// The texture window that repeats a 16x16 tile.
static inline uint32_t tile_window(int tile) {
	return 0xe2000000 | 0x1e | 0x1e << 5 | ((tile & 15) << 1) << 10 | ((tile >> 4) << 1) << 15;
}

static void emit_gt4(const DVECTOR *s0, const DVECTOR *s1, const DVECTOR *s2, const DVECTOR *s3,
		const uint8_t *c0, const uint8_t *c1, const uint8_t *c2, const uint8_t *c3,
		const int16_t *uv0, const int16_t *uv1, const int16_t *uv2, const int16_t *uv3, int otz, uint32_t win) {
	POLY_GT4 *p = alloc(sizeof(POLY_GT4));
	if (!p)
		return;
	setPolyGT4(p);
	// Our corners go around the face; the GPU wants them as two rows (0 1 / 3 2).
	setXY4(p, s0->vx, s0->vy, s1->vx, s1->vy, s3->vx, s3->vy, s2->vx, s2->vy);
	setRGB0(p, c0[0], c0[1], c0[2]);
	setRGB1(p, c1[0], c1[1], c1[2]);
	setRGB2(p, c3[0], c3[1], c3[2]);
	setRGB3(p, c2[0], c2[1], c2[2]);
	setUV4(p, uv0[0] >> 4, uv0[1] >> 4, uv1[0] >> 4, uv1[1] >> 4, uv3[0] >> 4, uv3[1] >> 4, uv2[0] >> 4, uv2[1] >> 4);
	p->tpage = tp_atlas;
	p->clut = 0;
	link(p, otz, win);
}

static inline int big(int a, int b, int c, int d) {
	int lo = a, hi = a;
	if (b < lo) lo = b; if (b > hi) hi = b;
	if (c < lo) lo = c; if (c > hi) hi = c;
	if (d < lo) lo = d; if (d > hi) hi = d;
	return hi - lo;
}

static inline int facing(const DVECTOR *a, const DVECTOR *b, const DVECTOR *c) {
	return (b->vx - a->vx) * (c->vy - a->vy) - (c->vx - a->vx) * (b->vy - a->vy);
}

// Draws a face given in local coordinates (the GTE transform must be set),
// splitting it in four while it crosses the near plane or is very large.
static void draw_split(const SubV *q, int depth, uint32_t win) {
	DVECTOR s[4];
	int32_t z[4];
	GTE_SYNC();
	gte_ldv3(&q[0].p, &q[1].p, &q[2].p);
	gte_rtpt();
	gte_stsxy3(&s[0], &s[1], &s[2]);
	gte_stsz3(&z[0], &z[1], &z[2]);
	gte_ldv0(&q[3].p);
	gte_rtps();
	gte_stsxy(&s[3]);
	gte_stsz(&z[3]);
	int zmin = z[0], zmax = z[0];
	for (int i = 1; i < 4; i++) {
		if (z[i] < zmin) zmin = z[i];
		if (z[i] > zmax) zmax = z[i];
	}
	if (zmax < NEAR_Z || zmin > far_z)
		return;
	int crossing = zmin < NEAR_Z;
	int w = big(s[0].vx, s[1].vx, s[2].vx, s[3].vx), h = big(s[0].vy, s[1].vy, s[2].vy, s[3].vy);
	if (!crossing && facing(&s[0], &s[1], &s[3]) >= 0 && facing(&s[1], &s[2], &s[3]) >= 0)
		return;
	if (crossing || w > SPLIT_SIZE || h > SPLIT_SIZE) {
		if (depth <= 0) {
			if (crossing || w > 1000 || h > 500)
				return;
		} else {
			SubV m[9];
			// 0 1 2 / 7 8 3 / 6 5 4 around the original corners q0 q1 q2 q3.
			const SubV *c[4] = { &q[0], &q[1], &q[2], &q[3] };
			for (int k = 0; k < 4; k++) {
				const SubV *a = c[k], *b = c[(k + 1) & 3];
				SubV *o = &m[k * 2];
				*o = *a;
				SubV *mid = &m[k * 2 + 1];
				mid->p.vx = (a->p.vx + b->p.vx) >> 1;
				mid->p.vy = (a->p.vy + b->p.vy) >> 1;
				mid->p.vz = (a->p.vz + b->p.vz) >> 1;
				mid->u = (a->u + b->u) >> 1;
				mid->v = (a->v + b->v) >> 1;
				for (int i = 0; i < 3; i++)
					mid->c[i] = (a->c[i] + b->c[i]) >> 1;
			}
			SubV *ce = &m[8];
			ce->p.vx = (m[1].p.vx + m[5].p.vx) >> 1;
			ce->p.vy = (m[1].p.vy + m[5].p.vy) >> 1;
			ce->p.vz = (m[1].p.vz + m[5].p.vz) >> 1;
			ce->u = (m[1].u + m[5].u) >> 1;
			ce->v = (m[1].v + m[5].v) >> 1;
			for (int i = 0; i < 3; i++)
				ce->c[i] = (m[1].c[i] + m[5].c[i]) >> 1;
			SubV quad[4];
			static const uint8_t parts[4][4] = { { 0, 1, 8, 7 }, { 1, 2, 3, 8 }, { 8, 3, 4, 5 }, { 7, 8, 5, 6 } };
			for (int k = 0; k < 4; k++) {
				for (int i = 0; i < 4; i++)
					quad[i] = m[parts[k][i]];
				draw_split(quad, depth - 1, win);
			}
			return;
		}
	}
	if ((s[0].vx < 0 && s[1].vx < 0 && s[2].vx < 0 && s[3].vx < 0) ||
		(s[0].vx >= SCREEN_W && s[1].vx >= SCREEN_W && s[2].vx >= SCREEN_W && s[3].vx >= SCREEN_W) ||
		(s[0].vy < 0 && s[1].vy < 0 && s[2].vy < 0 && s[3].vy < 0) ||
		(s[0].vy >= SCREEN_H && s[1].vy >= SCREEN_H && s[2].vy >= SCREEN_H && s[3].vy >= SCREEN_H))
		return;
	int16_t uv[4][2];
	for (int i = 0; i < 4; i++)
		uv[i][0] = q[i].u, uv[i][1] = q[i].v;
	emit_gt4(&s[0], &s[1], &s[2], &s[3], q[0].c, q[1].c, q[2].c, q[3].c, uv[0], uv[1], uv[2], uv[3],
		(z[0] + z[1] + z[2] + z[3]) >> (2 + OT_SHIFT), win);
}

void gfx_draw_meshlet(const MeshletHeader *m) {
	// Cull by bounding sphere against the view volume and the fog.
	int hx = (m->max[0] - m->min[0]) >> 1, hy = (m->max[1] - m->min[1]) >> 1, hz = (m->max[2] - m->min[2]) >> 1;
	int cx = m->ox + m->min[0] + hx - eye_u.x;
	int cy = m->oy + m->min[1] + hy - eye_u.y;
	int cz = m->oz + m->min[2] + hz - eye_u.z;
	int rad = hx > hy ? hx : hy;
	if (hz > rad) rad = hz;
	rad = (rad * 3 + hx + hy + hz) >> 2;   // a little over the true half-diagonal
	rad += rad >> 2;
	int vz = (view.m[2][0] * cx + view.m[2][1] * cy + view.m[2][2] * cz) >> 12;
	if (vz + rad < NEAR_Z || vz - rad > far_z)
		return;
	int vx = (view.m[0][0] * cx + view.m[0][1] * cy + view.m[0][2] * cz) >> 12;
	int vy = (view.m[1][0] * cx + view.m[1][1] * cy + view.m[1][2] * cz) >> 12;
	if (vx < 0) vx = -vx;
	if (vy < 0) vy = -vy;
	if (vx - ((rad * 1400) >> 10) > ((vz * 960) >> 10) || vy - ((rad * 1260) >> 10) > ((vz * 720) >> 10))
		return;

	gfx_stat_ml++;
	gfx_stat_v += m->nv;
	V3 o = { m->ox << 2, m->oy << 2, m->oz << 2 };
	set_transform(&o, 0, 0);
	__asm__ volatile ("mtc2 %0, $6\n" : : "r"(0x3c000000));
	const MVert *v = (const MVert *) (m + 1);
	const MQuad *q = (const MQuad *) (v + m->nv);
	int nv = m->nv;
	int32_t *z = SCRATCH_Z;
	int i = 0;
	for (; i + 3 <= nv; i += 3) {
		gte_ldv3(&v[i], &v[i + 1], &v[i + 2]);
		gte_rtpt();
		gte_stsxy3(&sxy[i], &sxy[i + 1], &sxy[i + 2]);
		gte_stsz3(&z[i], &z[i + 1], &z[i + 2]);
	}
	for (; i < nv; i++) {
		gte_ldv0(&v[i]);
		gte_rtps();
		gte_stsxy(&sxy[i]);
		gte_stsz(&z[i]);
	}
	// Vertex colours are worked out the first time a face that uses them is drawn.
	static uint16_t stamp;
	static uint16_t vstamp[MAX_VERTS];
	stamp++;
	if (!stamp) {
		memset(vstamp, 0, sizeof(vstamp));
		stamp = 1;
	}
	// Whole groups of faces that all face away from the eye are skipped.
	int ex = eye_u.x - m->ox, ey = eye_u.y - m->oy, ez = eye_u.z - m->oz;
	uint8_t skip[8] = {
		ex <= m->min[0], ex >= m->max[0], ey <= m->min[1], ey >= m->max[1], ez <= m->min[2], ez >= m->max[2], 0,
		vz - rad > DETAIL_Z
	};
	int far = far_z;
	uint32_t tpw = (uint32_t) tp_atlas << 16;
	const uint16_t *uv_ptr;
	for (int g = 0; g < 8; g++) {
	int count = m->group[g];
	if (skip[g]) {
		gfx_stat_skip += count;
		q += count;
		continue;
	}
	gfx_stat_q += count;
	for (int k = count; k > 0; k--, q++) {
		int a = q->v[0], b = q->v[1], c = q->v[2], d = q->v[3];
		int za = z[a], zb = z[b], zc = z[c], zd = z[d];
		if (za > far && zb > far && zc > far && zd > far)
			continue;
		int near = za < NEAR_Z || zb < NEAR_Z || zc < NEAR_Z || zd < NEAR_Z;
		int tx = (q->tile & 15) << 4, ty = (q->tile >> 4) << 4;
		// Merged faces repeat their tile: texture coordinates run past it and the
		// texture window wraps them.
		uint32_t win = 0;
		int ue = 15, ve = 15;
		if (q->rep != 0x11) {
			win = tile_window(q->tile);
			ue = (q->rep & 15) * 16 - 1, ve = (q->rep >> 4) * 16 - 1;
			tx = ty = 0;
		}
		if (near) {
			if (za < NEAR_Z && zb < NEAR_Z && zc < NEAR_Z && zd < NEAR_Z)
				continue;
			SubV s[4];
			const int idx[4] = { a, b, c, d };
			for (int i = 0; i < 4; i++) {
				int j = idx[i];
				const MVert *mv = &v[j];
				s[i].p.vx = mv->x, s[i].p.vy = mv->y, s[i].p.vz = mv->z;
				s[i].u = (tx + ((q->uv >> (i * 2)) & 1) * ue) << 4;
				s[i].v = (ty + ((q->uv >> (i * 2 + 1)) & 1) * ve) << 4;
				if (vstamp[j] != stamp) {
					vcol[j] = shade_vertex(&palette[mv->col * 4], z[j]);
					vstamp[j] = stamp;
				}
				memcpy(s[i].c, &vcol[j], 4);
			}
			draw_split(s, MAX_SPLIT, win);
			continue;
		}
		const DVECTOR *sa = &sxy[a], *sb = &sxy[b], *sd = &sxy[d];
		if (facing(sa, sb, sd) >= 0)
			continue;
		if (vstamp[a] != stamp) vcol[a] = shade_vertex(&palette[v[a].col * 4], za), vstamp[a] = stamp;
		if (vstamp[b] != stamp) vcol[b] = shade_vertex(&palette[v[b].col * 4], zb), vstamp[b] = stamp;
		if (vstamp[c] != stamp) vcol[c] = shade_vertex(&palette[v[c].col * 4], zc), vstamp[c] = stamp;
		if (vstamp[d] != stamp) vcol[d] = shade_vertex(&palette[v[d].col * 4], zd), vstamp[d] = stamp;
		uint32_t *w = alloc(sizeof(POLY_GT4));
		if (!w)
			return;
		// Our corners go around the face; the GPU wants them as two rows (a b / d c).
		const uint32_t *xy = (const uint32_t *) sxy;
		uint32_t base = tx | ty << 8;
		if (win) {
			// (Merged faces aren't common enough to bother with the table.)
			static uint16_t big_uv[4];
			static const uint8_t order[4] = { 0, 1, 3, 2 };
			for (int k = 0; k < 4; k++) {
				int c = order[k];
				big_uv[k] = ((q->uv >> (c * 2)) & 1) * ue | (((q->uv >> (c * 2 + 1)) & 1) * ve) << 8;
			}
			uv_ptr = big_uv;
		} else {
			uv_ptr = uv_lut[q->uv];
		}
		const uint16_t *uv = uv_ptr;
		w[1] = vcol[a];
		w[2] = xy[a];
		w[3] = base + uv[0];
		w[4] = vcol[b];
		w[5] = xy[b];
		w[6] = (base + uv[1]) | tpw;
		w[7] = vcol[d];
		w[8] = xy[d];
		w[9] = base + uv[2];
		w[10] = vcol[c];
		w[11] = xy[c];
		w[12] = base + uv[3];
		int otz = (za + zb + zc + zd) >> (2 + OT_SHIFT);
		if (win) {
			setlen(w, 12);
			link(w, otz, win);
			continue;
		}
		if (otz < 1) otz = 1;
		if (otz > OT_LEN - 2) otz = OT_LEN - 2;
		w[0] = (ot[otz] & 0x00ffffff) | 12 << 24;
		ot[otz] = (uint32_t) w & 0x00ffffff;
		gfx_stat_polys++;
	}
	}
}

void gfx_draw_sky(int with_moon) {
	// Everything at a quarter scale around a quartered eye (see gfx_init).
	V3 e4 = { eye.x >> 2, eye.y >> 2, eye.z >> 2 };
	MATRIX c = view;
	for (int i = 0; i < 3; i++)
		c.t[i] = -((view.m[i][0] * (e4.x >> 2)) + (view.m[i][1] * (e4.y >> 2)) + (view.m[i][2] * (e4.z >> 2))) >> 12;
	GTE_SYNC();
	gte_SetRotMatrix(&c);
	gte_SetTransMatrix(&c);
	for (int i = 0; i < star_n; i++) {
		DVECTOR s;
		VECTOR mac;
		gte_ldv0(&star_pos[i]);
		gte_rtps();
		gte_stsxy(&s);
		gte_stlvnl(&mac);
		if (mac.vz < 64 || s.vx < 0 || s.vx >= SCREEN_W || s.vy < 0 || s.vy >= SCREEN_H)
			continue;
		TILE *t = alloc(sizeof(TILE));
		if (!t)
			return;
		setTile(t);
		int b = star_col[i] * 2;
		if (b > 255) b = 255;
		setRGB0(t, b, b, (b * 230) >> 8);
		setXY0(t, s.vx, s.vy);
		setWH(t, b > 200 ? 2 : 1, b > 200 ? 2 : 1);
		addPrim(&ot[OT_LEN - 1], t);
	}
	if (!with_moon)
		return;
	for (int i = 0; i < sky_n; i++) {
		const SkyQuad *q = &sky[i];
		int dx = q->p[0].vx - q->p[2].vx;
		if (dx < 0) dx = -dx;
		if (dx <= 400)
			continue;   // a star
		SVECTOR p[4];
		DVECTOR s[4];
		int32_t z[4];
		for (int k = 0; k < 4; k++)
			p[k].vx = q->p[k].vx >> 2, p[k].vy = q->p[k].vy >> 2, p[k].vz = q->p[k].vz >> 2;
		GTE_SYNC();
		gte_ldv3(&p[0], &p[1], &p[2]);
		gte_rtpt();
		gte_stsxy3(&s[0], &s[1], &s[2]);
		gte_stsz3(&z[0], &z[1], &z[2]);
		gte_ldv0(&p[3]);
		gte_rtps();
		gte_stsxy(&s[3]);
		gte_stsz(&z[3]);
		if (z[0] < 64 || z[1] < 64 || z[2] < 64 || z[3] < 64)
			continue;
		if (facing(&s[0], &s[1], &s[3]) >= 0)
			continue;
		POLY_FT4 *f = alloc(sizeof(POLY_FT4));
		if (!f)
			return;
		setPolyFT4(f);
		setXY4(f, s[0].vx, s[0].vy, s[1].vx, s[1].vy, s[3].vx, s[3].vy, s[2].vx, s[2].vy);
		int tx = (q->tile & 15) << 4, ty = (q->tile >> 4) << 4;
		int u[4], v[4];
		for (int k = 0; k < 4; k++) {
			u[k] = tx + ((q->uv >> (k * 2)) & 1) * 15;
			v[k] = ty + ((q->uv >> (k * 2 + 1)) & 1) * 15;
		}
		setUV4(f, u[0], v[0], u[1], v[1], u[3], v[3], u[2], v[2]);
		setRGB0(f, q->r, q->g, q->b);
		f->tpage = tp_atlas;
		f->clut = 0;
		addPrim(&ot[OT_LEN - 1], f);
	}
}

void gfx_light_at(const V3 *p, int out[3]) {
	const LightHeader *h = light;
	int x = ((p->x >> 8) - h->x0 + h->step / 2) / h->step;
	int y = ((p->y >> 8) - h->y0 + h->step / 2) / h->step;
	int z = ((p->z >> 8) - h->z0 + h->step / 2) / h->step;
	if (x < 0) x = 0;
	if (y < 0) y = 0;
	if (z < 0) z = 0;
	if (x >= h->nx) x = h->nx - 1;
	if (y >= h->ny) y = h->ny - 1;
	if (z >= h->nz) z = h->nz - 1;
	uint16_t v = ((const uint16_t *) (h + 1))[(z * h->ny + y) * h->nx + x];
	out[0] = (v & 31) << 8;
	out[1] = ((v >> 5) & 63) << 7;
	out[2] = ((v >> 11) & 31) << 8;
}

// ------------------------------------------------------------------ models and boxes

const ModelHeader *gfx_model(int model) {
	return models[model];
}

// Minecraft-style shade for a face normal (columns of `m`), 4.12.
static int face_shade(const MATRIX *m, int face) {
	int axis = face >> 1;
	int nx = m->m[0][axis], ny = m->m[1][axis], nz = m->m[2][axis];
	if (face & 1)
		nx = -nx, ny = -ny, nz = -nz;
	int len = isqrt(nx * nx + ny * ny + nz * nz);
	if (len == 0)
		return 3277;
	int ax = nx < 0 ? -nx : nx;
	int s = 3277 + ((819 * ny - 573 * ax) / len);
	return s < 1843 ? 1843 : s > 4096 ? 4096 : s;
}

static void emit_f4(const DVECTOR *s0, const DVECTOR *s1, const DVECTOR *s2, const DVECTOR *s3, const int c[3], int otz) {
	POLY_F4 *p = alloc(sizeof(POLY_F4));
	if (!p)
		return;
	setPolyF4(p);
	setXY4(p, s0->vx, s0->vy, s1->vx, s1->vy, s3->vx, s3->vy, s2->vx, s2->vy);
	setRGB0(p, c[0], c[1], c[2]);
	if (otz < 1) otz = 1;
	if (otz > OT_LEN - 2) otz = OT_LEN - 2;
	addPrim(&ot[otz], p);
	gfx_stat_polys++;
}

void gfx_draw_model(int mi, const MATRIX *m, const V3 *pos, const int light[3], int flash, const int *tint) {
	const ModelHeader *h = models[mi];
	const SVECTOR *v = (const SVECTOR *) (h + 1);
	const MFace *f = (const MFace *) (v + h->nv);
	set_transform(pos, m, 2);
	// Depth of the model's origin, for its fog and lantern light.
	int dx = (pos->x - eye.x) >> 2, dy = (pos->y - eye.y) >> 2, dz = (pos->z - eye.z) >> 2;
	int depth = (view.m[2][0] * dx + view.m[2][1] * dy + view.m[2][2] * dz) >> 12;
	if (depth < NEAR_Z || depth > far_z + 128)
		return;
	int nv = h->nv < MAX_VERTS ? h->nv : MAX_VERTS;
	for (int i = 0; i < nv; i += 3) {
		gte_ldv3(&v[i], &v[i + 1], &v[i + 2]);
		gte_rtpt();
		gte_stsxy3(&sxy[i], &sxy[i + 1], &sxy[i + 2]);
		gte_stsz3(&sz[i], &sz[i + 1], &sz[i + 2]);
	}
	// Each face's colour is base * mul + add, with mul and add worked out here for
	// every (glow, direction) pair: the PC's face_color, tint, flash, lantern and fog.
	int li = depth >> 4;
	if (li > 255) li = 255;
	int keep = 256 - fog_lut[li], fog = fog_lut[li], lamp = lamp_lut[li];
	int ng = n_glows[mi];
	int mul[MAX_GLOWS][6][3], add[MAX_GLOWS][6][3];
	for (int d = 0; d < 6; d++) {
		int sh = face_shade(m, d);
		for (int g = 0; g < ng; g++) {
			int glow = glows[mi][g] << 4;
			if (flash > glow)
				glow = flash;
			int gl = (glow * (3072 + (sh >> 2))) >> 12;
			int resp = ((4096 - glow) * sh) >> 12;
			if (resp < 0) resp = 0;
			int lk = (lamp * resp) >> 12;
			static const int warm[3] = { 256, 200, 128 };
			for (int k = 0; k < 3; k++) {
				int l = (light[k] * sh) >> 12;
				if (l < gl) l = gl;
				if (tint)
					l = (l * tint[k]) >> 12;
				// flash: base' = base * (1 - flash) + 255 * flash
				int mu = (l * (4096 - flash)) >> 12;
				int ad = (255 * ((l * flash) >> 12)) >> 12;
				ad += (lk * warm[k]) >> 8;
				mul[g][d][k] = (mu * keep) >> 8;
				add[g][d][k] = ((ad * keep) >> 8) + ((fog_rgb[k] * fog) >> 8);
			}
		}
	}
	for (int k = 0; k < h->nq; k++, f++) {
		int a = f->v[0], b = f->v[1], c = f->v[2], d = f->v[3];
		int za = sz[a], zb = sz[b], zc = sz[c], zd = sz[d];
		if (za < NEAR_Z || zb < NEAR_Z || zc < NEAR_Z || zd < NEAR_Z)
			continue;
		if (facing(&sxy[a], &sxy[b], &sxy[d]) >= 0)
			continue;
		const int *mu = mul[f->pad][f->face], *ad = add[f->pad][f->face];
		int col[3];
		col[0] = ((f->r * mu[0]) >> 12) + ad[0];
		col[1] = ((f->g * mu[1]) >> 12) + ad[1];
		col[2] = ((f->b * mu[2]) >> 12) + ad[2];
		if (col[0] > 255) col[0] = 255;
		if (col[1] > 255) col[1] = 255;
		if (col[2] > 255) col[2] = 255;
		emit_f4(&sxy[a], &sxy[b], &sxy[c], &sxy[d], col, (za + zb + zc + zd) >> (2 + OT_SHIFT));
	}
}

// Unit cube corners (world units, half a block) and faces in FACES order.
static const SVECTOR cube_v[8] = {
	{ -32, -32, -32, 0 }, { 32, -32, -32, 0 }, { 32, 32, -32, 0 }, { -32, 32, -32, 0 },
	{ -32, -32, 32, 0 }, { 32, -32, 32, 0 }, { 32, 32, 32, 0 }, { -32, 32, 32, 0 },
};
static const uint8_t cube_f[6][4] = {
	{ 5, 1, 2, 6 },   // +x
	{ 0, 4, 7, 3 },   // -x
	{ 7, 6, 2, 3 },   // +y
	{ 0, 1, 5, 4 },   // -y
	{ 4, 5, 6, 7 },   // +z
	{ 1, 0, 3, 2 },   // -z
};

void gfx_draw_box(const MATRIX *m, const V3 *pos, int tile, const uint8_t rgb[3]) {
	set_transform(pos, m, 0);
	DVECTOR s[8];
	int32_t z[8];
	gte_ldv3(&cube_v[0], &cube_v[1], &cube_v[2]);
	gte_rtpt();
	gte_stsxy3(&s[0], &s[1], &s[2]);
	gte_stsz3(&z[0], &z[1], &z[2]);
	gte_ldv3(&cube_v[3], &cube_v[4], &cube_v[5]);
	gte_rtpt();
	gte_stsxy3(&s[3], &s[4], &s[5]);
	gte_stsz3(&z[3], &z[4], &z[5]);
	gte_ldv0(&cube_v[6]);
	gte_rtps();
	gte_stsxy(&s[6]);
	gte_stsz(&z[6]);
	gte_ldv0(&cube_v[7]);
	gte_rtps();
	gte_stsxy(&s[7]);
	gte_stsz(&z[7]);
	for (int i = 0; i < 8; i++)
		if (z[i] < NEAR_Z || z[i] > far_z + 256)
			return;
	for (int fi = 0; fi < 6; fi++) {
		const uint8_t *f = cube_f[fi];
		if (facing(&s[f[0]], &s[f[1]], &s[f[3]]) >= 0 && facing(&s[f[1]], &s[f[2]], &s[f[3]]) >= 0)
			continue;
		int sh = face_shade(m, fi);
		int c[3] = { (rgb[0] * sh) >> 12, (rgb[1] * sh) >> 12, (rgb[2] * sh) >> 12 };
		int zz = (z[f[0]] + z[f[1]] + z[f[2]] + z[f[3]]) >> 2;
		shade_flat(c, zz, 0);
		if (tile < 0) {
			emit_f4(&s[f[0]], &s[f[1]], &s[f[2]], &s[f[3]], c, zz >> OT_SHIFT);
			continue;
		}
		POLY_FT4 *p = alloc(sizeof(POLY_FT4));
		if (!p)
			return;
		setPolyFT4(p);
		setXY4(p, s[f[0]].vx, s[f[0]].vy, s[f[1]].vx, s[f[1]].vy, s[f[3]].vx, s[f[3]].vy, s[f[2]].vx, s[f[2]].vy);
		int tx = (tile & 15) << 4, ty = (tile >> 4) << 4;
		// Same orientation as push_quad: corner 0 at the bottom left of the tile.
		setUV4(p, tx, ty + 15, tx + 15, ty + 15, tx, ty, tx + 15, ty);
		setRGB0(p, c[0] >> 1, c[1] >> 1, c[2] >> 1);
		p->tpage = tp_atlas;
		p->clut = 0;
		int otz = zz >> OT_SHIFT;
		if (otz < 1) otz = 1;
		if (otz > OT_LEN - 2) otz = OT_LEN - 2;
		addPrim(&ot[otz], p);
		gfx_stat_polys++;
	}
}

void gfx_draw_spark(const V3 *pos, int size, const uint8_t rgb[3]) {
	int sx, sy;
	int z = gfx_project(pos, &sx, &sy);
	if (z < 48 || z > far_z)
		return;
	int px = ((size >> 2) * PROJ_H) / z;
	if (px < 1) px = 1;
	if (px > 8) px = 8;
	if (sx + px < 0 || sx - px >= SCREEN_W || sy + px < 0 || sy - px >= SCREEN_H)
		return;
	int c[3] = { rgb[0], rgb[1], rgb[2] };
	shade_flat(c, z, 0);
	TILE *t = alloc(sizeof(TILE));
	if (!t)
		return;
	setTile(t);
	setRGB0(t, c[0], c[1], c[2]);
	setXY0(t, sx - px / 2, sy - px / 2);
	setWH(t, px, px);
	int otz = z >> OT_SHIFT;
	if (otz > OT_LEN - 2) otz = OT_LEN - 2;
	addPrim(&ot[otz], t);
}

void gfx_puffs_begin(void) {
	V3 o = { 0, 0, 0 };
	set_transform(&o, 0, 0);
}

void gfx_puff(const SVECTOR *pos, int hw, int hh, const uint8_t rgb[3]) {
	DVECTOR s;
	int z;
	gte_ldv0(pos);
	gte_rtps();
	gte_stsxy(&s);
	gte_stsz(&z);
	if (z < 64 || z > far_z)
		return;
	int w = (hw * PROJ_H) / z, h = (hh * PROJ_H) / z;
	int sx = s.vx, sy = s.vy;
	if (w < 1 || h < 1 || w > 300 || h > 200)
		return;
	if (sx + w < 0 || sx - w >= SCREEN_W || sy + h < 0 || sy - h >= SCREEN_H)
		return;
	// Fade with the fog and right in front of the camera.
	int i = z >> 4;
	if (i > 255) i = 255;
	int k = 255 - fog_lut[i];
	if (z < 384)
		k = (k * (z - 64)) / 320;
	POLY_FT4 *p = alloc(sizeof(POLY_FT4));
	if (!p)
		return;
	setPolyFT4(p);
	setSemiTrans(p, 1);
	setXY4(p, sx - w, sy - h, sx + w, sy - h, sx - w, sy + h, sx + w, sy + h);
	setUV4(p, 0, 0, 15, 0, 0, 15, 15, 15);
	setRGB0(p, (rgb[0] * k) >> 9, (rgb[1] * k) >> 9, (rgb[2] * k) >> 9);
	p->tpage = tp_puff;
	p->clut = 0;
	int otz = z >> OT_SHIFT;
	if (otz > OT_LEN - 2) otz = OT_LEN - 2;
	addPrim(&ot[otz], p);
}

// ------------------------------------------------------------------ HUD

static int glyph_scale(int scale) {
	return scale < 3072 ? 3072 : scale;
}

int hud_text_width(const char *s, int scale) {
	int n = strlen(s);
	return (n * 8 * glyph_scale(scale)) >> 12;
}

void hud_text(const char *s, int x, int y, int scale, const uint8_t rgb[3], int align) {
	scale = glyph_scale(scale);
	int w = hud_text_width(s, scale);
	if (align == ALIGN_CENTER)
		x -= w / 2;
	else if (align == ALIGN_RIGHT)
		x -= w;
	int size = (8 * scale) >> 12;
	int fx = x << 12;
	for (; *s; s++) {
		int ch = (uint8_t) *s;
		int gx = fx >> 12;
		fx += 8 * scale;
		if (ch == ' ' || ch >= 128)
			continue;
		int u = (ch & 31) * 8, v = 128 + (ch >> 5) * 8;
		if (gx + size < 0 || gx >= SCREEN_W || y + size < 0 || y >= SCREEN_H)
			continue;
		if (size == 8) {
			SPRT_8 *p = alloc(sizeof(SPRT_8));
			if (!p)
				return;
			setSprt8(p);
			setXY0(p, gx, y);
			setUV0(p, u, v);
			setRGB0(p, rgb[0] >> 1, rgb[1] >> 1, rgb[2] >> 1);
			p->clut = 0;
			// Sprites take the texture page from the draw mode, which the atlas
			// primitives leave set; set it anyway in case a mode change came first.
			DR_TPAGE *t = alloc(sizeof(DR_TPAGE));
			if (!t)
				return;
			setDrawTPage(t, 0, 1, tp_atlas);
			hud_add(t);
			hud_add(p);
		} else {
			POLY_FT4 *p = alloc(sizeof(POLY_FT4));
			if (!p)
				return;
			setPolyFT4(p);
			setXY4(p, gx, y, gx + size, y, gx, y + size, gx + size, y + size);
			setUV4(p, u, v, u + 7, v, u, v + 7, u + 7, v + 7);
			setRGB0(p, rgb[0] >> 1, rgb[1] >> 1, rgb[2] >> 1);
			p->tpage = tp_atlas;
			p->clut = 0;
			hud_add(p);
		}
	}
}

void hud_text_fit(const char *s, int x, int y, int scale, int max_w, const uint8_t rgb[3], int align) {
	int w = hud_text_width(s, 4096);
	if (w > 0 && ((w * scale) >> 12) > max_w)
		scale = (max_w << 12) / w;
	hud_text(s, x, y, scale, rgb, align);
}

static void mode(int abr) {
	DR_TPAGE *t = alloc(sizeof(DR_TPAGE));
	if (!t)
		return;
	setDrawTPage(t, 0, 1, getTPage(2, abr, ATLAS_X, 0));
	hud_add(t);
}

void hud_rect(int x, int y, int w, int h, const uint8_t rgb[3], int alpha) {
	if (w <= 0 || h <= 0 || alpha <= 0)
		return;
	TILE *t = alloc(sizeof(TILE));
	if (!t)
		return;
	setTile(t);
	setXY0(t, x, y);
	setWH(t, w, h);
	setRGB0(t, rgb[0], rgb[1], rgb[2]);
	if (alpha < 160) {
		mode(0);
		setSemiTrans(t, 1);
	}
	hud_add(t);
}

void hud_panel(int x, int y, int w, int h, const uint8_t fill[3], int alpha, const uint8_t border[3]) {
	hud_rect(x, y, w, h, fill, alpha);
	hud_rect(x, y, w, 1, border, 255);
	hud_rect(x, y + h - 1, w, 1, border, 255);
	hud_rect(x, y, 1, h, border, 255);
	hud_rect(x + w - 1, y, 1, h, border, 255);
}

void hud_sprite(int tile, int cx, int cy, int size, const uint8_t rgb[3]) {
	POLY_FT4 *p = alloc(sizeof(POLY_FT4));
	if (!p)
		return;
	int h = size / 2;
	int tx = (tile & 15) << 4, ty = (tile >> 4) << 4;
	setPolyFT4(p);
	setXY4(p, cx - h, cy - h, cx + h, cy - h, cx - h, cy + h, cx + h, cy + h);
	setUV4(p, tx, ty, tx + 15, ty, tx, ty + 15, tx + 15, ty + 15);
	setRGB0(p, rgb[0] >> 1, rgb[1] >> 1, rgb[2] >> 1);
	p->tpage = tp_atlas;
	p->clut = 0;
	hud_add(p);
}

void hud_darken(int amount) {
	static const uint8_t black[3] = { 0, 0, 0 };
	if (amount >= 200) {
		static const uint8_t dark[3] = { 10, 5, 18 };
		hud_rect(0, 0, SCREEN_W, SCREEN_H, dark, 255);
	} else if (amount > 0) {
		hud_rect(0, 0, SCREEN_W, SCREEN_H, black, 100);
		if (amount > 120)
			hud_rect(0, 0, SCREEN_W, SCREEN_H, black, 100);
	}
}

void hud_flash(void) {
	static const uint8_t white[3] = { 255, 255, 255 };
	hud_rect(0, 0, SCREEN_W, SCREEN_H, white, 255);
}
