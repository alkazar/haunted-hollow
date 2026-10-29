// Rendering on the Wii's GX: the world straight from the exported meshlets (all in
// memory), voxel models, boxes, sparks, mist and the HUD. The picture is 640x480;
// the HUD and everything the game works out on screen use 320x240 like the PS1, or
// 427x240 when the Wii is set to 16:9 (a wider view, as on the PC).
// Fog is the GPU's own; the rider's lantern is added to vertex colours here.
// Vertex colours are on the PS1's scale (128 = 1.0), doubled by the TEV.

#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <malloc.h>
#include <math.h>
#include <gccore.h>
#include "gfx.h"
#include "util.h"

#define FIFO_SIZE  (256 * 1024)
#define NEAR       0.1f
#define FAR        600.0f
// Lantern reach (blocks) and colour, as in the PC shader.
#define LAMP_RANGE 20.0f
// Small details further than this aren't drawn.
#define DETAIL_FAR 24.0f

int wii_screen_w = 320;

static GXRModeObj *rmode;
static void *xfb[2];
static int cur;
static volatile int retraces;
static int last_retrace;
static GXTexObj atlas_tex, puff_tex;
static uint8_t *atlas_gx;

static Mtx view;              // world (blocks) -> eye
static Mtx44 proj3d, proj2d;
static float eye[3], fwd[3], right[3], up[3];
static float tan_h, tan_v;
static GXColor fog_col;
static float fog_start = 20, fog_end = 46, lamp = 0.5f;
static const uint8_t *palette;
static int mode;              // 0 = 3D, 1 = HUD
static int blend_add;

static const ModelHeader *models[M_COUNT];
static const LightHeader *light;
static const SkyQuad *sky;
static int sky_n;

int gfx_stat_polys, gfx_stat_ml, gfx_stat_v, gfx_stat_q, gfx_stat_skip;

int wii_refresh_rate(void) {
	return VIDEO_GetCurrentTvMode() == VI_PAL ? 50 : 60;
}

static void retrace(u32 count) {
	(void) count;
	retraces++;
}

// ------------------------------------------------------------------ textures

// RGBA8 (row by row) -> GX's RGBA8: 4x4 tiles of AR pairs then GB pairs.
static void to_gx_rgba8(const uint8_t *src, uint8_t *dst, int w, int h) {
	for (int ty = 0; ty < h; ty += 4) {
		for (int tx = 0; tx < w; tx += 4) {
			for (int y = 0; y < 4; y++) {
				for (int x = 0; x < 4; x++) {
					const uint8_t *p = &src[((ty + y) * w + tx + x) * 4];
					int i = y * 4 + x;
					dst[i * 2] = p[3];
					dst[i * 2 + 1] = p[0];
					dst[32 + i * 2] = p[1];
					dst[32 + i * 2 + 1] = p[2];
				}
			}
			dst += 64;
		}
	}
}

void gfx_load_atlas(const uint16_t *pixels) {
	atlas_gx = memalign(32, 256 * 256 * 4);
	to_gx_rgba8((const uint8_t *) pixels, atlas_gx, 256, 256);
	DCFlushRange(atlas_gx, 256 * 256 * 4);
	GX_InitTexObj(&atlas_tex, atlas_gx, 256, 256, GX_TF_RGBA8, GX_CLAMP, GX_CLAMP, GX_FALSE);
	GX_InitTexObjLOD(&atlas_tex, GX_NEAR, GX_NEAR, 0, 0, 0, GX_FALSE, GX_FALSE, GX_ANISO_1);
	GX_LoadTexObj(&atlas_tex, GX_TEXMAP0);
}

static void make_puff_texture(void) {
	static uint8_t rgba[32 * 32 * 4];
	for (int y = 0; y < 32; y++) {
		for (int x = 0; x < 32; x++) {
			float dx = (x + 0.5f) / 16.0f - 1.0f, dy = (y + 0.5f) / 16.0f - 1.0f;
			float a = 1.0f - (dx * dx + dy * dy);
			a = a < 0 ? 0 : a * a;
			uint8_t *p = &rgba[(y * 32 + x) * 4];
			p[0] = p[1] = p[2] = 255;
			p[3] = (uint8_t) (a * 255);
		}
	}
	uint8_t *gx = memalign(32, sizeof(rgba));
	to_gx_rgba8(rgba, gx, 32, 32);
	DCFlushRange(gx, sizeof(rgba));
	GX_InitTexObj(&puff_tex, gx, 32, 32, GX_TF_RGBA8, GX_CLAMP, GX_CLAMP, GX_FALSE);
}

