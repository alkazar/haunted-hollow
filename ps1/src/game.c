// Gameplay, ported from src/game.rs. The ride's targets are the very ones the PC
// game spawns (recorded by the exporter, which also checked every spot can be
// seen from the cart), replayed here at the same moments.
//
// Up to two riders, one per controller port: P1 red, P2 blue.

#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <psxgte.h>
#include "game.h"
#include "gfx.h"
#include "cam.h"
#include "stream.h"
#include "audio.h"
#include "input.h"
#include "util.h"
#include "data.h"
#include <hwregs_c.h>

#define NP PORTS

enum { K_PUMPKIN, K_GHOST, K_BAT, K_BOOK, K_SKELETON, K_SPIDER, K_CANDY, K_PORTRAIT };
enum { S_TITLE, S_TUTORIAL, S_RIDE, S_BOSS, S_VICTORY, S_OUTRO, S_RESULTS, S_INITIALS, S_HIGHSCORES };
enum { Z_GRAVEYARD, Z_FOYER, Z_LIBRARY, Z_DINING, Z_DROP, Z_CRYPT, Z_LAIR };

#define RIDE_SECONDS   120.0f
#define BOSS_COUNT     8
#define PORTRAIT_PEEL  0.8f
#define MAX_AIM_BONUS  5000
#define GUARD_END      2.15f
#define DOOR_START     1.8f
#define DOOR_TIME      2.4f
#define OUTRO_TIME     4.8f
#define DOOR_HALF      5.0f
#define DOOR_FLOOR     -17.0f
#define DOOR_Z         214.5f
#define INITIALS_SECONDS 45.0f
#define TABLE_SIZE     10
#define MAX_INITIALS   3

#define MAX_TARGETS    24
#define MAX_PARTICLES  160
#define MAX_POPUPS     12

static const uint8_t PLAYER_COLORS[4][3] = { { 255, 71, 71 }, { 77, 153, 255 }, { 89, 242, 102 }, { 255, 219, 51 } };
static const char *const PLAYER_NAMES[4] = { "RED", "BLUE", "GREEN", "YELLOW" };
static const int POINTS[8] = { 100, 100, 150, 150, 200, 150, 500, 200 };
static const float RADIUS[8] = { 0.7f, 0.85f, 0.9f, 0.9f, 0.8f, 1.05f, 0.6f, 0.75f };
static const float KIND_SIZE[8] = { 1, 1, 1.5f, 1.5f, 1, 1.5f, 1, 1 };
static const char *const AWARDS[10] = {
	"PUMPKIN POPPER", "GHOST CATCHER", "BAT BOPPER", "BOOKWORM", "BONE BONKER",
	"SPIDER SPOTTER", "CANDY CHAMP", "ART CRITIC", "BOSS BUSTER", "SPOOKY STAR",
};
static const char *const KEYS[28] = {
	"A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O", "P", "Q", "R", "S", "T", "U", "V",
	"W", "X", "Y", "Z", "DEL", "END",
};
#define KEY_DEL  26
#define KEY_END  27
#define KEY_COLS 7

static const uint8_t WHITE[3] = { 255, 255, 255 };
static const uint8_t ORANGE[3] = { 255, 153, 26 };
static const uint8_t BLACK[3] = { 0, 0, 0 };

typedef struct {
	int kind, variant, dead;
	F3 home, a, b, pos;
	float age, life, leave, phase, scale;
} Target;

typedef struct {
	V3 pos, vel;             // fine units, fine units per second
	uint8_t color[3];
	int16_t size;            // fine units
	int16_t gravity;         // fine units per second^2
	int age, life;           // ms
} Particle;

typedef struct {
	F3 pos;
	char text[20];
	const uint8_t *color;
	float age;
	int big;
} Popup;

typedef struct {
	int joined, score, hits, shots, bonus;
	float shown;
	float aim_x, aim_y, aim_tx, aim_ty;
	int aim_visible;
	float cooldown, held, kick;
	int counts[9];
} Player;

typedef struct {
	int active;
	float hp, max_hp;
	F3 pos;
	float t, sway, guard_t, flash, wobble, taunt;
	int summons_done, defeated;
	const char *taunt_text;
} Boss;

typedef struct {
	char letters[4];
	int active;
	float done_at;           // < 0 while still entering
} Initials;

typedef struct {
	int score;
	char initials[4];
} Entry;

static struct {
	int state;
	float state_t, ride_t, total_t;
	int paused;
	Player players[NP];
	Target targets[MAX_TARGETS];
	int n_targets;
	Particle particles[MAX_PARTICLES];
	int n_particles;
	Popup popups[MAX_POPUPS];
	int n_popups;
	Boss boss;
	int next_spawn;
	const SpawnRec *spawns;
	int n_spawns;
	float last_spawn_sound[8];
	int zone;
	char banner[32];
	float banner_t;
	float fog[3], fog_start, fog_end, lamp;
	float shake;
	float door_open;
	float title_join_t;      // < 0 until someone joins
	float practice[NP];      // when each player popped their practice target, < 0 if not yet
	Initials initials[NP];
	float key_flash_t[28];
	int key_flash_p[28];
	Entry high[TABLE_SIZE];
	int n_high;
	int high_new[NP][2];     // (rank, player)
	int n_high_new;
	// Camera this frame.
	Cam cam;
	F3 eye, dir, up, right;
	int speed16;
	Rng rng;
} G;

static const SpawnRec *spawn_one, *spawn_two, *spawn_summon;
static int n_one, n_two, n_summon;
static const SVECTOR *fires, *cauldrons;
static const Cloud *clouds;
static const Mist *mists;
static int n_fires, n_cauldrons, n_clouds, n_mist;

// ------------------------------------------------------------------ helpers

static inline V3 fine(F3 p) {
	V3 v = { (int) (p.x * FINE), (int) (p.y * FINE), (int) (p.z * FINE) };
	return v;
}

static F3 rec_vec(const int16_t *v) {
	return f3(v[0] * (1.0f / UNIT), v[1] * (1.0f / UNIT), v[2] * (1.0f / UNIT));
}

static void sfx(int which, float vol, float pan, float pitch) {
	audio_sfx(which, vol, pan, pitch);
}

static int joined_count(void) {
	int n = 0;
	for (int i = 0; i < NP; i++)
		n += G.players[i].joined;
	return n;
}

static float pan_of(F3 p) {
	int sx, sy;
	V3 f = fine(p);
	if (!gfx_project(&f, &sx, &sy))
		return 0;
	return clampf((sx - SCREEN_W / 2) / (float) (SCREEN_W / 2), -1, 1) * 0.8f;
}

static float pan_x(float x) {
	return (x / SCREEN_W * 2.0f - 1.0f) * 0.8f;
}

// World point -> screen position and depth in blocks, if it's in front and near the screen.
static int project(F3 p, float *sx, float *sy, float *w) {
	int x, y;
	V3 f = fine(p);
	int z = gfx_project(&f, &x, &y);
	if (!z || x < -SCREEN_W / 5 || x > SCREEN_W * 6 / 5 || y < -SCREEN_H / 5 || y > SCREEN_H * 6 / 5)
		return 0;
	*sx = x, *sy = y, *w = z * (1.0f / UNIT);
	return 1;
}

static float pixel_radius(float r, float w) {
	float px = r * 1.45f * PROJ_H / w;
	return px < 10 ? 10 : px;
}

// Direction of the ray through a screen point.
static F3 ray_dir(float sx, float sy) {
	float kx = (sx - SCREEN_W / 2) / (float) PROJ_H, ky = (sy - SCREEN_H / 2) / (float) PROJ_H;
	return f3_norm(f3_add(G.dir, f3_sub(f3_mul(G.right, kx), f3_mul(G.up, ky))));
}

static void light_at(F3 p, int out[3]) {
	V3 f = fine(p);
	gfx_light_at(&f, out);
}

// ------------------------------------------------------------------ transforms

// A transform: rotation/scale (4.12) and translation (fine units).
typedef struct {
	int m[3][3];
	V3 t;
} Xf;

static void xf_identity(Xf *x) {
	memset(x, 0, sizeof(*x));
	x->m[0][0] = x->m[1][1] = x->m[2][2] = 4096;
}

static void xf_at(Xf *x, F3 p) {
	xf_identity(x);
	x->t = fine(p);
}

// a = a * b (b is applied first).
static void xf_mul(Xf *a, const Xf *b) {
	Xf r;
	for (int i = 0; i < 3; i++) {
		for (int j = 0; j < 3; j++)
			r.m[i][j] = (a->m[i][0] * b->m[0][j] + a->m[i][1] * b->m[1][j] + a->m[i][2] * b->m[2][j]) >> 12;
	}
	r.t.x = a->t.x + ((a->m[0][0] * b->t.x + a->m[0][1] * b->t.y + a->m[0][2] * b->t.z) >> 12);
	r.t.y = a->t.y + ((a->m[1][0] * b->t.x + a->m[1][1] * b->t.y + a->m[1][2] * b->t.z) >> 12);
	r.t.z = a->t.z + ((a->m[2][0] * b->t.x + a->m[2][1] * b->t.y + a->m[2][2] * b->t.z) >> 12);
	*a = r;
}

static void xf_move(Xf *a, float x, float y, float z) {
	Xf b;
	xf_identity(&b);
	b.t.x = (int) (x * FINE), b.t.y = (int) (y * FINE), b.t.z = (int) (z * FINE);
	xf_mul(a, &b);
}

static void xf_rot(Xf *a, int axis, float ang) {
	Xf b;
	xf_identity(&b);
	int c = fix(fcos(ang)), s = fix(fsin(ang));
	if (axis == 0) {
		b.m[1][1] = c, b.m[1][2] = -s, b.m[2][1] = s, b.m[2][2] = c;
	} else if (axis == 1) {
		b.m[0][0] = c, b.m[0][2] = s, b.m[2][0] = -s, b.m[2][2] = c;
	} else {
		b.m[0][0] = c, b.m[0][1] = -s, b.m[1][0] = s, b.m[1][1] = c;
	}
	xf_mul(a, &b);
}

static void xf_scale(Xf *a, float x, float y, float z) {
	int sx = fix(x), sy = fix(y), sz = fix(z);
	for (int i = 0; i < 3; i++) {
		a->m[i][0] = (a->m[i][0] * sx) >> 12;
		a->m[i][1] = (a->m[i][1] * sy) >> 12;
		a->m[i][2] = (a->m[i][2] * sz) >> 12;
	}
}

static void to_matrix(const Xf *x, MATRIX *m) {
	for (int i = 0; i < 3; i++)
		for (int j = 0; j < 3; j++) {
			int v = x->m[i][j];
			m->m[i][j] = v > 32767 ? 32767 : v < -32768 ? -32768 : v;
		}
}

static void draw_model(int model, const Xf *x, const int light[3], float flash, const int *tint) {
	MATRIX m;
	to_matrix(x, &m);
	gfx_draw_model(model, &m, &x->t, light, fix(flash), tint);
}

// push_box from the PC: a box lit by `light` with a minimum glow.
static void draw_box(const Xf *x, int tile, const float tint[3], const int light[3], float glow) {
	MATRIX m;
	to_matrix(x, &m);
	int g = fix(glow * 0.875f);
	uint8_t rgb[3];
	for (int i = 0; i < 3; i++) {
		int l = light[i] > g ? light[i] : g;
		int v = (int) (tint[i] * l) >> 4;
		rgb[i] = v > 255 ? 255 : v < 0 ? 0 : v;
	}
	gfx_draw_box(&m, &x->t, tile, rgb);
}

static void box_at(F3 c, float sx, float sy, float sz, int tile, const float tint[3], const int light[3], float glow) {
	Xf x;
	xf_at(&x, c);
	xf_scale(&x, sx, sy, sz);
	draw_box(&x, tile, tint, light, glow);
}

// ------------------------------------------------------------------ zones

static int zone_at(F3 p) {
	if (p.z < 39.5f) return Z_GRAVEYARD;
	if (p.z < 70.5f) return Z_FOYER;
	if (p.z < 100.5f) return Z_LIBRARY;
	if (p.z < 127.5f && p.y > -0.5f) return Z_DINING;
	if (p.z < 140.0f) return Z_DROP;
	if (p.z < 176.5f) return Z_CRYPT;
	return Z_LAIR;
}

static const char *zone_name(int z) {
	static const char *const names[7] = {
		"THE GRAVEYARD", "THE GRAND FOYER", "THE HAUNTED LIBRARY", "THE DINING HALL",
		"HOLD ON TIGHT!", "THE CREEPY CRYPT", "THE PUMPKIN KING'S LAIR",
	};
	return names[z];
}

