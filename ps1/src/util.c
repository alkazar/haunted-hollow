// Maths helpers (see util.h).

#include "util.h"

int isqrt(int v) {
	if (v <= 0)
		return 0;
	unsigned int r = 0, bit = 1u << 30;
	unsigned int n = v;
	while (bit > n)
		bit >>= 2;
	while (bit) {
		if (n >= r + bit) {
			n -= r + bit;
			r = (r >> 1) + bit;
		} else {
			r >>= 1;
		}
		bit >>= 2;
	}
	return r;
}

void normalize3(int v[3]) {
	// Bring the components into a range where the squares can't overflow.
	int m = v[0] < 0 ? -v[0] : v[0];
	int a = v[1] < 0 ? -v[1] : v[1], b = v[2] < 0 ? -v[2] : v[2];
	if (a > m) m = a;
	if (b > m) m = b;
	if (m == 0) {
		v[2] = 4096;
		return;
	}
	while (m > 16384) {
		v[0] >>= 1, v[1] >>= 1, v[2] >>= 1;
		m >>= 1;
	}
	while (m < 2048) {
		v[0] <<= 1, v[1] <<= 1, v[2] <<= 1;
		m <<= 1;
	}
	int len = isqrt(v[0] * v[0] + v[1] * v[1] + v[2] * v[2]);
	if (len == 0)
		return;
	for (int i = 0; i < 3; i++)
		v[i] = (v[i] << 12) / len;
}

#define SIN_N 1024
static float sin_table[SIN_N + 1];

void util_init(void) {
	// sin by rotating a unit vector in small steps, renormalising as we go.
	float c = 1.0f, s = 0.0f;
	const float dc = 0.99998117528f, ds = 0.00613588465f;  // cos/sin(2pi/1024)
	for (int i = 0; i <= SIN_N; i++) {
		sin_table[i] = s;
		float nc = c * dc - s * ds;
		float ns = s * dc + c * ds;
		float k = 1.5f - 0.5f * (nc * nc + ns * ns);   // renormalise
		c = nc * k, s = ns * k;
	}
}

float fsin(float x) {
	float t = x * (SIN_N / TAU);
	int i = (int) t;
	if (t < 0 && (float) i != t)
		i--;
	float f = t - (float) i;
	i &= SIN_N - 1;
	return sin_table[i] + (sin_table[i + 1] - sin_table[i]) * f;
}

float fcos(float x) {
	return fsin(x + PI * 0.5f);
}

float fsqrt(float x) {
	if (x <= 0)
		return 0;
	union { float f; uint32_t i; } u = { x };
	u.i = 0x1fbd1df5 + (u.i >> 1);
	float y = u.f;
	y = 0.5f * (y + x / y);
	y = 0.5f * (y + x / y);
	return y;
}

float fatan2(float y, float x) {
	float ax = absf(x), ay = absf(y);
	if (ax < 1e-9f && ay < 1e-9f)
		return 0;
	float a = ax > ay ? ay / ax : ax / ay;
	float s = a * a;
	float r = ((-0.0464964749f * s + 0.15931422f) * s - 0.327622764f) * s * a + a;
	if (ay > ax)
		r = PI * 0.5f - r;
	if (x < 0)
		r = PI - r;
	if (y < 0)
		r = -r;
	return r;
}

float fexp(float x) {
	// For the small negative arguments used in smoothing: e^x = (1 + x/16)^16.
	if (x < -16)
		return 0;
	float y = 1.0f + x * (1.0f / 16.0f);
	if (y < 0)
		return 0;
	y *= y; y *= y; y *= y; y *= y;
	return y;
}

void rng_seed(Rng *r, uint32_t seed) {
	r->s = seed * 2654435761u | 1;
}

uint32_t rng_u32(Rng *r) {
	uint32_t x = r->s;
	x ^= x << 13;
	x ^= x >> 17;
	x ^= x << 5;
	r->s = x;
	return x;
}

float rng_f(Rng *r) {
	return (float) (rng_u32(r) >> 8) * (1.0f / 16777216.0f);
}

float rng_range(Rng *r, float a, float b) {
	return a + (b - a) * rng_f(r);
}

int rng_int(Rng *r, int a, int b) {
	int n = b - a + 1;
	return n <= 1 ? a : a + (int) (rng_u32(r) % (uint32_t) n);
}