// ------------------------------------------------------------------ setup

void gfx_init(void) {
	VIDEO_Init();
	rmode = VIDEO_GetPreferredMode(NULL);
	for (int i = 0; i < 2; i++)
		xfb[i] = MEM_K0_TO_K1(SYS_AllocateFramebuffer(rmode));
	VIDEO_Configure(rmode);
	VIDEO_SetNextFramebuffer(xfb[0]);
	VIDEO_SetPostRetraceCallback(retrace);
	VIDEO_SetBlack(false);
	VIDEO_Flush();
	VIDEO_WaitVSync();
	if (rmode->viTVMode & VI_NON_INTERLACE)
		VIDEO_WaitVSync();

	void *fifo = MEM_K0_TO_K1(memalign(32, FIFO_SIZE));
	memset(fifo, 0, FIFO_SIZE);
	GX_Init(fifo, FIFO_SIZE);
	GXColor black = { 0, 0, 0, 255 };
	GX_SetCopyClear(black, 0x00ffffff);
	GX_SetViewport(0, 0, rmode->fbWidth, rmode->efbHeight, 0, 1);
	float yscale = GX_GetYScaleFactor(rmode->efbHeight, rmode->xfbHeight);
	int xfb_h = GX_SetDispCopyYScale(yscale);
	GX_SetScissor(0, 0, rmode->fbWidth, rmode->efbHeight);
	GX_SetDispCopySrc(0, 0, rmode->fbWidth, rmode->efbHeight);
	GX_SetDispCopyDst(rmode->fbWidth, xfb_h);
	GX_SetCopyFilter(rmode->aa, rmode->sample_pattern, GX_TRUE, rmode->vfilter);
	GX_SetFieldMode(rmode->field_rendering, (rmode->viHeight == 2 * rmode->xfbHeight) ? GX_ENABLE : GX_DISABLE);
	GX_SetPixelFmt(rmode->aa ? GX_PF_RGB565_Z16 : GX_PF_RGB8_Z24, GX_ZC_LINEAR);
	GX_SetDispCopyGamma(GX_GM_1_0);

	GX_ClearVtxDesc();
	GX_SetVtxDesc(GX_VA_POS, GX_DIRECT);
	GX_SetVtxDesc(GX_VA_CLR0, GX_DIRECT);
	GX_SetVtxDesc(GX_VA_TEX0, GX_DIRECT);
	GX_SetVtxAttrFmt(GX_VTXFMT0, GX_VA_POS, GX_POS_XYZ, GX_F32, 0);
	GX_SetVtxAttrFmt(GX_VTXFMT0, GX_VA_CLR0, GX_CLR_RGBA, GX_RGBA8, 0);
	GX_SetVtxAttrFmt(GX_VTXFMT0, GX_VA_TEX0, GX_TEX_ST, GX_F32, 0);
	GX_SetNumChans(1);
	GX_SetChanCtrl(GX_COLOR0A0, GX_DISABLE, GX_SRC_VTX, GX_SRC_VTX, 0, GX_DF_NONE, GX_AF_NONE);
	GX_SetNumTexGens(1);
	GX_SetTexCoordGen(GX_TEXCOORD0, GX_TG_MTX2x4, GX_TG_TEX0, GX_IDENTITY);
	GX_SetNumTevStages(1);
	GX_SetTevOrder(GX_TEVSTAGE0, GX_TEXCOORD0, GX_TEXMAP0, GX_COLOR0A0);
	// Colour = texture * vertex colour * 2; alpha = texture * vertex alpha.
	GX_SetTevColorIn(GX_TEVSTAGE0, GX_CC_ZERO, GX_CC_TEXC, GX_CC_RASC, GX_CC_ZERO);
	GX_SetTevColorOp(GX_TEVSTAGE0, GX_TEV_ADD, GX_TB_ZERO, GX_CS_SCALE_2, GX_TRUE, GX_TEVPREV);
	GX_SetTevAlphaIn(GX_TEVSTAGE0, GX_CA_ZERO, GX_CA_TEXA, GX_CA_RASA, GX_CA_ZERO);
	GX_SetTevAlphaOp(GX_TEVSTAGE0, GX_TEV_ADD, GX_TB_ZERO, GX_CS_SCALE_1, GX_TRUE, GX_TEVPREV);
	GX_SetAlphaCompare(GX_GREATER, 0, GX_AOP_AND, GX_ALWAYS, 0);
	GX_SetZCompLoc(GX_FALSE);
	GX_SetBlendMode(GX_BM_BLEND, GX_BL_SRCALPHA, GX_BL_INVSRCALPHA, GX_LO_CLEAR);
	GX_SetCullMode(GX_CULL_NONE);

	float aspect = 4.0f / 3.0f;
	if (CONF_GetAspectRatio() == CONF_ASPECT_16_9) {
		aspect = 16.0f / 9.0f;
		wii_screen_w = 427;
	}
	guPerspective(proj3d, 70.0f, aspect, NEAR, FAR);
	tan_v = tanf(35.0f * (float) M_PI / 180.0f);
	tan_h = tan_v * aspect;
	guOrtho(proj2d, 0, SCREEN_H, 0, SCREEN_W, 0, 1);
	make_puff_texture();

	const uint8_t *mb = models_bin;
	for (int i = 0; i < M_COUNT; i++)
		models[i] = (const ModelHeader *) (mb + ((const uint32_t *) (mb + 8))[i]);
	light = (const LightHeader *) light_bin;
	sky_n = *(const uint32_t *) (sky_bin + 4);
	sky = (const SkyQuad *) (sky_bin + 8);
	last_retrace = retraces;
}