// Fog colour, start and end (blocks) and lantern strength per zone. The PS1 fog
// closes in sooner than the PC's (it can't draw as far).
static void zone_fog(int z, float rgb[3], float *start, float *end, float *lamp) {
	static const float col[7][3] = {
		{ 0.07f, 0.06f, 0.15f }, { 0.04f, 0.03f, 0.06f }, { 0.04f, 0.03f, 0.06f }, { 0.04f, 0.03f, 0.06f },
		{ 0.02f, 0.04f, 0.03f }, { 0.03f, 0.06f, 0.05f }, { 0.015f, 0.005f, 0.025f },
	};
	static const float range[7][3] = {
		{ 9, 20, 0.5f }, { 7, 17, 0.95f }, { 7, 17, 0.95f }, { 7, 17, 0.95f },
		{ 6, 15, 0.9f }, { 7, 17, 0.9f }, { 12, 26, 0.45f },
	};
	for (int i = 0; i < 3; i++)
		rgb[i] = col[z][i];
	*start = range[z][0], *end = range[z][1], *lamp = range[z][2];
}

// ------------------------------------------------------------------ states

static void set_banner(const char *s, float t) {
	strncpy(G.banner, s, sizeof(G.banner) - 1);
	G.banner[sizeof(G.banner) - 1] = 0;
	G.banner_t = t;
}

static void reset_score(Player *p) {
	p->score = p->hits = p->shots = p->bonus = 0;
	p->shown = 0;
	memset(p->counts, 0, sizeof(p->counts));
}

static void start_tutorial(void) {
	G.state = S_TUTORIAL;
	G.state_t = 0;
	G.ride_t = 0;
	for (int i = 0; i < NP; i++)
		G.practice[i] = -1;
	G.n_targets = 0;
	G.n_particles = 0;
}

static void start_ride(void) {
	G.state = S_RIDE;
	G.state_t = 0;
	G.ride_t = 0;
	G.n_targets = G.n_particles = G.n_popups = 0;
	G.door_open = 0;
	G.boss.active = 0;
	G.zone = Z_GRAVEYARD;
	set_banner(zone_name(Z_GRAVEYARD), -2.5f);
	for (int i = 0; i < NP; i++)
		reset_score(&G.players[i]);
	// More riders get more targets (the PC game schedules them denser).
	if (joined_count() >= 2) {
		G.spawns = spawn_two, G.n_spawns = n_two;
	} else {
		G.spawns = spawn_one, G.n_spawns = n_one;
	}
	G.next_spawn = 0;
	for (int k = 0; k < 8; k++)
		G.last_spawn_sound[k] = -9;
}

static void go_to_title(void) {
	G.state = S_TITLE;
	G.state_t = 0;
	G.title_join_t = -1;
	G.paused = 0;
	G.n_targets = G.n_particles = G.n_popups = 0;
	G.boss.active = 0;
	G.door_open = 0;
	for (int i = 0; i < NP; i++) {
		G.players[i].joined = 0;
		G.players[i].aim_visible = 0;
	}
	// Back to the station: read the start of the ride again.
	stream_reset();
}

static int qualifies(int score, int place_offset) {
	if (score <= 0)
		return 0;
	int better = 0;
	for (int i = 0; i < G.n_high; i++)
		if (G.high[i].score >= score)
			better++;
	return better + place_offset < TABLE_SIZE;
}

// Riders whose score makes the table (each taking a place from the next).
static int high_score_riders(int out[NP]) {
	int order[NP], n = 0;
	for (int i = 0; i < NP; i++)
		if (G.players[i].joined)
			order[n++] = i;
	for (int a = 0; a < n; a++)
		for (int b = a + 1; b < n; b++)
			if (G.players[order[b]].score > G.players[order[a]].score) {
				int t = order[a];
				order[a] = order[b], order[b] = t;
			}
	int k = 0;
	for (int i = 0; i < n; i++)
		if (qualifies(G.players[order[i]].score, k))
			out[k++] = order[i];
	return k;
}

static void leave_results(void) {
	int made[NP];
	int n = high_score_riders(made);
	G.n_high_new = 0;
	G.state_t = 0;
	if (n) {
		memset(G.initials, 0, sizeof(G.initials));
		for (int i = 0; i < n; i++) {
			G.initials[made[i]].active = 1;
			G.initials[made[i]].done_at = -1;
		}
		for (int k = 0; k < 28; k++)
			G.key_flash_t[k] = -9;
		G.state = S_INITIALS;
	} else if (G.n_high) {
		G.state = S_HIGHSCORES;
	} else {
		go_to_title();
	}
}

static int insert_high(int score, const char *name) {
	int pos = 0;
	while (pos < G.n_high && G.high[pos].score >= score)
		pos++;
	if (pos >= TABLE_SIZE)
		return -1;
	int n = G.n_high < TABLE_SIZE ? G.n_high + 1 : TABLE_SIZE;
	for (int i = n - 1; i > pos; i--)
		G.high[i] = G.high[i - 1];
	G.high[pos].score = score;
	strncpy(G.high[pos].initials, name, 3);
	G.high[pos].initials[3] = 0;
	G.n_high = n;
	return pos;
}

static void finish_initials(void) {
	int order[NP], n = 0;
	for (int i = 0; i < NP; i++)
		if (G.initials[i].active)
			order[n++] = i;
	for (int a = 0; a < n; a++)
		for (int b = a + 1; b < n; b++)
			if (G.players[order[b]].score > G.players[order[a]].score) {
				int t = order[a];
				order[a] = order[b], order[b] = t;
			}
	for (int k = 0; k < n; k++) {
		int i = order[k];
		char name[4];
		if (G.initials[i].letters[0]) {
			strcpy(name, G.initials[i].letters);
		} else {
			name[0] = 'P', name[1] = '1' + i, name[2] = 0;
		}
		G.initials[i].active = 0;
		int rank = insert_high(G.players[i].score, name);
		if (rank >= 0 && G.n_high_new < NP) {
			// Earlier entries this round move down past the new one.
			for (int j = 0; j < G.n_high_new; j++)
				if (G.high_new[j][0] >= rank)
					G.high_new[j][0]++;
			G.high_new[G.n_high_new][0] = rank;
			G.high_new[G.n_high_new][1] = i;
			G.n_high_new++;
		}
	}
	G.state = S_HIGHSCORES;
	G.state_t = 0;
}

static void practice_pos(int i, float *x, float *y) {
	*x = (i + 0.5f) * SCREEN_W / NP;
	*y = SCREEN_H * 0.6f;
}

#define PRACTICE_R 28

// ------------------------------------------------------------------ targets

static F3 motion(const Target *t, float a) {
	float ph = t->phase;
	switch (t->kind) {
	case K_GHOST:
		return f3_add(t->home, f3(fsin(a * 1.3f + ph) * 0.6f, fsin(a * 2.1f + ph) * 0.35f, 0));
	case K_PUMPKIN:
		return f3_add(t->home, f3(0, fsin(a * 3.0f + ph) * 0.06f, 0));
	case K_SKELETON:
		return t->home;
	case K_SPIDER:
		return f3_add(t->home, f3(0, fsin(a * 2.0f) * 0.12f, 0));
	case K_BOOK:
		return f3_add(t->home, f3(fcos(a * 1.6f + ph) * 0.9f, fsin(a * 3.0f) * 0.3f, fsin(a * 1.6f + ph) * 0.9f));
	case K_BAT: {
		float u = minf(a / t->life, 1.0f);
		u = u + fsin(u * TAU * 3.0f + ph) * 0.04f;
		F3 jink = f3(fsin(a * 4.3f + ph) * 0.35f + fsin(a * 11.0f + ph * 2.0f) * 0.15f,
			fsin(a * 5.0f + ph) * 0.4f + fsin(a * 13.7f + ph) * 0.2f,
			fcos(a * 3.7f + ph * 1.5f) * 0.3f);
		return f3_add(f3_lerp(t->a, t->b, u), jink);
	}
	case K_CANDY:
		return f3_add(f3_lerp(t->a, t->b, minf(a / t->life, 1.0f)), f3(0, fsin(a * 6.0f) * 0.3f, 0));
	case K_PORTRAIT:
		if (a < PORTRAIT_PEEL)
			return t->a;
		{
			float u = clampf((a - PORTRAIT_PEEL) / (t->life - PORTRAIT_PEEL), 0, 1);
			return f3_add(f3_lerp(t->a, t->b, u * u * (3.0f - 2.0f * u)),
				f3(0, fsin(a * 2.5f + ph) * 0.12f * minf(u * 3.0f, 1.0f), 0));
		}
	}
	return t->home;
}

static void spawn_sound(int kind, F3 at) {
	static const int8_t snd[8] = { SFX_BOING, SFX_GHOST, SFX_SQUEAK, SFX_FLUTTER, SFX_RATTLE, SFX_ZIP, SFX_BONUS, SFX_RATTLE };
	static const float vol[8] = { 0.35f, 0.35f, 0.3f, 0.4f, 0.45f, 0.3f, 0.25f, 0.3f };
	if (G.total_t - G.last_spawn_sound[kind] < 1.2f)
		return;
	G.last_spawn_sound[kind] = G.total_t;
	sfx(snd[kind], vol[kind], pan_of(at), 0.9f + fractf(G.total_t * 7.3f) * 0.2f);
}

static void spawn_rec(const SpawnRec *r, F3 rel) {
	if (G.n_targets >= MAX_TARGETS)
		return;
	Target *t = &G.targets[G.n_targets++];
	memset(t, 0, sizeof(*t));
	t->kind = r->kind;
	t->variant = r->variant;
	t->home = f3_add(rec_vec(r->home), rel);
	t->a = f3_add(rec_vec(r->a), rel);
	t->b = f3_add(rec_vec(r->b), rel);
	t->life = r->life_ms * 0.001f;
	t->phase = r->phase * (1.0f / 4096.0f);
	t->pos = motion(t, 0);
	spawn_sound(t->kind, t->home);
}

static void burst(F3 at, int n, const uint8_t *player_color, int puff) {
	static const uint8_t candy[6][3] = {
		{ 255, 140, 26 }, { 179, 77, 255 }, { 102, 255, 77 }, { 255, 230, 51 }, { 255, 115, 191 }, { 255, 255, 255 },
	};
	V3 p = fine(at);
	for (int k = 0; k < n + (puff ? 10 : 0); k++) {
		if (G.n_particles >= MAX_PARTICLES)
			return;
		Particle *q = &G.particles[G.n_particles++];
		q->pos = p;
		q->age = 0;
		if (k < n) {
			F3 v = f3_mul(f3_norm(f3(rng_range(&G.rng, -1, 1), rng_range(&G.rng, -0.2f, 1.4f), rng_range(&G.rng, -1, 1))),
				rng_range(&G.rng, 2.0f, 5.5f));
			q->vel = fine(v);
			const uint8_t *c = (k % 4 == 0) ? player_color : candy[rng_int(&G.rng, 0, 5)];
			memcpy(q->color, c, 3);
			q->size = (int16_t) (rng_range(&G.rng, 0.09f, 0.17f) * FINE);
			q->life = (int) (rng_range(&G.rng, 0.7f, 1.3f) * 1000);
			q->gravity = 7 * FINE;
		} else {
			F3 v = f3_mul(f3(rng_range(&G.rng, -1, 1), rng_range(&G.rng, -0.5f, 1), rng_range(&G.rng, -1, 1)), 1.2f);
			q->vel = fine(v);
			q->color[0] = 230, q->color[1] = 230, q->color[2] = 255;
			q->size = (int16_t) (rng_range(&G.rng, 0.25f, 0.4f) * FINE);
			q->life = 500;
			q->gravity = -FINE / 2;
		}
	}
}

static void sparkle(F3 at, int n, const uint8_t *color, float speed, float life, int gravity, float size) {
	V3 p = fine(at);
	for (int k = 0; k < n && G.n_particles < MAX_PARTICLES; k++) {
		Particle *q = &G.particles[G.n_particles++];
		q->pos = p;
		F3 v = f3_mul(f3(rng_range(&G.rng, -1, 1), rng_range(&G.rng, -1, 1), rng_range(&G.rng, -1, 1)), speed);
		q->vel = fine(v);
		memcpy(q->color, color, 3);
		q->size = (int16_t) (size * FINE);
		q->age = 0;
		q->life = (int) (life * 1000);
		q->gravity = gravity;
	}
}

static void popup(F3 pos, const char *text, const uint8_t *color, int big) {
	if (G.n_popups >= MAX_POPUPS) {
		memmove(&G.popups[0], &G.popups[1], sizeof(Popup) * (MAX_POPUPS - 1));
		G.n_popups--;
	}
	Popup *p = &G.popups[G.n_popups++];
	p->pos = pos;
	strncpy(p->text, text, sizeof(p->text) - 1);
	p->text[sizeof(p->text) - 1] = 0;
	p->color = color;
	p->age = 0;
	p->big = big;
}

// ------------------------------------------------------------------ boss

static float boss_guard(const Boss *b) {
	if (b->defeated)
		return 0;
	float u = b->guard_t;
	return smooth(u / 0.35f) * (1.0f - smooth((u - (GUARD_END - 0.35f)) / 0.35f));
}

