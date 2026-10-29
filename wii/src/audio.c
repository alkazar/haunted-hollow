// Sound with the Wii's ASND library: the PC game's songs and effects as they are
// (PCM). Voices 0-1 play music (crossfading), 2-14 effects, 15 the cart's rattle.

#include <stdint.h>
#include <string.h>
#include <malloc.h>
#include <gccore.h>
#include <asndlib.h>
#include "audio.h"

#define N_SONGS    5
#define MUSIC_VOL  0.42f   // as on the PC
#define FADE_MS    800
#define SFX_FIRST  2
#define SFX_VOICES 13
#define CART_VOICE 15

typedef struct {
	uint32_t off, bytes, rate, looping;
} Clip;

static uint8_t *blob;
static const Clip *clips;   // effects, songs, cart
static int music[2] = { -1, -1 };   // song on voice 0 / 1
static int music_gain[2];           // 0..256
static int music_target[2];
static int next_sfx;

void audio_init(const void *data, int size) {
	// ASND wants 32-byte aligned sound in memory it can reach.
	blob = memalign(32, size);
	memcpy(blob, data, size);
	DCFlushRange(blob, size);
	clips = (const Clip *) (blob + 8);
	ASND_Init();
	ASND_Pause(0);
	const Clip *c = &clips[SFX_COUNT + N_SONGS];
	ASND_SetInfiniteVoice(CART_VOICE, VOICE_MONO_16BIT, c->rate, 0, blob + c->off, c->bytes, 0, 0);
}

static void set_music_volume(int v) {
	int vol = (int) (255 * MUSIC_VOL) * music_gain[v] >> 8;
	ASND_ChangeVolumeVoice(v, vol, vol);
}

void audio_update(int ms) {
	for (int v = 0; v < 2; v++) {
		if (music[v] < 0)
			continue;
		int step = 256 * ms / FADE_MS;
		if (music_gain[v] < music_target[v])
			music_gain[v] = music_gain[v] + step > music_target[v] ? music_target[v] : music_gain[v] + step;
		else if (music_gain[v] > music_target[v])
			music_gain[v] = music_gain[v] - step < music_target[v] ? music_target[v] : music_gain[v] - step;
		if (music_gain[v] <= 0 && music_target[v] == 0) {
			ASND_StopVoice(v);
			music[v] = -1;
			continue;
		}
		set_music_volume(v);
	}
}

void audio_music(int song) {
	int cur = music[0] >= 0 && music_target[0] ? 0 : music[1] >= 0 && music_target[1] ? 1 : -1;
	if (cur >= 0 && music[cur] == song)
		return;
	if (cur < 0 && song < 0)
		return;
	if (cur >= 0)
		music_target[cur] = 0;
	if (song < 0)
		return;
	int v = cur == 0 ? 1 : 0;
	const Clip *c = &clips[SFX_COUNT + song];
	ASND_StopVoice(v);
	if (c->looping)
		ASND_SetInfiniteVoice(v, VOICE_MONO_16BIT, c->rate, 0, blob + c->off, c->bytes, 0, 0);
	else
		ASND_SetVoice(v, VOICE_MONO_16BIT, c->rate, 0, blob + c->off, c->bytes, 0, 0, NULL);
	music[v] = song;
	music_gain[v] = 0;
	music_target[v] = 256;
}

void audio_sfx(int sfx, float vol, float pan, float pitch) {
	if (sfx < 0 || sfx >= SFX_COUNT || !blob)
		return;
	const Clip *c = &clips[sfx];
	// Prefer a free voice; otherwise take turns.
	int v = -1;
	for (int k = 0; k < SFX_VOICES; k++) {
		int cand = SFX_FIRST + (next_sfx + k) % SFX_VOICES;
		if (ASND_StatusVoice(cand) == SND_UNUSED) {
			v = cand;
			break;
		}
	}
	if (v < 0)
		v = SFX_FIRST + next_sfx % SFX_VOICES;
	next_sfx = (v - SFX_FIRST + 1) % SFX_VOICES;
	if (pan < -1) pan = -1;
	if (pan > 1) pan = 1;
	int l = (int) (255 * vol * (1.0f - pan * 0.7f)), r = (int) (255 * vol * (1.0f + pan * 0.7f));
	ASND_StopVoice(v);
	ASND_SetVoice(v, VOICE_MONO_16BIT, (int) (c->rate * pitch), 0, blob + c->off, c->bytes,
		l > 255 ? 255 : l, r > 255 ? 255 : r, NULL);
}

void audio_cart(float vol) {
	int v = (int) (vol * 0.95f * 255);
	if (v < 0) v = 0;
	if (v > 255) v = 255;
	ASND_ChangeVolumeVoice(CART_VOICE, v, v);
}