static void set_blend(int add) {
	if (add == blend_add)
		return;
	blend_add = add;
	GX_SetBlendMode(GX_BM_BLEND, GX_BL_SRCALPHA, add ? GX_BL_ONE : GX_BL_INVSRCALPHA, GX_LO_CLEAR);
}

static void set_fog(int on) {
	if (on)
		GX_SetFog(GX_FOG_LIN, fog_start, fog_end, NEAR, FAR, fog_col);
	else
		GX_SetFog(GX_FOG_NONE, 0, 1, NEAR, FAR, fog_col);
}

// Back to drawing the world: perspective, world -> eye, depth and fog on.
static void mode_3d(void) {
	GX_LoadProjectionMtx(proj3d, GX_PERSPECTIVE);
	GX_LoadPosMtxImm(view, GX_PNMTX0);
	GX_SetZMode(GX_TRUE, GX_LEQUAL, GX_TRUE);
	GX_LoadTexObj(&atlas_tex, GX_TEXMAP0);
	set_blend(0);
	set_fog(1);
	mode = 0;
}

static void mode_hud(void) {
	if (mode == 1)
		return;
	Mtx id;
	guMtxIdentity(id);
	GX_LoadProjectionMtx(proj2d, GX_ORTHOGRAPHIC);
	GX_LoadPosMtxImm(id, GX_PNMTX0);
	GX_SetZMode(GX_FALSE, GX_ALWAYS, GX_FALSE);
	GX_LoadTexObj(&atlas_tex, GX_TEXMAP0);
	set_blend(0);
	set_fog(0);
	mode = 1;
}

void gfx_begin(void) {
	GX_SetViewport(0, 0, rmode->fbWidth, rmode->efbHeight, 0, 1);
	GX_InvVtxCache();
	GX_InvalidateTexAll();
	gfx_stat_polys = gfx_stat_ml = gfx_stat_v = gfx_stat_q = gfx_stat_skip = 0;
	mode_3d();
}

int gfx_end(void) {
	GX_DrawDone();
	GX_SetCopyClear(fog_col, 0x00ffffff);
	GX_SetZMode(GX_TRUE, GX_LEQUAL, GX_TRUE);
	GX_SetColorUpdate(GX_TRUE);
	GX_CopyDisp(xfb[cur], GX_TRUE);
	GX_Flush();
	VIDEO_SetNextFramebuffer(xfb[cur]);
	VIDEO_Flush();
	VIDEO_WaitVSync();
	cur ^= 1;
	int vb = retraces - last_retrace;
	last_retrace = retraces;
	return vb < 1 ? 1 : vb;
}

// ------------------------------------------------------------------ camera, fog