static void boss_parts(const Boss *b, F3 *head, F3 hands[2]) {
	float t = b->t;
	*head = b->pos;
	F3 l = f3_add(b->pos, f3(-6.2f + fsin(t * 2.0f) * 0.6f, -2.0f + fsin(t * 3.1f) * 1.2f, -1.5f));
	F3 r = f3_add(b->pos, f3(6.2f + fcos(t * 2.3f) * 0.6f, -2.0f + fcos(t * 2.7f) * 1.2f, -1.5f));
	float g = boss_guard(b);
	F3 shake = f3(0, fsin(t * 9.0f) * 0.1f, 0);
	F3 face = f3_add(b->pos, f3(0, 0.4f, 0));
	F3 to_cam = f3_norm(f3_sub(G.eye, face));
	F3 side = f3_norm(f3_cross(to_cam, f3(0, 1, 0)));
	F3 gl = f3_add(f3_sub(f3_add(face, f3_mul(to_cam, 5.5f)), f3_mul(side, 1.6f)), shake);
	F3 gr = f3_sub(f3_add(f3_add(face, f3_mul(to_cam, 5.5f)), f3_mul(side, 1.6f)), shake);
	hands[0] = f3_lerp(l, gl, g);
	hands[1] = f3_lerp(r, gr, g);
}

static void start_boss(void) {
	G.state = S_BOSS;
	G.state_t = 0;
	float n = (float) (joined_count() > 0 ? joined_count() : 1);
	Boss *b = &G.boss;
	memset(b, 0, sizeof(*b));
	b->active = 1;
	b->hp = b->max_hp = 40 + 22 * n;
#ifdef HH_TEST_BOSS
	b->hp = b->max_hp = 3;   // (test builds: a quick boss)
#endif
	b->pos = f3(0, -26, 200);
	b->guard_t = -5;
	b->taunt = 2.5f;
	b->taunt_text = "MWAHAHA!";
	set_banner("THE PUMPKIN KING!", 0);
	G.shake = 0.6f;
	sfx(SFX_RUMBLE, 0.8f, 0, 1);
	sfx(SFX_LAUGH, 0.9f, 0, 1);
}

static void update_boss(float dt) {
	Boss *b = &G.boss;
	b->t += dt;
	b->flash = maxf(b->flash - dt * 5.0f, 0);
	b->wobble = maxf(b->wobble - dt * 2.0f, 0);
	float rage = 1.0f - b->hp / b->max_hp;
	float rise = smooth(b->t / 2.5f);
	b->sway += dt * (0.45f + rage * 0.5f);
	b->guard_t += dt;
	float k = 1.0f - rage;
	float rest = maxf(4.5f * k * fsqrt(k), 0.4f);
	if (b->guard_t > GUARD_END + rest)
		b->guard_t = 0;
	float x = fsin(b->sway) * (7.5f + rage * 2.0f) * smooth((b->t - 2.0f) / 2.0f);
	float y = -26.0f + (16.5f + fsin(b->t * 1.1f) * 1.3f) * rise;
	float z = 200.0f + fsin(b->t * 0.37f) * 2.5f - rage * 2.0f;
	b->pos = f3(x, y, z);
	b->taunt -= dt;
	if (b->taunt <= 0) {
		static const char *const lines[5] = { "MWAHAHA!", "TRICK OR TREAT!", "YOU CAN'T CATCH ME!", "OOH, THAT TICKLES!", "BOO!" };
		b->taunt = 5.5f;
		b->taunt_text = lines[((int) b->t / 5) % 5];
		sfx(SFX_LAUGH, 0.7f, 0, 0.95f + fractf(b->t * 0.37f) * 0.15f);
	}
	// Summon helpers at 75%, 50% and 25% health.
	static const float thresholds[3] = { 0.75f, 0.5f, 0.25f };
	if (b->summons_done < 3 && b->hp / b->max_hp < thresholds[b->summons_done]) {
		int wave = b->summons_done++;
		b->taunt = 3.0f;
		b->taunt_text = "GET THEM, MY BATS!";
		sfx(SFX_LAUGH, 0.8f, 0, 1.1f);
		if (G.n_targets < 12) {
			// The recorded waves (for two riders); one rider gets one helper fewer.
			int seen = 0, first = 0;
			uint32_t t_prev = 0xffffffff;
			for (int i = 0; i < n_summon; i++) {
				if (spawn_summon[i].t_ms != t_prev) {
					if (seen == wave + 1)
						break;
					seen++;
					first = i;
					t_prev = spawn_summon[i].t_ms;
				}
				if (seen == wave + 1 && i - first < 3 + joined_count())
					spawn_rec(&spawn_summon[i], b->pos);
			}
		}
	}
	if (b->defeated) {
		G.state = S_VICTORY;
		G.state_t = 0;
	}
}

static void hit_boss(int pi, F3 at, int have_at) {
	Boss *b = &G.boss;
	const uint8_t *color = PLAYER_COLORS[pi];
	b->hp -= 1;
	b->flash = 1;
	b->wobble = 1;
	int final_hit = b->hp <= 0;
	Player *p = &G.players[pi];
	p->hits++;
	p->counts[BOSS_COUNT]++;
	int pts = final_hit ? 1000 : 25;
	sfx(SFX_BONK, 0.55f, 0, 0.9f + pi * 0.1f + (1.0f - b->hp / b->max_hp) * 0.3f);
	if (final_hit)
		sfx(SFX_BOOM, 1.0f, 0, 1);
	p->score += pts;
	char txt[20];
	if (final_hit)
		strcpy(txt, "+1000 FINAL HIT!");
	else
		sprintf(txt, "+%d", pts);
	popup(f3_add(b->pos, f3(rng_range(&G.rng, -2, 2), rng_range(&G.rng, 0, 3), -3)), txt, color, final_hit);
	if (!have_at)
		at = f3_add(b->pos, f3(rng_range(&G.rng, -2.5f, 2.5f), rng_range(&G.rng, -1, 3), -3.5f));
	int first = G.n_particles;
	burst(at, 8, color, 0);
	// None of the sparks fly back into him.
	F3 toward = f3_norm(f3_sub(G.eye, b->pos));
	V3 tw = fine(toward);
	for (int i = first; i < G.n_particles; i++) {
		Particle *q = &G.particles[i];
		int into = (q->vel.x * tw.x + q->vel.y * tw.y + q->vel.z * tw.z) / FINE;
		if (into < 0) {
			q->vel.x -= (tw.x * into * 2) / FINE;
			q->vel.y -= (tw.y * into * 2) / FINE;
			q->vel.z -= (tw.z * into * 2) / FINE;
		}
	}
	if (final_hit) {
		b->defeated = 1;
		for (int k = 0; k < 6; k++)
			burst(f3_add(b->pos, f3(rng_range(&G.rng, -3, 3), rng_range(&G.rng, -2, 4), rng_range(&G.rng, -3, 3))), 16, color, 1);
		G.n_targets = 0;
		set_banner("YOU BEAT THE PUMPKIN KING!", 0);
		G.shake = 1;
		for (int i = 0; i < NP; i++)
			if (G.players[i].joined)
				G.players[i].score += 500;
	}
}

// ------------------------------------------------------------------ shooting

static int in_rect(float x, float y, float rx, float ry, float rw, float rh) {
	return x >= rx && x < rx + rw && y >= ry && y < ry + rh;
}

static void key_rect(int k, float *x, float *y, float *cw, float *ch) {
	float w = SCREEN_W, h = SCREEN_H;
	int rows = (28 + KEY_COLS - 1) / KEY_COLS;
	float gap = 4;
	float c_h = (h * 0.56f - gap * (rows - 1)) / rows;
	float c_w = minf((w * 0.9f - gap * (KEY_COLS - 1)) / KEY_COLS, c_h * 1.6f);
	c_h = minf(c_h, c_w);
	float gw = c_w * KEY_COLS + gap * (KEY_COLS - 1);
	float x0 = (w - gw) * 0.5f, y0 = h * 0.2f;
	*x = x0 + (k % KEY_COLS) * (c_w + gap);
	*y = y0 + (k / KEY_COLS) * (c_h + gap);
	*cw = c_w, *ch = c_h;
}

static void initials_shot(int pi, float ax, float ay) {
	if (G.state_t < 0.6f)
		return;
	int k = -1;
	for (int i = 0; i < 28; i++) {
		float x, y, w, h;
		key_rect(i, &x, &y, &w, &h);
		if (in_rect(ax, ay, x, y, w, h))
			k = i;
	}
	if (k < 0)
		return;
	Initials *e = &G.initials[pi];
	if (!e->active || e->done_at >= 0)
		return;
	int n = strlen(e->letters);
	int ok = 0;
	if (k == KEY_DEL) {
		if (n) e->letters[n - 1] = 0, ok = 1;
	} else if (k == KEY_END) {
		if (n) e->done_at = G.state_t, ok = 1;
	} else if (n < MAX_INITIALS) {
		e->letters[n] = KEYS[k][0];
		e->letters[n + 1] = 0;
		ok = 1;
	}
	if (ok) {
		G.key_flash_t[k] = G.total_t;
		G.key_flash_p[k] = pi;
		sfx(k == KEY_END ? SFX_DING : SFX_POP, 0.6f, pan_x(ax), 0.9f + pi * 0.12f);
	} else {
		sfx(SFX_BONK, 0.4f, pan_x(ax), 1);
	}
}

static void shoot(int pi, float ax, float ay) {
	const uint8_t *color = PLAYER_COLORS[pi];
	if (G.state == S_INITIALS) {
		initials_shot(pi, ax, ay);
		return;
	}
	if (G.state == S_TUTORIAL) {
		float px, py;
		practice_pos(pi, &px, &py);
		// A light gun's first shot at its target also lines it up.
		float slack = 0;
		if (inputs[pi].dev == DEV_GUN) {
			input_calibrate(pi, (int) px, (int) py);
			slack = 48;
		}
		float dx = ax - px, dy = ay - py;
		float r = PRACTICE_R + slack;
		if (G.practice[pi] < 0 && dx * dx + dy * dy <= r * r) {
			G.practice[pi] = G.state_t;
			burst(f3_add(G.eye, f3_mul(ray_dir(px, py), 4.0f)), 20, color, 1);
			sfx(SFX_POP, 0.7f, pan_x(px), 0.9f + pi * 0.12f);
			sfx(SFX_POOF, 0.5f, pan_x(px), 1);
			if (inputs[pi].dev == DEV_GUN)
				G.players[pi].aim_x = G.players[pi].aim_tx = px, G.players[pi].aim_y = G.players[pi].aim_ty = py;
			return;
		}
	}
	// The nearest target under the crosshair.
	int best = -1;
	float best_w = 1e9f;
	for (int i = 0; i < G.n_targets; i++) {
		Target *t = &G.targets[i];
		if (t->scale < 0.3f)
			continue;
		float sx, sy, w;
		if (project(t->pos, &sx, &sy, &w)) {
			float r = pixel_radius(RADIUS[t->kind], w);
			float dx = sx - ax, dy = sy - ay;
			if (dx * dx + dy * dy <= r * r && w < best_w)
				best = i, best_w = w;
		}
	}
	int boss_hit = 0, blocked = -1;
	F3 boss_c = f3(0, 0, 0), hands[2];
	Boss *b = &G.boss;
	if (b->active && !b->defeated && b->t > 1.5f) {
		boss_c = f3_add(b->pos, f3(0, 0.5f, 0));
		float head_w = 1e9f, sx, sy, w;
		if (project(boss_c, &sx, &sy, &w)) {
			float r = pixel_radius(3.8f, w) * 0.9f;
			float dx = sx - ax, dy = sy - ay;
			if (dx * dx + dy * dy <= r * r && w < best_w) {
				boss_hit = 1;
				head_w = w;
				best = -1;
			}
		}
		F3 head;
		boss_parts(b, &head, hands);
		for (int k = 0; k < 2; k++) {
			if (project(hands[k], &sx, &sy, &w)) {
				float r = pixel_radius(1.2f, w);
				float dx = sx - ax, dy = sy - ay;
				if (dx * dx + dy * dy <= r * r && w < head_w && w < best_w) {
					blocked = k;
					boss_hit = 0;
					best = -1;
					head_w = w;
				}
			}
		}
	}
	if (best >= 0) {
		Target t = G.targets[best];
		G.targets[best] = G.targets[--G.n_targets];
		int pts = POINTS[t.kind];
		Player *p = &G.players[pi];
		p->score += pts;
		p->hits++;
		p->counts[t.kind]++;
		char txt[12];
		sprintf(txt, "+%d", pts);
		popup(f3_add(t.pos, f3(0, 0.5f, 0)), txt, color, t.kind == K_CANDY);
		burst(t.pos, t.kind == K_CANDY ? 28 : 16, color, t.kind == K_GHOST);
		sfx(SFX_POP, 0.7f, pan_x(ax), 0.9f + pi * 0.12f);
		if (t.kind == K_CANDY)
			sfx(SFX_BONUS, 0.7f, pan_x(ax), 1);
		else
			sfx(SFX_POOF, 0.5f, pan_x(ax), 1);
	} else if (boss_hit) {
		F3 d = ray_dir(ax, ay);
		float dist = maxf(f3_dot(f3_sub(boss_c, G.eye), d) - 3.8f - 1.5f, 1.0f);
		hit_boss(pi, f3_add(G.eye, f3_mul(d, dist)), 1);
	} else if (blocked >= 0) {
		F3 d = ray_dir(ax, ay);
		F3 at = f3_add(G.eye, f3_mul(d, maxf(f3_dot(f3_sub(hands[blocked], G.eye), d) - 1.2f, 1.0f)));
		static const uint8_t spark[3] = { 255, 255, 230 };
		sfx(SFX_DING, 0.35f, pan_x(ax), 0.6f);
		sparkle(at, 8, spark, 2.5f, 0.4f, 4 * FINE, 0.07f);
	} else {
		F3 at = f3_add(G.eye, f3_mul(ray_dir(ax, ay), 6.0f));
		sparkle(at, 5, color, 1.5f, 0.35f, 0, 0.06f);
	}
}

