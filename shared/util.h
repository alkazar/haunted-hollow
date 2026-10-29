// Small maths helpers. The PS1 has no FPU, so floats (software) are kept to game
// logic that runs a handful of times per frame; fast table-based sin/cos below.

#pragma once

#include <stdint.h>

#define PI    3.14159265f
#define TAU   6.28318531f

int  isqrt(int v);
// Scales a vector to length 4096.
void normalize3(int v[3]);

void  util_init(void);
float fsin(float x);
float fcos(float x);
float fsqrt(float x);
float fatan2(float y, float x);
float fexp(float x);

static inline float clampf(float x, float a, float b) {
	return x < a ? a : x > b ? b : x;
}
static inline float minf(float a, float b) {
	return a < b ? a : b;
}
static inline float maxf(float a, float b) {
	return a > b ? a : b;
}
static inline float absf(float a) {
	return a < 0 ? -a : a;
}
static inline float smooth(float t) {
	t = clampf(t, 0.0f, 1.0f);
	return t * t * (3.0f - 2.0f * t);
}
static inline float fractf(float x) {
	return x - (float) (int) x + (x < 0 && x != (float) (int) x ? 1.0f : 0.0f);
}
static inline int fix(float x) {
	return (int) (x * 4096.0f);
}

typedef struct { float x, y, z; } F3;

static inline F3 f3(float x, float y, float z) {
	F3 v = { x, y, z };
	return v;
}
static inline F3 f3_add(F3 a, F3 b) {
	return f3(a.x + b.x, a.y + b.y, a.z + b.z);
}
static inline F3 f3_sub(F3 a, F3 b) {
	return f3(a.x - b.x, a.y - b.y, a.z - b.z);
}
static inline F3 f3_mul(F3 a, float k) {
	return f3(a.x * k, a.y * k, a.z * k);
}
static inline F3 f3_lerp(F3 a, F3 b, float t) {
	return f3(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t, a.z + (b.z - a.z) * t);
}
static inline float f3_dot(F3 a, F3 b) {
	return a.x * b.x + a.y * b.y + a.z * b.z;
}
static inline F3 f3_cross(F3 a, F3 b) {
	return f3(a.y * b.z - a.z * b.y, a.z * b.x - a.x * b.z, a.x * b.y - a.y * b.x);
}
static inline float f3_len(F3 a) {
	return fsqrt(f3_dot(a, a));
}
static inline F3 f3_norm(F3 a) {
	float l = f3_len(a);
	return l > 1e-6f ? f3_mul(a, 1.0f / l) : f3(0, 0, 0);
}

// A small xorshift generator (for sparks and such; the ride's targets are recorded).
typedef struct { uint32_t s; } Rng;
void     rng_seed(Rng *r, uint32_t seed);
uint32_t rng_u32(Rng *r);
float    rng_f(Rng *r);
float    rng_range(Rng *r, float a, float b);
int      rng_int(Rng *r, int a, int b);