void gfx_camera(const V3 *e, const int16_t d[3], const int16_t u[3]) {
	guVector pos = { e->x / (float) FINE, e->y / (float) FINE, e->z / (float) FINE };
	guVector dir = { d[0] / 4096.0f, d[1] / 4096.0f, d[2] / 4096.0f };
	guVector upv = { u[0] / 4096.0f, u[1] / 4096.0f, u[2] / 4096.0f };
	guVecNormalize(&dir);
	guVector look = { pos.x + dir.x, pos.y + dir.y, pos.z + dir.z };
	guLookAt(view, &pos, &upv, &look);
	guVector r;
	guVecCross(&dir, &upv, &r);
	guVecNormalize(&r);
	guVector tu;
	guVecCross(&r, &dir, &tu);
	eye[0] = pos.x, eye[1] = pos.y, eye[2] = pos.z;
	fwd[0] = dir.x, fwd[1] = dir.y, fwd[2] = dir.z;
	right[0] = r.x, right[1] = r.y, right[2] = r.z;
	up[0] = tu.x, up[1] = tu.y, up[2] = tu.z;
	GX_LoadPosMtxImm(view, GX_PNMTX0);
}

const V3 *gfx_eye(void) {
	static V3 e;
	e.x = (int) (eye[0] * FINE), e.y = (int) (eye[1] * FINE), e.z = (int) (eye[2] * FINE);
	return &e;
}

void gfx_fog(const uint8_t rgb[3], int start, int end, int lamp_strength) {
	fog_col.r = rgb[0], fog_col.g = rgb[1], fog_col.b = rgb[2], fog_col.a = 255;
	fog_start = start / 256.0f;
	fog_end = end / 256.0f;
	if (fog_end <= fog_start)
		fog_end = fog_start + 1;
	lamp = lamp_strength / 256.0f;
	if (mode == 0)
		set_fog(1);
}

// Eye-space position of a world point (blocks): x right, y up, z forward.
static void to_eye(float x, float y, float z, float *ex, float *ey, float *ez) {
	float dx = x - eye[0], dy = y - eye[1], dz = z - eye[2];
	*ex = dx * right[0] + dy * right[1] + dz * right[2];
	*ey = dx * up[0] + dy * up[1] + dz * up[2];
	*ez = dx * fwd[0] + dy * fwd[1] + dz * fwd[2];
}

int gfx_project(const V3 *p, int *sx, int *sy) {
	float x, y, z;
	to_eye(p->x / (float) FINE, p->y / (float) FINE, p->z / (float) FINE, &x, &y, &z);
	if (z < 0.05f)
		return 0;
	*sx = (int) (SCREEN_W * 0.5f * (1.0f + x / (z * tan_h)));
	*sy = (int) (SCREEN_H * 0.5f * (1.0f - y / (z * tan_v)));
	int d = (int) (z * UNIT);
	return d < 1 ? 1 : d;
}

// Lantern light (0..1) at a distance in blocks.
static inline float lamp_at(float d) {
	float k = 1.0f - d / LAMP_RANGE;
	return k > 0 ? lamp * k * k : 0;
}

// ------------------------------------------------------------------ world

void gfx_world_palette(const uint8_t *p) {
	palette = p;
}

static inline void vertex(float x, float y, float z, uint32_t rgba, float u, float v) {
	GX_Position3f32(x, y, z);
	GX_Color1u32(rgba);
	GX_TexCoord2f32(u, v);
}

#define WHITE_U (8.0f / 256.0f)