static void handle_shooting(float dt) {
	int can_shoot = G.state == S_TUTORIAL || G.state == S_RIDE || G.state == S_BOSS || G.state == S_INITIALS;
	int can_auto = G.state != S_INITIALS;
	for (int i = 0; i < NP; i++) {
		Player *p = &G.players[i];
		Input *in = &inputs[i];
		float k = 1.0f - fexp(-dt * 28.0f);
		p->aim_x += (p->aim_tx - p->aim_x) * k;
		p->aim_y += (p->aim_ty - p->aim_y) * k;
		p->cooldown -= dt;
		if (in->fire)
			p->held += dt;
		else
			p->held = 0;
		int fire_pressed = in->fire_pressed;
		// Guns aim where they last saw the screen.
		if (fire_pressed && in->dev == DEV_GUN) {
			p->aim_x = p->aim_tx = in->x;
			p->aim_y = p->aim_ty = in->y;
		}
		int auto_fire = can_auto && in->fire && p->held > 0.35f && p->cooldown <= 0 && in->aim_valid;
		if (!(p->joined && p->aim_visible && can_shoot && (fire_pressed || auto_fire) && p->cooldown <= 0))
			continue;
		p->cooldown = auto_fire ? 0.28f : 0.11f;
		p->shots++;
		p->kick = 1;
		sfx(SFX_SHOT, 0.18f, pan_x(p->aim_x), 1.0f + i * 0.07f);
		shoot(i, p->aim_x, p->aim_y);
	}
}

// ------------------------------------------------------------------ update

static void handle_sources(void) {
	for (int i = 0; i < NP; i++) {
		Player *p = &G.players[i];
		Input *in = &inputs[i];
		if (in->dev == DEV_NONE) {
			p->aim_visible = 0;
			continue;
		}
		// Any button but START joins (also mid-ride), except on the score screens.
		if (!p->joined && in->any_pressed && !in->start_pressed &&
				G.state != S_RESULTS && G.state != S_INITIALS && G.state != S_HIGHSCORES) {
			p->joined = 1;
			reset_score(p);
			sfx(SFX_JOIN, 0.6f, 0, 1.0f + i * 0.1f);
		}
		if (in->aim_valid) {
			p->aim_tx = in->x, p->aim_ty = in->y;
			if (!p->aim_visible)
				p->aim_x = p->aim_tx, p->aim_y = p->aim_ty;
			p->aim_visible = 1;
		} else {
			p->aim_visible = 0;
		}
	}
}

static int any_start(void) {
	for (int i = 0; i < NP; i++)
		if (inputs[i].start_pressed)
			return 1;
	return 0;
}

static int any_go(void) {
	for (int i = 0; i < NP; i++)
		if (inputs[i].start_pressed || inputs[i].fire_pressed || inputs[i].any_pressed)
			return 1;
	return 0;
}

static void update_camera(float dt) {
	Cam *c = &G.cam;
	F3 eye, dir, up;
	int speed16 = 0;
	switch (G.state) {
	case S_TITLE:
		cam_pose(POSE_TITLE, c);
		break;
	case S_TUTORIAL:
		cam_ride(0, c);
		break;
	case S_RIDE:
		cam_ride((int) (G.ride_t * 1000), c);
		speed16 = c->speed;
		break;
	case S_BOSS:
	case S_VICTORY:
		cam_pose(POSE_STOP, c);
		break;
	case S_OUTRO:
		cam_outro((int) (G.state_t * 1000), c);
		speed16 = c->speed;
		break;
	default:
		cam_pose(POSE_END, c);
		break;
	}
	eye = f3(c->eye.x * (1.0f / FINE), c->eye.y * (1.0f / FINE), c->eye.z * (1.0f / FINE));
	dir = f3(c->dir[0] / 4096.0f, c->dir[1] / 4096.0f, c->dir[2] / 4096.0f);
	up = f3(c->up[0] / 4096.0f, c->up[1] / 4096.0f, c->up[2] / 4096.0f);
	if (G.state == S_TITLE) {
		float s = G.state_t * 0.25f;
		dir = f3_norm(f3(fsin(s) * 0.25f, 0.08f + fsin(s * 0.7f) * 0.03f, 1.0f));
		up = f3(0, 1, 0);
	}
	F3 door = f3(0, DOOR_FLOOR + 6.5f, DOOR_Z);
	if (G.boss.active && (G.state == S_BOSS || G.state == S_VICTORY)) {
		F3 to = f3_norm(f3_sub(G.boss.pos, eye));
		F3 boss_look = f3_norm(f3_lerp(dir, to, 0.3f));
		float k = G.state == S_VICTORY ? smooth((G.state_t - 1.0f) / 1.5f) : 0;
		F3 door_look = f3_norm(f3_sub(door, eye));
		dir = f3_norm(f3_lerp(boss_look, door_look, k));
	}
	if (G.shake > 0) {
		float k = G.shake * 0.12f;
		eye = f3_add(eye, f3(fsin(G.total_t * 53.0f) * k, fcos(G.total_t * 61.0f) * k, 0));
	}
	float speed = speed16 / 16.0f;
	if (speed > 0.5f)
		eye.y += fsin(G.total_t * 23.0f) * 0.012f * minf(speed, 8.0f) / 8.0f;
	G.eye = eye, G.dir = dir, G.up = up;
	G.right = f3_norm(f3_cross(dir, up));
	G.up = f3_cross(G.right, dir);
	G.speed16 = speed16;

	// Zone banners and fog.
	int z = c->zone;
	if (z != G.zone && G.state == S_RIDE) {
		G.zone = z;
		set_banner(zone_name(z), 0);
		if (z != Z_DROP)
			sfx(SFX_WHOOSH, 0.5f, 0, 1);
	}
	if (G.state == S_TITLE)
		G.zone = Z_GRAVEYARD;
	float fc[3], fs, fe, lamp;
	zone_fog(z, fc, &fs, &fe, &lamp);
	float rate = 1.5f;
	int opening = G.state == S_VICTORY && G.state_t > DOOR_START;
	if (opening || G.state == S_OUTRO || G.state == S_RESULTS || G.state == S_INITIALS || G.state == S_HIGHSCORES) {
		fc[0] = fc[1] = fc[2] = 0;
		if (G.state == S_VICTORY)
			fs = 18, fe = 34;
		else
			fs = 8, fe = 26;
		rate = G.state == S_VICTORY ? 1.5f : 2.5f;
	}
	float k = 1.0f - fexp(-dt * rate);
	for (int i = 0; i < 3; i++)
		G.fog[i] += (fc[i] - G.fog[i]) * k;
	G.fog_start += (fs - G.fog_start) * k;
	G.fog_end += (fe - G.fog_end) * k;
	G.lamp += (lamp - G.lamp) * k;
}

static float cart_noise(void) {
	if (G.state == S_RIDE && !G.paused)
		return clampf(G.speed16 / 16.0f / 6.0f, 0, 1);
	if (G.state == S_OUTRO)
		return 0.6f;
	return 0;
}

static int music_for_state(void) {
	switch (G.state) {
	case S_TITLE: case S_TUTORIAL: return MUSIC_TITLE;
	case S_RIDE: return MUSIC_RIDE;
	case S_BOSS: return MUSIC_BOSS;
	case S_VICTORY: case S_OUTRO: return MUSIC_VICTORY;
	default: return MUSIC_RESULTS;
	}
}

// Bonus for good aim: nothing at half the shots hitting, rising to MAX_AIM_BONUS
// for every shot a hit.
static int accuracy_bonus(int hits, int shots) {
	if (shots == 0)
		return 0;
	float over = clampf(((float) hits / shots - 0.5f) / 0.5f, 0, 1);
	return (int) (MAX_AIM_BONUS * over * over / 10.0f + 0.5f) * 10;
}