void gfx_draw_meshlet(const MeshletHeader *m) {
	// Cull by bounding sphere against the view volume and the fog.
	float hx = (m->max[0] - m->min[0]) * (0.5f / UNIT), hy = (m->max[1] - m->min[1]) * (0.5f / UNIT),
		hz = (m->max[2] - m->min[2]) * (0.5f / UNIT);
	float cx = (m->ox + m->min[0]) / (float) UNIT + hx, cy = (m->oy + m->min[1]) / (float) UNIT + hy,
		cz = (m->oz + m->min[2]) / (float) UNIT + hz;
	float rad = sqrtf(hx * hx + hy * hy + hz * hz);
	float ex, ey, ez;
	to_eye(cx, cy, cz, &ex, &ey, &ez);
	if (ez + rad < NEAR || ez - rad > fog_end)
		return;
	if (fabsf(ex) - rad * 1.7f > ez * tan_h || fabsf(ey) - rad * 1.3f > ez * tan_v)
		return;
	gfx_stat_ml++;
	gfx_stat_v += m->nv;
	const MVert *v = (const MVert *) (m + 1);
	const MQuad *q = (const MQuad *) (v + m->nv);
	float ox = m->ox / (float) UNIT, oy = m->oy / (float) UNIT, oz = m->oz / (float) UNIT;
	// Vertex colours, with the lantern if it reaches this far.
	static uint32_t col[256];
	float dist = sqrtf(ex * ex + ey * ey + ez * ez);
	int lit = lamp > 0 && dist - rad < LAMP_RANGE;
	for (int i = 0; i < m->nv; i++) {
		const uint8_t *p = &palette[v[i].col * 4];
		int r = p[0], g = p[1], b = p[2];
		if (lit) {
			float dx = ox + v[i].x / (float) UNIT - eye[0], dy = oy + v[i].y / (float) UNIT - eye[1],
				dz = oz + v[i].z / (float) UNIT - eye[2];
			float k = lamp_at(sqrtf(dx * dx + dy * dy + dz * dz)) * p[3] * (128.0f / 255.0f);
			r += (int) k;
			g += (int) (k * 0.78f);
			b += (int) (k * 0.5f);
		}
		col[i] = (r > 255 ? 255 : r) << 24 | (g > 255 ? 255 : g) << 16 | (b > 255 ? 255 : b) << 8 | 255;
	}
	// Small details only nearby.
	int n = m->nq;
	if (dist - rad > DETAIL_FAR) {
		n -= m->group[7];
		gfx_stat_skip += m->group[7];
	}
	gfx_stat_q += n;
	gfx_stat_polys += n;
	GX_Begin(GX_QUADS, GX_VTXFMT0, n * 4);
	for (int k = 0; k < n; k++, q++) {
		float tu = (q->tile & 15) * (16.0f / 256.0f), tv = (q->tile >> 4) * (16.0f / 256.0f);
		for (int c = 0; c < 4; c++) {
			const MVert *p = &v[q->v[c]];
			float u = tu + ((q->uv >> (c * 2)) & 1) * (15.99f / 256.0f);
			float w = tv + ((q->uv >> (c * 2 + 1)) & 1) * (15.99f / 256.0f);
			vertex(ox + p->x / (float) UNIT, oy + p->y / (float) UNIT, oz + p->z / (float) UNIT, col[q->v[c]], u, w);
		}
	}
	GX_End();
}