static void update(float dt) {
	G.total_t += dt;
	handle_sources();
	int any_pause = 0, any_back = 0;
	for (int i = 0; i < NP; i++) {
		if (G.players[i].joined && inputs[i].start_pressed)
			any_pause = 1;
		if (inputs[i].back_pressed)
			any_back = 1;
	}
	if (any_pause && (G.state == S_RIDE || G.state == S_BOSS))
		G.paused = !G.paused;
	if (G.paused) {
		if (any_back)
			go_to_title();
		update_camera(0);
		return;
	}
	G.state_t += dt;

	switch (G.state) {
	case S_TITLE: {
		if (joined_count() > 0 && G.title_join_t < 0)
			G.title_join_t = 0;
		if (G.title_join_t >= 0)
			G.title_join_t += dt;
		int start = 0;
		for (int i = 0; i < NP; i++)
			if (G.players[i].joined && inputs[i].start_pressed && G.title_join_t > 0.2f)
				start = 1;
		if (start || G.title_join_t > 15.0f) {
			G.title_join_t = -1;
			start_tutorial();
		}
		break;
	}
	case S_TUTORIAL: {
		float last = 0;
		int all = 1, any = 0;
		for (int i = 0; i < NP; i++) {
			if (!G.players[i].joined)
				continue;
			any = 1;
			if (G.practice[i] < 0)
				all = 0;
			else
				last = maxf(last, G.practice[i]);
		}
		int skip = any_start() && G.state_t > 0.5f;
		if (!any || skip || (all && G.state_t - last > 0.8f))
			start_ride();
		break;
	}
	case S_RIDE: {
		float before = G.ride_t;
		G.ride_t += dt;
#ifdef HH_TEST_BOSS
		if (G.ride_t > 4 && G.ride_t < RIDE_SECONDS - 1)
			G.ride_t = RIDE_SECONDS - 1, G.next_spawn = G.n_spawns, before = G.ride_t;   // (test builds: skip the ride)
#endif
		for (int k = 0; k < 4; k++) {
			float at = (float) k;
			if ((before < at + 0.001f && G.ride_t >= at + 0.001f && before > 0) || (k == 0 && before == 0))
				sfx(k == 3 ? SFX_GO : SFX_BEEP, 0.6f, 0, 1);
		}
		int now_ms = (int) (G.ride_t * 1000);
		while (G.next_spawn < G.n_spawns && (int) G.spawns[G.next_spawn].t_ms <= now_ms)
			spawn_rec(&G.spawns[G.next_spawn++], f3(0, 0, 0));
		if (G.ride_t >= RIDE_SECONDS) {
			G.ride_t = RIDE_SECONDS;
			start_boss();
		}
		break;
	}
	case S_BOSS:
		update_boss(dt);
		break;
	case S_VICTORY: {
		float before = G.state_t - dt;
		if (before < DOOR_START && G.state_t >= DOOR_START) {
			sfx(SFX_RUMBLE, 0.9f, 0, 0.8f);
			G.shake = 0.4f;
		}
		G.door_open = smooth((G.state_t - DOOR_START) / DOOR_TIME);
		if (G.state_t > DOOR_START + DOOR_TIME + 0.3f) {
			G.state = S_OUTRO;
			G.state_t = 0;
		}
		break;
	}
	case S_OUTRO:
		G.door_open = 1;
		if (G.state_t > OUTRO_TIME) {
			G.state = S_RESULTS;
			G.state_t = 0;
			for (int i = 0; i < NP; i++) {
				Player *p = &G.players[i];
				if (p->joined) {
					p->bonus = accuracy_bonus(p->hits, p->shots);
					p->score += p->bonus;
				}
			}
		}
		break;
	case S_RESULTS: {
		int rows = joined_count();
		for (int rank = 0; rank < rows; rank++) {
			float at = 0.4f * (rows - rank);
			if (G.state_t - dt < at && G.state_t >= at)
				sfx(SFX_DING, 0.5f, 0, 1.0f + 0.12f * (rows - rank));
		}
		if ((any_go() && G.state_t > 4.0f) || G.state_t > 45.0f)
			leave_results();
		break;
	}
	case S_INITIALS: {
		float last = 0;
		int all = 1;
		for (int i = 0; i < NP; i++) {
			if (!G.initials[i].active)
				continue;
			if (G.initials[i].done_at < 0)
				all = 0;
			else
				last = maxf(last, G.initials[i].done_at);
		}
		if ((all && G.state_t - last > 0.8f) || G.state_t > INITIALS_SECONDS)
			finish_initials();
		break;
	}
	case S_HIGHSCORES:
		if ((any_go() && G.state_t > 2.0f) || G.state_t > 12.0f)
			go_to_title();
		break;
	}

	// Targets.
	for (int i = 0; i < G.n_targets; i++) {
		Target *t = &G.targets[i];
		t->age += dt;
		if (t->age > t->life)
			t->leave += dt;
		float appear = smooth(t->age / 0.35f) * (1.0f - smooth(t->leave / 0.4f));
		t->scale = t->kind == K_PORTRAIT ? 1.0f - smooth(t->leave / 0.4f) : appear;
		F3 pos = motion(t, t->age);
		if (t->kind == K_PUMPKIN)
			pos.y -= (1.0f - appear) * 0.9f;
		else if (t->kind == K_SKELETON)
			pos.y -= (1.0f - appear) * 1.7f;
		else if (t->kind == K_SPIDER) {
			float top = t->a.y - 0.4f;
			pos.y = top + (pos.y - top) * appear;
		}
		t->pos = pos;
		if (t->kind == K_BAT || t->kind == K_CANDY)
			t->scale = smooth(t->age / 0.25f) * (1.0f - smooth(t->leave / 0.4f));
		if (t->leave > 0.4f)
			t->dead = 1;
		F3 to = f3_sub(t->pos, G.eye);
		if (G.state == S_RIDE && f3_dot(to, to) > 9.0f && f3_dot(to, G.dir) < -1.0f)
			t->dead = 1;
	}
	for (int i = 0; i < G.n_targets;) {
		if (G.targets[i].dead)
			G.targets[i] = G.targets[--G.n_targets];
		else
			i++;
	}

	// Sparks and popups.
	int dt_ms = (int) (dt * 1000);
	int dt12 = (int) (dt * 4096);
	int drag = 4096 - (int) (0.6f * dt * 4096);
	for (int i = 0; i < G.n_particles;) {
		Particle *q = &G.particles[i];
		q->age += dt_ms;
		if (q->age >= q->life) {
			*q = G.particles[--G.n_particles];
			continue;
		}
		q->vel.y -= (q->gravity * dt12) >> 12;
		q->vel.x = (q->vel.x * drag) >> 12;
		q->vel.y = (q->vel.y * drag) >> 12;
		q->vel.z = (q->vel.z * drag) >> 12;
		q->pos.x += (q->vel.x * dt12) >> 12;
		q->pos.y += (q->vel.y * dt12) >> 12;
		q->pos.z += (q->vel.z * dt12) >> 12;
		i++;
	}
	for (int i = 0; i < G.n_popups;) {
		Popup *p = &G.popups[i];
		p->age += dt;
		p->pos.y += dt * 0.9f;
		if (p->age >= 1.3f) {
			memmove(p, p + 1, sizeof(Popup) * (G.n_popups - i - 1));
			G.n_popups--;
			continue;
		}
		i++;
	}
	G.banner_t += dt;
	G.shake = maxf(G.shake - dt, 0);
	for (int i = 0; i < NP; i++) {
		Player *p = &G.players[i];
		p->shown += (p->score - p->shown) * (1.0f - fexp(-dt * 6.0f));
		if (absf(p->score - p->shown) < 1)
			p->shown = p->score;
		p->kick = maxf(p->kick - dt * 5.0f, 0);
	}
	update_camera(dt);
	handle_shooting(dt);
}

// ------------------------------------------------------------------ drawing the world

static void draw_targets(void) {
	for (int i = 0; i < G.n_targets; i++) {
		Target *t = &G.targets[i];
		float s = maxf(t->scale, 0.001f);
		F3 to_cam = f3_sub(G.eye, t->pos);
		float yaw = fatan2(to_cam.x, to_cam.z);
		int light[3];
		light_at(t->pos, light);
		float a = t->age;
		float hl = 0.12f + 0.12f * fsin(a * 6.0f + t->phase);
		Xf base;
		xf_at(&base, t->pos);
		xf_rot(&base, 1, yaw);
		float ks = s * KIND_SIZE[t->kind];
		xf_scale(&base, ks, ks, ks);
		Xf m;
		switch (t->kind) {
		case K_GHOST: {
			static const int crypt_tint[3] = { 2867, 4096, 3277 };
			m = base;
			xf_rot(&m, 2, fsin(a * 2.0f + t->phase) * 0.12f);
			xf_move(&m, 0, -gfx_model(M_GHOST)->size[1] * 0.5f / MODEL_UNIT, 0);
			draw_model(M_GHOST, &m, light, hl, zone_at(t->pos) == Z_CRYPT ? crypt_tint : 0);
			break;
		}
		case K_PUMPKIN:
			m = base;
			xf_rot(&m, 2, fsin(a * 3.0f + t->phase) * 0.12f);
			xf_move(&m, 0, -0.56f, 0);
			draw_model(M_PUMPKIN, &m, light, hl, 0);
			break;
		case K_BAT: {
			m = base;
			xf_move(&m, 0, -0.3f, 0);
			draw_model(M_BAT_BODY, &m, light, hl, 0);
			float flap = fsin(a * 26.0f + t->phase) * 0.8f;
			m = base;
			xf_move(&m, -0.27f, 0, 0);
			xf_rot(&m, 2, flap);
			xf_move(&m, -0.36f, 0, 0);
			draw_model(M_BAT_WING, &m, light, hl, 0);
			m = base;
			xf_move(&m, 0.27f, 0, 0);
			xf_rot(&m, 2, -flap);
			xf_rot(&m, 1, PI);
			xf_move(&m, -0.36f, 0, 0);
			draw_model(M_BAT_WING, &m, light, hl, 0);
			break;
		}
		case K_CANDY: {
			xf_at(&m, t->pos);
			xf_rot(&m, 1, a * 5.0f);
			xf_rot(&m, 0, fsin(a * 3.0f) * 0.5f);
			xf_scale(&m, s, s, s);
			xf_move(&m, 0, -0.26f, 0);
			draw_model(M_CANDY, &m, light, hl, 0);
			static const uint8_t cols[3][3] = { { 255, 64, 204 }, { 64, 242, 255 }, { 153, 255, 64 } };
			for (int k = 0; k < 3; k++) {
				float b = a + k * 2.1f;
				if (((int) (b * 10.0f)) % 2 == 0) {
					F3 sp = f3_add(t->pos, f3(fsin(b * 7.0f) * 0.7f, fcos(b * 9.0f) * 0.4f, fcos(b * 5.0f) * 0.7f));
					V3 f = fine(sp);
					gfx_draw_spark(&f, (int) (0.16f * FINE), cols[k]);
				}
			}
			break;
		}
		case K_SKELETON: {
			m = base;
			xf_move(&m, 0, -0.85f, 0);
			draw_model(M_TORSO, &m, light, hl, 0);
			m = base;
			xf_move(&m, 0, -0.04f, 0);
			xf_rot(&m, 2, fsin(a * 4.0f) * 0.15f);
			draw_model(M_SKULL, &m, light, hl, 0);
			float wave = fsin(a * 8.0f) * 0.6f + 2.4f;
			m = base;
			xf_move(&m, -0.38f, -0.12f, 0);
			xf_rot(&m, 2, -wave);
			xf_move(&m, 0, -0.8f, 0);
			draw_model(M_ARM, &m, light, hl, 0);
			m = base;
			xf_move(&m, 0.38f, -0.12f, 0);
			xf_rot(&m, 2, 0.15f);
			xf_move(&m, 0, -0.8f, 0);
			draw_model(M_ARM, &m, light, hl, 0);
			break;
		}
		case K_SPIDER: {
			static const float thread_c[3] = { 0.85f, 0.85f, 0.9f };
			static const float leg_c[3] = { 0.2f, 0.15f, 0.24f };
			float len = maxf(t->a.y - t->pos.y, 0.05f);
			box_at(f3(t->pos.x, t->pos.y + len * 0.5f, t->pos.z), 0.03f, len, 0.03f, -1, thread_c, light, 0.5f);
			m = base;
			xf_move(&m, 0, -0.22f, 0);
			draw_model(M_SPIDER, &m, light, hl, 0);
			for (int k = 0; k < 8; k++) {
				float side = k < 4 ? -1.0f : 1.0f;
				float j = (float) (k % 4);
				float wig = fsin(a * 9.0f + j * 1.3f + side) * 0.18f;
				float up_ang = 0.6f + wig;
				Xf hip = base;
				xf_move(&hip, side * 0.34f, 0, 0.3f - j * 0.2f);
				xf_rot(&hip, 1, side * (0.45f - j * 0.3f));
				m = hip;
				xf_rot(&m, 2, side * up_ang);
				xf_move(&m, side * 0.18f, 0, 0);
				xf_scale(&m, 0.38f, 0.06f, 0.06f);
				draw_box(&m, -1, leg_c, light, 0.25f);
				m = hip;
				xf_move(&m, side * 0.36f * fcos(up_ang), 0.36f * fsin(up_ang), 0);
				xf_rot(&m, 2, -side * (1.15f - wig));
				xf_move(&m, side * 0.24f, 0, 0);
				xf_scale(&m, 0.5f, 0.05f, 0.05f);
				draw_box(&m, -1, leg_c, light, 0.25f);
			}
			break;
		}
		case K_PORTRAIT: {
			F3 n = f3_norm(f3_sub(t->a, t->home));
			float peel = smooth(a / PORTRAIT_PEEL);
			F3 pos = a < PORTRAIT_PEEL ? f3_add(t->home, f3_mul(n, 0.07f + 0.63f * peel)) : t->pos;
			float wall_yaw = fatan2(n.x, n.z);
			float turn = smooth((a - 0.3f) / 1.2f);
			float d = yaw - wall_yaw + PI;
			while (d < 0) d += TAU;
			while (d >= TAU) d -= TAU;
			d -= PI;
			float sway = fsin(a * 1.9f + t->phase) * 0.14f * turn;
			Xf mm;
			xf_at(&mm, pos);
			xf_rot(&mm, 1, wall_yaw + d * turn);
			xf_rot(&mm, 2, sway);
			xf_rot(&mm, 0, -0.25f * peel * (1.0f - turn));
			xf_scale(&mm, s, s, s);
			static const float one[3] = { 1, 1, 1 };
			m = mm;
			xf_scale(&m, 1.2f, 1.5f, 0.12f);
			// The painting on the front, gold frame elsewhere.
			draw_box(&m, t->variant % 2 == 0 ? T_PORTRAIT : T_PORTRAIT2, one, light, hl);
			float glare = 0.6f + 0.4f * absf(fsin(a * 7.0f + t->phase));
			float eye_c[3] = { 1.0f, 0.25f * glare, 0.15f * glare };
			for (int e = 0; e < 2; e++) {
				m = mm;
				xf_move(&m, e ? 0.1125f : -0.0375f, 0.14f, 0.065f);
				xf_scale(&m, 0.085f, 0.085f, 0.02f);
				draw_box(&m, -1, eye_c, light, 1.0f);
			}
			break;
		}
		case K_BOOK: {
			float open = 0.35f + 0.9f * (0.5f + 0.5f * fsin(a * 10.0f + t->phase));
			Xf flight = base;
			xf_rot(&flight, 0, 0.4f);
			int half = M_BOOK0 + t->variant % 3;
			m = flight;
			xf_rot(&m, 2, open);
			xf_move(&m, 0.25f, -0.2f, 0);
			draw_model(half, &m, light, hl, 0);
			m = flight;
			xf_rot(&m, 1, PI);
			xf_rot(&m, 2, open);
			xf_move(&m, 0.25f, -0.2f, 0);
			draw_model(half, &m, light, hl, 0);
			break;
		}
		}
	}
}