void gfx_draw_sky(int with_moon) {
	// Far away and never fogged; drawn behind everything.
	set_fog(0);
	GX_SetZMode(GX_TRUE, GX_LEQUAL, GX_FALSE);
	int n = 0;
	for (int i = 0; i < sky_n; i++)
		if (with_moon || abs(sky[i].p[0].vx - sky[i].p[2].vx) <= 400)
			n++;
	GX_Begin(GX_QUADS, GX_VTXFMT0, n * 4);
	for (int i = 0; i < sky_n; i++) {
		const SkyQuad *q = &sky[i];
		if (!with_moon && abs(q->p[0].vx - q->p[2].vx) > 400)
			continue;
		uint32_t c = q->r << 24 | q->g << 16 | q->b << 8 | 255;
		float tu = (q->tile & 15) * (16.0f / 256.0f), tv = (q->tile >> 4) * (16.0f / 256.0f);
		for (int k = 0; k < 4; k++) {
			float u = tu + ((q->uv >> (k * 2)) & 1) * (15.99f / 256.0f);
			float w = tv + ((q->uv >> (k * 2 + 1)) & 1) * (15.99f / 256.0f);
			// (Pulled in toward the eye a little, to stay inside the far plane.)
			float x = q->p[k].vx / (float) UNIT, y = q->p[k].vy / (float) UNIT, z = q->p[k].vz / (float) UNIT;
			vertex(eye[0] + (x - eye[0]) * 0.9f, eye[1] + (y - eye[1]) * 0.9f, eye[2] + (z - eye[2]) * 0.9f, c, u, w);
		}
	}
	GX_End();
	GX_SetZMode(GX_TRUE, GX_LEQUAL, GX_TRUE);
	set_fog(1);
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

// Loads world -> eye times the model's own transform (4.12, scaled by `unit`).
static void load_model_matrix(const MATRIX *m, const V3 *pos, float unit) {
	Mtx mm, mv;
	for (int i = 0; i < 3; i++) {
		for (int j = 0; j < 3; j++)
			mm[i][j] = m->m[i][j] / 4096.0f * unit;
	}
	mm[0][3] = pos->x / (float) FINE, mm[1][3] = pos->y / (float) FINE, mm[2][3] = pos->z / (float) FINE;
	guMtxConcat(view, mm, mv);
	GX_LoadPosMtxImm(mv, GX_PNMTX0);
}

// Minecraft-style shade for a face normal (a column of `m`, negated for odd faces).
static float face_shade(const MATRIX *m, int face) {
	int axis = face >> 1;
	float nx = m->m[0][axis], ny = m->m[1][axis], nz = m->m[2][axis];
	if (face & 1)
		nx = -nx, ny = -ny, nz = -nz;
	float len = sqrtf(nx * nx + ny * ny + nz * nz);
	if (len <= 0)
		return 0.8f;
	float s = 0.8f + 0.2f * ny / len - 0.14f * fabsf(nx) / len;
	return s < 0.45f ? 0.45f : s > 1 ? 1 : s;
}

static inline uint32_t rgba_half(float r, float g, float b) {
	int ir = (int) (r * 128), ig = (int) (g * 128), ib = (int) (b * 128);
	return (ir > 255 ? 255 : ir < 0 ? 0 : ir) << 24 | (ig > 255 ? 255 : ig < 0 ? 0 : ig) << 16 |
		(ib > 255 ? 255 : ib < 0 ? 0 : ib) << 8 | 255;
}

void gfx_draw_model(int mi, const MATRIX *m, const V3 *pos, const int light_in[3], int flash_in, const int *tint) {
	const ModelHeader *h = models[mi];
	const SVECTOR *v = (const SVECTOR *) (h + 1);
	const MFace *f = (const MFace *) (v + h->nv);
	float ex, ey, ez;
	to_eye(pos->x / (float) FINE, pos->y / (float) FINE, pos->z / (float) FINE, &ex, &ey, &ez);
	if (ez < -8 || ez > fog_end + 8)
		return;
	float lk = lamp_at(sqrtf(ex * ex + ey * ey + ez * ez));
	float shade[6];
	for (int i = 0; i < 6; i++)
		shade[i] = face_shade(m, i);
	float light[3] = { light_in[0] / 4096.0f, light_in[1] / 4096.0f, light_in[2] / 4096.0f };
	float flash = flash_in / 4096.0f;
	load_model_matrix(m, pos, 1.0f / MODEL_UNIT);
	GX_Begin(GX_QUADS, GX_VTXFMT0, h->nq * 4);
	for (int k = 0; k < h->nq; k++, f++) {
		// face_color from the PC, with tint, flash and the lantern.
		float c[3] = { f->r / 255.0f, f->g / 255.0f, f->b / 255.0f };
		if (tint)
			for (int i = 0; i < 3; i++)
				c[i] *= tint[i] / 4096.0f;
		float glow = f->glow / 255.0f;
		if (flash > 0) {
			for (int i = 0; i < 3; i++)
				c[i] += (1 - c[i]) * flash;
			if (flash > glow)
				glow = flash;
		}
		float s = shade[f->face];
		float g = glow * (0.75f + 0.25f * s);
		float resp = (1 - glow) * s * lk;
		float out[3];
		static const float warm[3] = { 1.0f, 0.78f, 0.5f };
		for (int i = 0; i < 3; i++) {
			float l = light[i] * s;
			out[i] = c[i] * (l > g ? l : g) + warm[i] * resp * c[i];
		}
		uint32_t col = rgba_half(out[0], out[1], out[2]);
		for (int i = 0; i < 4; i++) {
			const SVECTOR *p = &v[f->v[i]];
			vertex(p->vx, p->vy, p->vz, col, WHITE_U, WHITE_U);
		}
	}
	GX_End();
	gfx_stat_polys += h->nq;
	GX_LoadPosMtxImm(view, GX_PNMTX0);
}

// Unit cube corners and faces in FACES order (see src/mesh.rs).
static const float cube_v[8][3] = {
	{ -0.5f, -0.5f, -0.5f }, { 0.5f, -0.5f, -0.5f }, { 0.5f, 0.5f, -0.5f }, { -0.5f, 0.5f, -0.5f },
	{ -0.5f, -0.5f, 0.5f }, { 0.5f, -0.5f, 0.5f }, { 0.5f, 0.5f, 0.5f }, { -0.5f, 0.5f, 0.5f },
};
static const uint8_t cube_f[6][4] = {
	{ 5, 1, 2, 6 }, { 0, 4, 7, 3 }, { 7, 6, 2, 3 }, { 0, 1, 5, 4 }, { 4, 5, 6, 7 }, { 1, 0, 3, 2 },
};

void gfx_draw_box(const MATRIX *m, const V3 *pos, int tile, const uint8_t rgb[3]) {
	load_model_matrix(m, pos, 1.0f);
	float tu = tile < 0 ? WHITE_U : (tile & 15) * (16.0f / 256.0f), tv = tile < 0 ? WHITE_U : (tile >> 4) * (16.0f / 256.0f);
	float du = tile < 0 ? 0 : 15.99f / 256.0f;
	GX_Begin(GX_QUADS, GX_VTXFMT0, 24);
	for (int fi = 0; fi < 6; fi++) {
		float s = face_shade(m, fi);
		float k = s / 255.0f;
		uint32_t col = rgba_half(rgb[0] * k, rgb[1] * k, rgb[2] * k);
		// Corner order as push_quad: bottom left of the tile first.
		static const uint8_t cu[4] = { 0, 1, 1, 0 }, cv[4] = { 1, 1, 0, 0 };
		for (int i = 0; i < 4; i++) {
			const float *c = cube_v[cube_f[fi][i]];
			vertex(c[0], c[1], c[2], col, tu + cu[i] * du, tv + cv[i] * du);
		}
	}
	GX_End();
	gfx_stat_polys += 6;
	GX_LoadPosMtxImm(view, GX_PNMTX0);
}

void gfx_draw_spark(const V3 *pos, int size, const uint8_t rgb[3]) {
	float c[3] = { pos->x / (float) FINE, pos->y / (float) FINE, pos->z / (float) FINE };
	float h = size / (float) FINE * 0.5f;
	uint32_t col = rgba_half(rgb[0] / 255.0f, rgb[1] / 255.0f, rgb[2] / 255.0f);
	GX_Begin(GX_QUADS, GX_VTXFMT0, 4);
	static const float sx[4] = { -1, 1, 1, -1 }, sy[4] = { -1, -1, 1, 1 };
	for (int i = 0; i < 4; i++)
		vertex(c[0] + (right[0] * sx[i] + up[0] * sy[i]) * h, c[1] + (right[1] * sx[i] + up[1] * sy[i]) * h,
			c[2] + (right[2] * sx[i] + up[2] * sy[i]) * h, col, WHITE_U, WHITE_U);
	GX_End();
}

void gfx_puffs_begin(void) {
}

void gfx_puff(const SVECTOR *pos, int hw, int hh, const uint8_t rgb[3]) {
	float c[3] = { pos->vx / (float) UNIT, pos->vy / (float) UNIT, pos->vz / (float) UNIT };
	float ex, ey, ez;
	to_eye(c[0], c[1], c[2], &ex, &ey, &ez);
	if (ez < 0.5f || ez > fog_end)
		return;
	// Upright, turned to face the camera; thinner right in front of it.
	float tx = eye[0] - c[0], tz = eye[2] - c[2];
	float l = sqrtf(tx * tx + tz * tz);
	if (l < 0.01f)
		return;
	float rx = tz / l * hw / UNIT, rz = -tx / l * hw / UNIT, uy = hh / (float) UNIT;
	float near = ez < 1 ? 0 : ez > 6 ? 1 : (ez - 1) / 5;
	GX_LoadTexObj(&puff_tex, GX_TEXMAP0);
	set_blend(1);
	GX_SetZMode(GX_TRUE, GX_LEQUAL, GX_FALSE);
	uint32_t col = (rgb[0] >> 1) << 24 | (rgb[1] >> 1) << 16 | (rgb[2] >> 1) << 8 | (int) (255 * near);
	GX_Begin(GX_QUADS, GX_VTXFMT0, 4);
	vertex(c[0] - rx, c[1] - uy, c[2] - rz, col, 0, 1);
	vertex(c[0] + rx, c[1] - uy, c[2] + rz, col, 1, 1);
	vertex(c[0] + rx, c[1] + uy, c[2] + rz, col, 1, 0);
	vertex(c[0] - rx, c[1] + uy, c[2] - rz, col, 0, 0);
	GX_End();
	GX_SetZMode(GX_TRUE, GX_LEQUAL, GX_TRUE);
	set_blend(0);
	GX_LoadTexObj(&atlas_tex, GX_TEXMAP0);
}

// ------------------------------------------------------------------ HUD

static int glyph_scale(int scale) {
	return scale < 2048 ? 2048 : scale;
}

int hud_text_width(const char *s, int scale) {
	return (strlen(s) * 8 * glyph_scale(scale)) >> 12;
}

static void hud_quad(float x, float y, float w, float h, float u0, float v0, float u1, float v1, uint32_t col) {
	GX_Begin(GX_QUADS, GX_VTXFMT0, 4);
	vertex(x, y, 0, col, u0, v0);
	vertex(x + w, y, 0, col, u1, v0);
	vertex(x + w, y + h, 0, col, u1, v1);
	vertex(x, y + h, 0, col, u0, v1);
	GX_End();
}

static inline uint32_t hud_col(const uint8_t rgb[3], int alpha) {
	return (rgb[0] >> 1) << 24 | (rgb[1] >> 1) << 16 | (rgb[2] >> 1) << 8 | alpha;
}

void hud_text(const char *s, int x, int y, int scale, const uint8_t rgb[3], int align) {
	mode_hud();
	scale = glyph_scale(scale);
	int w = hud_text_width(s, scale);
	if (align == ALIGN_CENTER)
		x -= w / 2;
	else if (align == ALIGN_RIGHT)
		x -= w;
	float size = 8 * scale / 4096.0f, gx = x;
	uint32_t col = hud_col(rgb, 255);
	for (; *s; s++, gx += size) {
		int ch = (uint8_t) *s;
		if (ch == ' ' || ch >= 128)
			continue;
		float u = (ch & 31) * 8 / 256.0f, v = (128 + (ch >> 5) * 8) / 256.0f;
		hud_quad(gx, y, size, size, u, v, u + 8 / 256.0f, v + 8 / 256.0f, col);
	}
}

void hud_text_fit(const char *s, int x, int y, int scale, int max_w, const uint8_t rgb[3], int align) {
	int w = hud_text_width(s, 4096);
	if (w > 0 && ((w * scale) >> 12) > max_w)
		scale = (max_w << 12) / w;
	hud_text(s, x, y, scale, rgb, align);
}

void hud_rect(int x, int y, int w, int h, const uint8_t rgb[3], int alpha) {
	if (w <= 0 || h <= 0 || alpha <= 0)
		return;
	mode_hud();
	hud_quad(x, y, w, h, WHITE_U, WHITE_U, WHITE_U, WHITE_U, hud_col(rgb, alpha > 255 ? 255 : alpha));
}

void hud_panel(int x, int y, int w, int h, const uint8_t fill[3], int alpha, const uint8_t border[3]) {
	hud_rect(x, y, w, h, fill, alpha);
	hud_rect(x, y, w, 1, border, 255);
	hud_rect(x, y + h - 1, w, 1, border, 255);
	hud_rect(x, y, 1, h, border, 255);
	hud_rect(x + w - 1, y, 1, h, border, 255);
}

void hud_sprite(int tile, int cx, int cy, int size, const uint8_t rgb[3]) {
	mode_hud();
	float u = (tile & 15) * 16 / 256.0f, v = (tile >> 4) * 16 / 256.0f;
	hud_quad(cx - size * 0.5f, cy - size * 0.5f, size, size, u, v, u + 16 / 256.0f, v + 16 / 256.0f, hud_col(rgb, 255));
}

void hud_darken(int amount) {
	if (amount >= 200) {
		static const uint8_t dark[3] = { 10, 5, 18 };
		hud_rect(0, 0, SCREEN_W, SCREEN_H, dark, 240);
	} else if (amount > 0) {
		static const uint8_t black[3] = { 0, 0, 0 };
		hud_rect(0, 0, SCREEN_W, SCREEN_H, black, amount);
	}
}

void hud_flash(void) {
	static const uint8_t white[3] = { 255, 255, 255 };
	hud_rect(0, 0, SCREEN_W, SCREEN_H, white, 255);
}