static void draw_boss(void) {
	Boss *b = &G.boss;
	if (!b->active)
		return;
	F3 head, hands[2];
	boss_parts(b, &head, hands);
	float s = 1, spin = 0;
	if (G.state == S_VICTORY || G.state == S_OUTRO) {
		float k = G.state == S_VICTORY ? G.state_t : 99;
		s = maxf(1.0f - k / 1.6f, 0);
		spin = k * 8.0f;
	}
	if (s <= 0)
		return;
	F3 to_cam = f3_sub(G.eye, head);
	float yaw = fatan2(to_cam.x, to_cam.z) + spin;
	float wob = fsin(b->t * 25.0f) * 0.15f * b->wobble;
	static const int light[3] = { 3891, 3891, 3891 };
	float rage = 1.0f - b->hp / b->max_hp;
	float horiz = maxf(fsqrt(to_cam.x * to_cam.x + to_cam.z * to_cam.z), 0.1f);
	float pitch = clampf(fatan2(head.y - G.eye.y, horiz), 0, 0.6f);
	Xf m;
	xf_at(&m, head);
	xf_rot(&m, 1, yaw);
	xf_rot(&m, 0, pitch);
	xf_rot(&m, 2, wob + fsin(b->t * 1.3f) * 0.06f);
	xf_scale(&m, s, s, s);
	xf_move(&m, 0, -3.2f, 0);
	draw_model(rage > 0.5f ? M_BOSS_HEAD_ANGRY : M_BOSS_HEAD, &m, light, b->flash * 0.8f, 0);
	xf_at(&m, f3_sub(head, f3(0, 3.4f * s, 0)));
	xf_rot(&m, 1, yaw);
	xf_scale(&m, s, s, s);
	xf_move(&m, 0, -4.4f, 0);
	draw_model(M_BOSS_CAPE, &m, light, b->flash * 0.5f, 0);
	for (int k = 0; k < 2; k++) {
		float side = k == 0 ? -1.0f : 1.0f;
		xf_at(&m, f3_add(head, f3_mul(f3_sub(hands[k], head), s)));
		xf_rot(&m, 1, yaw);
		xf_rot(&m, 2, side * (0.3f + fsin(b->t * 3.0f + k) * 0.4f));
		xf_scale(&m, s, s, s);
		xf_move(&m, 0, -1.0f, 0);
		draw_model(M_BOSS_HAND, &m, light, b->flash * 0.6f, 0);
	}
}

static int near_cam(F3 p, float r) {
	F3 d = f3_sub(p, G.eye);
	return f3_dot(d, d) < r * r;
}

static void draw_scenery(void) {
	float t = G.total_t;
	static const int full[3] = { 4096, 4096, 4096 };
	for (int i = 0; i < n_fires; i++) {
		F3 f = f3(fires[i].vx * (1.0f / UNIT), fires[i].vy * (1.0f / UNIT), fires[i].vz * (1.0f / UNIT));
		if (!near_cam(f, 22))
			continue;
		float seed = i * 1.7f;
		for (int k = 0; k < 5; k++) {
			float ph = seed + k * 2.3f;
			float ox = (k - 2.0f) * 0.17f + fsin(t * 3.1f + ph) * 0.04f;
			float oz = ((k * 7 % 5) - 2.0f) * 0.12f;
			float h = 0.45f + 0.28f * absf(fsin(t * 7.3f + ph)) + 0.12f * fsin(t * 13.7f + ph * 1.3f);
			float w = 0.2f + 0.04f * fsin(t * 9.0f + ph);
			float sway = fsin(t * 4.0f + ph) * 0.05f;
			F3 base = f3_add(f, f3(ox, 0, oz));
			static const float body[3] = { 1.0f, 0.42f, 0.06f }, tip[3] = { 1.0f, 0.86f, 0.32f };
			box_at(f3_add(base, f3(sway * 0.5f, h * 0.35f, 0)), w, h * 0.7f, w, -1, body, full, 1);
			box_at(f3_add(base, f3(sway, h * 0.8f, 0)), w * 0.6f, h * 0.4f, w * 0.6f, -1, tip, full, 1);
		}
		for (int k = 0; k < 3; k++) {
			float ph = seed * 3.1f + k * 1.9f;
			float u = fractf(t * 0.7f + ph * 0.37f);
			F3 p = f3_add(f, f3(fsin(ph * 5.0f) * 0.3f + fsin(t * 2.0f + ph) * 0.08f, 0.5f + u * 1.4f, fcos(ph * 3.0f) * 0.25f));
			uint8_t c[3] = { 255, (uint8_t) ((0.6f - 0.3f * u) * 255), 38 };
			V3 fp = fine(p);
			gfx_draw_spark(&fp, (int) (0.14f * (1.0f - u) * FINE), c);
		}
	}
	for (int i = 0; i < n_cauldrons; i++) {
		F3 c = f3(cauldrons[i].vx * (1.0f / UNIT), cauldrons[i].vy * (1.0f / UNIT), cauldrons[i].vz * (1.0f / UNIT));
		if (!near_cam(c, 22))
			continue;
		float seed = i * 2.9f;
		float level = 0.735f + 0.012f * fsin(t * 2.3f + seed);
		static const float one[3] = { 1, 1, 1 };
		box_at(f3_add(c, f3(0, level, 0)), 0.76f, 0.04f, 0.76f, T_SLIME, one, full, 1);
		for (int k = 0; k < 6; k++) {
			float ph = seed + k * 1.37f;
			float rate = 0.8f + 0.5f * absf(fsin(ph * 3.7f));
			float cyc = t * rate + ph;
			float n = (float) (int) cyc, u = cyc - n;
			float a = fsin(n * 2.39f + ph * 5.1f) * 3.14f;
			float r = 0.08f + 0.22f * absf(fsin(n * 1.71f + ph));
			float sz = 0.05f + 0.13f * u * u;
			float g[3] = { 0.75f + 0.25f * u, 0.75f + 0.25f * u, 0.75f + 0.25f * u };
			box_at(f3_add(c, f3(fcos(a) * r, level + sz * 0.3f, fsin(a) * r)), sz, sz * 0.8f, sz, T_SLIME, g, full, 1);
		}
		for (int k = 0; k < 4; k++) {
			float ph = seed * 1.3f + k * 2.2f;
			float u = fractf(t * 0.45f + ph * 0.29f);
			F3 p = f3_add(c, f3(fsin(ph * 4.0f) * 0.2f + fsin(t * 1.5f + ph) * 0.1f * u, 0.8f + u * 1.3f, fcos(ph * 2.0f) * 0.2f));
			uint8_t col[3] = { (uint8_t) (48 * (1 - u)), (uint8_t) (191 * (1 - u)), (uint8_t) (77 * (1 - u)) };
			V3 fp = fine(p);
			gfx_draw_spark(&fp, (int) ((0.16f + 0.28f * u) * FINE), col);
		}
	}
	// Mist: soft puffs that wander and breathe, only in the zone the cart is in.
	// (Only the nearest: big see-through puffs are slow for the GPU to fill.)
	gfx_puffs_begin();
	int ex = (int) (G.eye.x * UNIT), ez = (int) (G.eye.z * UNIT);
	int drawn = 0;
	for (int i = 0; i < n_mist && drawn < 32; i++) {
		const Mist *p = &mists[i];
		if (!(p->zones & (1 << G.zone)))
			continue;
		int dx = p->pos[0] - ex, dz = p->pos[2] - ez;
		if (dx < -12 * UNIT || dx > 12 * UNIT || dz < -12 * UNIT || dz > 12 * UNIT)
			continue;
		drawn++;
		float ph = p->ph / 4096.0f, wander = p->wander;
		float a = p->alpha / 255.0f + 0.08f * fsin(t * 0.35f + ph * 2.3f);
		int y = p->pos[1];
		if (p->rise) {
			float u = fractf(t * 0.22f + ph);
			y += (int) (u * p->rise);
			a *= fsin(u * PI);
		}
		if (a <= 0)
			continue;
		SVECTOR c = {
			(int16_t) (p->pos[0] + (int) (fsin(t * 0.11f + ph) * wander)), (int16_t) y,
			(int16_t) (p->pos[2] + (int) (fcos(t * 0.08f + ph * 1.7f) * wander * 0.8f)), 0
		};
		int k = (int) (minf(a * 1.2f, 1.0f) * 256);
		uint8_t col[3] = { (uint8_t) ((p->tint[0] * k) >> 8), (uint8_t) ((p->tint[1] * k) >> 8), (uint8_t) ((p->tint[2] * k) >> 8) };
		gfx_puff(&c, p->hx, p->hy, col);
	}
}

static void draw_doors(void) {
	if (!(G.eye.z > 150.0f && G.eye.y < -5.0f))
		return;
	float swing = G.door_open * 1.75f;
	int light[3];
	light_at(f3(0, DOOR_FLOOR + 3.0f, DOOR_Z - 1.5f), light);
	for (int i = 0; i < 3; i++)
		if (light[i] < 1434) light[i] = 1434;
	Xf m;
	xf_at(&m, f3(-DOOR_HALF, DOOR_FLOOR, DOOR_Z));
	xf_rot(&m, 1, -swing);
	xf_move(&m, DOOR_HALF * 0.5f, 0, 0);
	draw_model(M_DOOR_L, &m, light, 0, 0);
	xf_at(&m, f3(DOOR_HALF, DOOR_FLOOR, DOOR_Z));
	xf_rot(&m, 1, swing);
	xf_move(&m, -DOOR_HALF * 0.5f, 0, 0);
	draw_model(M_DOOR_R, &m, light, 0, 0);
}

static void draw_particles(void) {
	for (int i = 0; i < G.n_particles; i++) {
		Particle *q = &G.particles[i];
		int k = 4096 - (q->age << 12) / q->life;
		int size = (q->size * (1638 + ((2458 * k) >> 12))) >> 12;
		gfx_draw_spark(&q->pos, size, q->color);
	}
}

// ------------------------------------------------------------------ HUD

static void text(const char *s, float x, float y, float scale, const uint8_t *c, int align) {
	hud_text(s, (int) x, (int) y, fix(scale), c, align);
}

static void text_fit(const char *s, float x, float y, float scale, float max_w, const uint8_t *c, int align) {
	hud_text_fit(s, (int) x, (int) y, fix(scale), (int) max_w, c, align);
}

// Shadowed text, like the PC's: a dark copy one pixel down and right.
static void stext(const char *s, float x, float y, float scale, const uint8_t *c, int align) {
	text(s, x + 1, y + 1, scale, BLACK, align);
	text(s, x, y, scale, c, align);
}

static void stext_fit(const char *s, float x, float y, float scale, float max_w, const uint8_t *c, int align) {
	text_fit(s, x + 1, y + 1, scale, max_w, BLACK, align);
	text_fit(s, x, y, scale, max_w, c, align);
}

static void scaled(const uint8_t *c, float k, uint8_t *out) {
	for (int i = 0; i < 3; i++) {
		int v = (int) (c[i] * k);
		out[i] = v > 255 ? 255 : v < 0 ? 0 : v;
	}
}

static void panel(float x, float y, float w, float h, const uint8_t *fill, float alpha, const uint8_t *border) {
	hud_panel((int) x, (int) y, (int) w, (int) h, fill, (int) (alpha * 255), border);
}

static void draw_crosshair(int i) {
	Player *p = &G.players[i];
	if (G.state == S_OUTRO || G.state == S_RESULTS || G.state == S_HIGHSCORES || !p->aim_visible ||
			(!p->joined && G.state != S_TITLE))
		return;
	if (G.state == S_INITIALS && !(G.initials[i].active && G.initials[i].done_at < 0))
		return;
	const uint8_t *c = PLAYER_COLORS[i];
	uint8_t cc[3];
	scaled(c, p->joined ? 1.0f : 0.5f, cc);
	int size = (int) (16 * (1.0f + p->kick * 0.35f));
	hud_sprite(T_RING, (int) p->aim_x, (int) p->aim_y, size + 2, BLACK);
	hud_sprite(T_RING, (int) p->aim_x, (int) p->aim_y, size, cc);
	hud_rect((int) p->aim_x - 1, (int) p->aim_y - 1, 2, 2, WHITE, 255);
	char lbl[4] = { 'P', '1' + i, 0 };
	text(lbl, p->aim_x + size * 0.45f, p->aim_y + size * 0.3f, 1, cc, ALIGN_LEFT);
}

static void draw_scores(void) {
	float w = SCREEN_W, h = SCREEN_H;
	float pw = w / NP, ph = 18, pad = 3;
	for (int i = 0; i < NP; i++) {
		Player *p = &G.players[i];
		float x = i * pw + pad, bw = pw - pad * 2, y = h - ph - pad;
		const uint8_t *c = PLAYER_COLORS[i];
		uint8_t dark[3];
		if (p->joined) {
			scaled(c, 0.25f, dark);
			panel(x, y, bw, ph, dark, 0.8f, c);
			char lbl[4] = { 'P', '1' + i, 0 };
			text(lbl, x + 4, y + ph * 0.5f - 4, 1, c, ALIGN_LEFT);
			char sc[16];
			sprintf(sc, "%d", (int) (p->shown + 0.5f));
			stext(sc, x + bw - 4, y + ph * 0.5f - 6, 1.5f, WHITE, ALIGN_RIGHT);
		} else {
			scaled(c, 0.5f, dark);
			panel(x, y, bw, ph, BLACK, 0.35f, dark);
			uint8_t dim[3];
			scaled(c, 0.7f, dim);
			const char *msg = inputs[i].dev == DEV_GUN ? "PULL TRIGGER" : "PRESS X";
			text_fit(msg, x + bw * 0.5f, y + ph * 0.5f - 4, 1, bw - 8, dim, ALIGN_CENTER);
		}
	}
}

static void draw_title(void) {
	float w = SCREEN_W, h = SCREEN_H, t = G.state_t;
	float bob = fsin(t * 2.0f) * 2.0f;
	hud_rect(0, (int) (h * 0.1f), (int) w, 52, BLACK, 90);
	static const uint8_t title_c[3] = { 255, 140, 20 }, sub_c[3] = { 191, 140, 255 }, gold[3] = { 255, 209, 51 };
	stext_fit("HAUNTED HOLLOW", w * 0.5f, h * 0.12f + bob, 3.5f, w * 0.94f, title_c, ALIGN_CENTER);
	stext_fit("THE SPOOKY COASTER RIDE", w * 0.5f, h * 0.12f + 34, 1.2f, w * 0.9f, sub_c, ALIGN_CENTER);
	if (G.n_high) {
		char line[40];
		sprintf(line, "TOP SCORE  %s  %d", G.high[0].initials, G.high[0].score);
		stext_fit(line, w * 0.5f, h * 0.12f + 52, 1, w * 0.9f, gold, ALIGN_CENTER);
	}
	float pw = w / NP, card_h = 40, y = h * 0.56f;
	for (int i = 0; i < NP; i++) {
		Player *p = &G.players[i];
		const uint8_t *c = PLAYER_COLORS[i];
		float x = i * pw + pw * 0.5f, bw = pw - 12, inner = bw - 8;
		float a = p->joined ? 1.0f : 0.55f;
		uint8_t fill[3], cc[3];
		scaled(c, 0.2f * a, fill);
		scaled(c, a, cc);
		panel(x - bw * 0.5f, y, bw, card_h, fill, 0.85f, cc);
		char head[12];
		sprintf(head, "PLAYER %d", i + 1);
		text_fit(head, x, y + 5, 1.1f, inner, cc, ALIGN_CENTER);
		text_fit(PLAYER_NAMES[i], x, y + 16, 1, inner, cc, ALIGN_CENTER);
		if (p->joined) {
			float pulse = 1.0f + 0.04f * fsin(t * 6.0f);
			text_fit("READY!", x, y + 27, 1.1f * pulse, inner, WHITE, ALIGN_CENTER);
		} else if (((int) (t * 1.5f)) % 2 == 0) {
			const char *msg = inputs[i].dev == DEV_GUN ? "PULL TRIGGER" : inputs[i].dev == DEV_PAD ? "PRESS X" : "NO CONTROLLER";
			text_fit(msg, x, y + 28, 1, inner, WHITE, ALIGN_CENTER);
		}
	}
	char msg[48];
	if (joined_count() == 0) {
		strcpy(msg, "PULL THE TRIGGER OR PRESS X TO JOIN");
	} else {
		float left = 15.0f - maxf(G.title_join_t, 0);
		sprintf(msg, "PRESS START (GUN: A) TO RIDE  (%d)", (int) (maxf(left, 0) + 0.99f));
	}
	static const uint8_t note[3] = { 255, 230, 128 };
	stext_fit(msg, w * 0.5f, h * 0.84f, 1, w * 0.94f, note, ALIGN_CENTER);
}

static void draw_tutorial(void) {
	float w = SCREEN_W, t = G.state_t;
	static const uint8_t head_c[3] = { 255, 153, 26 };
	stext_fit("SHOOT YOUR TARGET TO START!", w * 0.5f, SCREEN_H * 0.18f, 2, w * 0.92f, head_c, ALIGN_CENTER);
	float r = PRACTICE_R;
	for (int i = 0; i < NP; i++) {
		if (!G.players[i].joined)
			continue;
		const uint8_t *c = PLAYER_COLORS[i];
		float px, py;
		practice_pos(i, &px, &py);
		if (G.practice[i] < 0) {
			float bob = fsin(t * 3.0f + i) * 2.0f;
			float pulse = 1.0f + 0.06f * fsin(t * 5.0f + i);
			float x = px, y = py + bob;
			hud_sprite(T_RING, (int) x, (int) y, (int) (r * 2 * pulse) + 2, BLACK);
			hud_sprite(T_RING, (int) x, (int) y, (int) (r * 2 * pulse), c);
			hud_sprite(T_RING, (int) x, (int) y, (int) (r * 1.4f * pulse), WHITE);
			hud_sprite(T_RING, (int) x, (int) y, (int) (r * 0.8f * pulse), c);
			hud_rect((int) x - 2, (int) y - 2, 4, 4, WHITE, 255);
			char lbl[4] = { 'P', '1' + i, 0 };
			stext(lbl, x, y + r + 4, 1, c, ALIGN_CENTER);
		} else {
			float k = smooth((t - G.practice[i]) / 0.25f);
			float sc = 1.6f + (1.0f - k) * 0.8f;
			uint8_t cc[3];
			scaled(c, k, cc);
			stext_fit("READY!", px, py - 4 * sc, sc, w * 0.45f, cc, ALIGN_CENTER);
		}
	}
}

static const char *award(int i) {
	Player *p = &G.players[i];
	int best = 9, best_v = 0;
	for (int k = 0; k < 9; k++) {
		int v = k == BOSS_COUNT ? p->counts[k] / 3 : p->counts[k];
		if (v > best_v)
			best_v = v, best = k;
	}
	return AWARDS[best];
}

static void draw_results(void) {
	float w = SCREEN_W, h = SCREEN_H;
	hud_darken(240);
	static const uint8_t head_c[3] = { 255, 140, 20 };
	stext_fit("WHAT A SPOOKTACULAR RIDE!", w * 0.5f, h * 0.07f, 2, w * 0.94f, head_c, ALIGN_CENTER);
	int order[NP], n = 0;
	for (int i = 0; i < NP; i++)
		if (G.players[i].joined)
			order[n++] = i;
	for (int a = 0; a < n; a++)
		for (int b = a + 1; b < n; b++)
			if (G.players[order[b]].score > G.players[order[a]].score) {
				int t = order[a];
				order[a] = order[b], order[b] = t;
			}
	static const char *const medal[4] = { "1ST", "2ND", "3RD", "4TH" };
	static const uint8_t medal_c[4][3] = { { 255, 209, 51 }, { 204, 209, 230 }, { 217, 140, 77 }, { 153, 153, 179 } };
	int made[NP];
	int n_made = high_score_riders(made);
	float top = h * 0.22f, row_h = 50, gap = 8, rw = w * 0.92f, x = (w - rw) * 0.5f;
	for (int rank = 0; rank < n; rank++) {
		float appear = smooth((G.state_t - 0.4f * (n - rank)) / 0.4f);
		if (appear <= 0)
			continue;
		int i = order[rank];
		Player *p = &G.players[i];
		const uint8_t *c = PLAYER_COLORS[i];
		float y = top + rank * (row_h + gap);
		uint8_t fill[3], cc[3], mc[3], wc[3];
		scaled(c, 0.2f, fill);
		scaled(c, appear, cc);
		scaled(medal_c[rank], appear, mc);
		scaled(WHITE, appear, wc);
		panel(x, y, rw, row_h, fill, 0.9f, cc);
		text(medal[rank], x + 6, y + row_h * 0.5f - 8, 2, mc, ALIGN_LEFT);
		char name[12];
		sprintf(name, "PLAYER %d", i + 1);
		text(name, x + 62, y + 8, 1.4f, cc, ALIGN_LEFT);
		text_fit(award(i), x + 62, y + 28, 1, rw * 0.42f, wc, ALIGN_LEFT);
		float row_t = G.state_t - 0.4f * (n - rank);
		float tally = smooth((row_t - 0.8f) / 0.8f);
		char score[16];
		sprintf(score, "%d", p->score - p->bonus + (int) (p->bonus * tally));
		stext(score, x + rw - 6, y + 6, 2, wc, ALIGN_RIGHT);
		if (p->bonus > 0 && row_t > 0.5f) {
			char tag[24];
			sprintf(tag, "+%d AIM BONUS", p->bonus);
			uint8_t gc[3] = { 255, 217, 77 };
			scaled(gc, smooth((row_t - 0.5f) / 0.25f), gc);
			text(tag, x + rw - 6, y + 26, 1, gc, ALIGN_RIGHT);
		}
		char stats[24];
		sprintf(stats, "%d POPS %d%% AIM", p->hits, p->shots ? p->hits * 100 / p->shots : 0);
		uint8_t sc[3] = { 217, 217, 230 };
		scaled(sc, appear, sc);
		text(stats, x + rw - 6, y + 38, 1, sc, ALIGN_RIGHT);
		for (int m = 0; m < n_made; m++)
			if (made[m] == i) {
				float k = 0.75f + 0.25f * fsin(G.state_t * 6.0f);
				uint8_t f2[3] = { (uint8_t) (255 * k), (uint8_t) (128 * k), 13 };
				static const uint8_t b2[3] = { 255, 230, 102 };
				float tw = hud_text_width("NEW HIGH SCORE!", 4096) + 8;
				panel(x + rw * 0.5f - tw * 0.5f, y - 6, tw, 12, f2, 1, b2);
				text("NEW HIGH SCORE!", x + rw * 0.5f, y - 4, 1, WHITE, ALIGN_CENTER);
			}
	}
	if (G.state_t > 4.0f && ((int) (G.state_t * 2.0f)) % 2 == 0) {
		static const uint8_t note[3] = { 255, 230, 128 };
		stext_fit(n_made ? "SHOOT TO ENTER YOUR HIGH SCORE!" : "SHOOT TO RIDE AGAIN!", w * 0.5f, h * 0.9f, 1.3f, w * 0.92f, note, ALIGN_CENTER);
	}
}

static void draw_initials(void) {
	float w = SCREEN_W, h = SCREEN_H, t = G.state_t;
	hud_darken(240);
	float pulse = 1.0f + 0.04f * fsin(t * 5.0f);
	static const uint8_t head_c[3] = { 255, 140, 20 }, how_c[3] = { 217, 217, 242 }, red[3] = { 255, 102, 77 };
	stext_fit("NEW HIGH SCORE!", w * 0.5f, h * 0.03f, 2 * pulse, w * 0.9f, head_c, ALIGN_CENTER);
	text_fit("SHOOT THE LETTERS OF YOUR NAME, THEN END", w * 0.5f, h * 0.03f + 20, 1, w * 0.94f, how_c, ALIGN_CENTER);
	float left = INITIALS_SECONDS - t;
	if (left <= 15) {
		char s[8];
		sprintf(s, "%d", (int) (maxf(left, 0) + 0.99f));
		text(s, w - 4, h * 0.03f, 1.5f, red, ALIGN_RIGHT);
	}
	for (int k = 0; k < 28; k++) {
		float x, y, cw, ch;
		key_rect(k, &x, &y, &cw, &ch);
		int over = -1;
		for (int i = 0; i < NP; i++) {
			Player *p = &G.players[i];
			if (G.initials[i].active && G.initials[i].done_at < 0 && p->aim_visible && in_rect(p->aim_x, p->aim_y, x, y, cw, ch))
				over = i;
		}
		float flash = maxf(1.0f - (G.total_t - G.key_flash_t[k]) / 0.3f, 0);
		const uint8_t *fc = PLAYER_COLORS[G.key_flash_p[k]];
		uint8_t fill[3] = {
			(uint8_t) (31 + fc[0] * 0.6f * flash), (uint8_t) (15 + fc[1] * 0.6f * flash), (uint8_t) (46 + fc[2] * 0.6f * flash)
		};
		static const uint8_t def_border[3] = { 128, 82, 179 };
		const uint8_t *border = def_border;
		if (over >= 0) {
			border = PLAYER_COLORS[over];
			for (int c = 0; c < 3; c++)
				fill[c] = (uint8_t) minf(fill[c] + PLAYER_COLORS[over][c] * 0.15f, 255);
		}
		panel(x, y, cw, ch, fill, 0.95f, border);
		static const uint8_t del_c[3] = { 255, 115, 102 }, end_c[3] = { 115, 255, 128 };
		const uint8_t *col = k == KEY_DEL ? del_c : k == KEY_END ? end_c : WHITE;
		float sc = minf(ch * 0.5f / 8.0f, cw * 0.8f / (hud_text_width(KEYS[k], 4096) + 0.01f));
		text(KEYS[k], x + cw * 0.5f, y + (ch - 8 * sc) * 0.5f, sc, col, ALIGN_CENTER);
	}
	float pw = w / NP, pad = 3, ph = 36;
	for (int i = 0; i < NP; i++) {
		Initials *e = &G.initials[i];
		if (!e->active)
			continue;
		const uint8_t *c = PLAYER_COLORS[i];
		float x = i * pw + pad, bw = pw - pad * 2, y = h - ph - pad;
		uint8_t fill[3];
		scaled(c, 0.25f, fill);
		panel(x, y, bw, ph, fill, 0.9f, c);
		char head[20];
		sprintf(head, "P%d  %d", i + 1, G.players[i].score);
		text_fit(head, x + bw * 0.5f, y + 3, 1, bw - 6, c, ALIGN_CENTER);
		float sw = 13, sg = 3, sx = x + (bw - (sw * 3 + sg * 2)) * 0.5f, sy = y + 14;
		int n = strlen(e->letters);
		for (int k = 0; k < MAX_INITIALS; k++) {
			float bx = sx + k * (sw + sg);
			hud_rect((int) bx, (int) sy, (int) sw, (int) sw, BLACK, 128);
			if (k < n) {
				char ch[2] = { e->letters[k], 0 };
				text(ch, bx + sw * 0.5f, sy + 2, 1.2f, WHITE, ALIGN_CENTER);
			} else if (k == n && e->done_at < 0 && ((int) (t * 3.0f)) % 2 == 0) {
				hud_rect((int) bx + 2, (int) (sy + sw - 3), (int) sw - 4, 2, c, 255);
			}
		}
		if (e->done_at >= 0)
			text("DONE!", x + bw - 4, y + 18, 1, WHITE, ALIGN_RIGHT);
	}
}

static void draw_high_scores(void) {
	float w = SCREEN_W, h = SCREEN_H, t = G.state_t;
	hud_darken(240);
	static const uint8_t head_c[3] = { 255, 140, 20 }, gold[3] = { 255, 209, 51 }, plain[3] = { 230, 230, 242 };
	stext_fit("HIGH SCORES", w * 0.5f, h * 0.05f, 2.4f, w * 0.9f, head_c, ALIGN_CENTER);
	float top = h * 0.17f, row_h = h * 0.68f / TABLE_SIZE;
	float sc = 1.25f;
	float cw = 8 * sc;
	float x_rank = w * 0.5f - 15 * cw * 0.5f, x_name = w * 0.5f - 5 * cw * 0.5f, x_score = w * 0.5f + 15 * cw * 0.5f;
	for (int rank = 0; rank < G.n_high; rank++) {
		float a = smooth((t - 0.08f * rank) / 0.3f);
		if (a <= 0)
			continue;
		float y = top + rank * row_h + (row_h - 8 * sc) * 0.5f;
		int nw = -1;
		for (int j = 0; j < G.n_high_new; j++)
			if (G.high_new[j][0] == rank)
				nw = G.high_new[j][1];
		uint8_t col[3];
		if (nw >= 0) {
			const uint8_t *c = PLAYER_COLORS[nw];
			float k = 0.75f + 0.25f * fsin(t * 6.0f);
			for (int i = 0; i < 3; i++)
				col[i] = (uint8_t) ((c[i] * k + 255 * (1 - k)) * a);
			uint8_t f[3];
			scaled(c, 0.25f, f);
			hud_rect((int) (w * 0.08f), (int) (top + rank * row_h + row_h * 0.08f), (int) (w * 0.84f), (int) (row_h * 0.84f), f, 180);
			char lbl[4] = { 'P', '1' + nw, 0 };
			text(lbl, w * 0.1f, y, 1, c, ALIGN_LEFT);
		} else {
			scaled(rank == 0 ? gold : plain, a, col);
		}
		char s[16];
		sprintf(s, "%d.", rank + 1);
		text(s, x_rank, y, sc, col, ALIGN_LEFT);
		text(G.high[rank].initials, x_name, y, sc, col, ALIGN_LEFT);
		sprintf(s, "%d", G.high[rank].score);
		text(s, x_score, y, sc, col, ALIGN_RIGHT);
	}
	if (t > 2.0f && ((int) (t * 2.0f)) % 2 == 0) {
		static const uint8_t note[3] = { 255, 230, 128 };
		stext_fit("SHOOT TO RIDE AGAIN!", w * 0.5f, h * 0.9f, 1.3f, w * 0.9f, note, ALIGN_CENTER);
	}
}

static void draw_hud(void) {
	float w = SCREEN_W, h = SCREEN_H;
	for (int i = 0; i < G.n_popups; i++) {
		Popup *p = &G.popups[i];
		float sx, sy, z;
		if (project(p->pos, &sx, &sy, &z)) {
			float a = 1.0f - smooth((p->age - 0.8f) / 0.5f);
			float sc = (p->big ? 2.0f : 1.4f) * (1.0f + (1.0f - smooth(p->age / 0.15f)) * 0.6f);
			uint8_t c[3];
			scaled(p->color, a, c);
			text(p->text, sx, sy, sc, c, ALIGN_CENTER);
		}
	}
	switch (G.state) {
	case S_TITLE: draw_title(); break;
	case S_TUTORIAL: draw_tutorial(); break;
	case S_RESULTS: draw_results(); break;
	case S_INITIALS: draw_initials(); break;
	case S_HIGHSCORES: draw_high_scores(); break;
	}
	if (G.state == S_RIDE || G.state == S_BOSS || G.state == S_VICTORY || G.state == S_OUTRO) {
		if (G.state == S_RIDE && G.ride_t < 3.6f) {
			int n = 3 - (int) G.ride_t;
			char txt[4];
			static const uint8_t go_c[3] = { 102, 255, 102 };
			const uint8_t *col = ORANGE;
			if (n > 0)
				sprintf(txt, "%d", n);
			else
				strcpy(txt, "GO!"), col = go_c;
			float pulse = 1.0f + (1.0f - fractf(G.ride_t)) * 0.5f;
			stext(txt, w * 0.5f, h * 0.36f - 12 * pulse, 4 * pulse, col, ALIGN_CENTER);
		}
		float bt = G.banner_t;
		if (bt >= 0 && bt < 3.0f) {
			float a = smooth(bt / 0.3f) * (1.0f - smooth((bt - 2.4f) / 0.6f));
			float by = G.boss.active ? h * 0.66f : h * 0.14f;
			uint8_t c[3] = { 255, 191, 77 };
			scaled(c, a, c);
			uint8_t sh[3] = { 0, 0, 0 };
			text_fit(G.banner, w * 0.5f + 1, by + 1, 2, w * 0.92f, sh, ALIGN_CENTER);
			text_fit(G.banner, w * 0.5f, by, 2, w * 0.92f, c, ALIGN_CENTER);
		}
		Boss *b = &G.boss;
		if (b->active && !b->defeated && G.state == S_BOSS) {
			float bw = w * 0.5f, x = w * 0.25f, name_y = 4, y = name_y + 12;
			stext("THE PUMPKIN KING", w * 0.5f, name_y, 1, ORANGE, ALIGN_CENTER);
			static const uint8_t bar_bg[3] = { 26, 8, 31 };
			panel(x, y, bw, 7, bar_bg, 0.85f, BLACK);
			float f = maxf(b->hp / b->max_hp, 0);
			uint8_t bar[3] = { 255, (uint8_t) ((0.45f + 0.3f * f) * 255), 13 };
			hud_rect((int) (x + 1.5f), (int) (y + 1.5f), (int) ((bw - 3) * f), 4, bar, 255);
			if (b->taunt > 3.5f && b->t > 3.0f) {
				float sx, sy, z;
				if (project(f3_add(b->pos, f3(0, 5.5f, 0)), &sx, &sy, &z)) {
					static const uint8_t tc[3] = { 255, 230, 102 };
					stext_fit(b->taunt_text, clampf(sx, w * 0.3f, w * 0.7f), maxf(sy, y + 13), 1.3f, w * 0.6f, tc, ALIGN_CENTER);
				}
			}
		}
		draw_scores();
	}
	if (G.state == S_OUTRO) {
		float a = smooth((G.state_t - (OUTRO_TIME - 1.6f)) / 1.4f);
		hud_darken((int) (a * 255));
	}
	for (int i = 0; i < NP; i++)
		draw_crosshair(i);
	if (G.paused) {
		hud_darken(140);
		static const uint8_t grey[3] = { 204, 204, 204 };
		stext("PAUSED", w * 0.5f, h * 0.36f, 3, ORANGE, ALIGN_CENTER);
		stext_fit("PRESS START (GUN: A) TO KEEP RIDING", w * 0.5f, h * 0.36f + 36, 1, w * 0.94f, WHITE, ALIGN_CENTER);
		stext_fit("SELECT (GUN: B) GOES BACK TO THE START", w * 0.5f, h * 0.36f + 48, 1, w * 0.94f, grey, ALIGN_CENTER);
	}
}

// ------------------------------------------------------------------ frame

void game_init(void) {
	const SpawnHeader *sh = (const SpawnHeader *) spawns_bin;
	n_one = sh->n_one, n_two = sh->n_two, n_summon = sh->n_summon;
	spawn_one = (const SpawnRec *) (sh + 1);
	spawn_two = spawn_one + n_one;
	spawn_summon = spawn_two + n_two;
	const SceneryHeader *sc = (const SceneryHeader *) scenery_bin;
	n_fires = sc->n_fire, n_cauldrons = sc->n_cauldron, n_clouds = sc->n_cloud, n_mist = sc->n_mist;
	fires = (const SVECTOR *) (sc + 1);
	cauldrons = fires + n_fires;
	clouds = (const Cloud *) (cauldrons + n_cauldrons);
	mists = (const Mist *) (clouds + n_clouds);
	memset(&G, 0, sizeof(G));
	rng_seed(&G.rng, 7);
	G.state = S_TITLE;
	G.title_join_t = -1;
	float fc[3], fs, fe, lamp;
	zone_fog(Z_GRAVEYARD, fc, &fs, &fe, &lamp);
	for (int i = 0; i < 3; i++)
		G.fog[i] = fc[i];
	G.fog_start = fs, G.fog_end = fe, G.lamp = 0.35f;
	update_camera(0);
}

void game_frame(int ms) {
	float dt = ms * 0.001f;
	if (dt > 0.25f)
		dt = 0.25f;
	update(dt);
	audio_music(music_for_state());
	audio_cart(cart_noise());

	stream_update(G.cam.seg);
	gfx_begin();
	V3 eye = fine(G.eye);
	int16_t dir[3] = { (int16_t) fix(G.dir.x), (int16_t) fix(G.dir.y), (int16_t) fix(G.dir.z) };
	int16_t up[3] = { (int16_t) fix(G.up.x), (int16_t) fix(G.up.y), (int16_t) fix(G.up.z) };
	gfx_camera(&eye, dir, up);
	uint8_t fog[3];
	for (int i = 0; i < 3; i++)
		fog[i] = (uint8_t) clampf(G.fog[i] * 255.0f, 0, 255);
	gfx_fog(fog, (int) (G.fog_start * 256), (int) (G.fog_end * 256), (int) (G.lamp * 256));

	// (The score screens cover everything, so the world isn't drawn behind them.)
	int covered = G.state == S_RESULTS || G.state == S_INITIALS || G.state == S_HIGHSCORES;
	int t0 = TIMER_VALUE(1);
	const uint16_t *list;
	int n = covered ? 0 : stream_list(G.cam.seg, &list);
	for (int i = 0; i < n; i++) {
		const MeshletHeader *m = stream_get(list[i]);
		if (m)
			gfx_draw_meshlet(m);
	}
	int t1 = TIMER_VALUE(1);
	if (G.cam.zone == Z_GRAVEYARD && !covered)
		gfx_draw_sky(1);
	if (!covered)
		draw_scenery();
	int t2 = TIMER_VALUE(1);
	if (!covered) {
		draw_doors();
		draw_targets();
		draw_boss();
		draw_particles();
	}
	int t3 = TIMER_VALUE(1);
	draw_hud();
	int t4 = TIMER_VALUE(1);
#ifdef HH_DEBUG
	{
		static int acc_ms, acc_f, fps10;
		acc_ms += ms, acc_f++;
		if (acc_ms >= 1000) {
			fps10 = acc_f * 10000 / acc_ms;
			acc_ms = acc_f = 0;
		}
		// Frames per second, then hblanks spent on the world, scenery, models and HUD.
		char buf[64];
		sprintf(buf, "%d.%d W%d S%d M%d H%d P%d", fps10 / 10, fps10 % 10, (t1 - t0) & 0xffff, (t2 - t1) & 0xffff,
			(t3 - t2) & 0xffff, (t4 - t3) & 0xffff, gfx_stat_polys);
		static const uint8_t dbg[3] = { 160, 255, 160 };
		hud_text(buf, 2, 2, 4096, dbg, ALIGN_LEFT);
		sprintf(buf, "ML%d V%d Q%d SK%d L%d R%d", gfx_stat_ml, gfx_stat_v, gfx_stat_q, gfx_stat_skip, n, stream_resident());
		hud_text(buf, 2, 12, 4096, dbg, ALIGN_LEFT);
	}
#else
	(void) t0, (void) t1, (void) t2, (void) t3, (void) t4;
#endif
	if (input_wants_flash())
		hud_flash();
}
