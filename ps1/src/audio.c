// Sound: the songs are note lists (exported from the PC synth) played on sampled
// instruments, so they take a few KB instead of megabytes of audio. Voices 0-15
// play music, 16-22 effects, 23 the cart's rattle.

#include <stdint.h>
#include <psxspu.h>
#include <hwregs_c.h>
#include "audio.h"
#include "data.h"

#define SPU_BASE     0x1010
#define MUSIC_VOICES 16
#define SFX_FIRST    16
#define SFX_VOICES   7
#define CART_VOICE   23
#define N_INSTS      8

static const SoundHeader *hdr;
static const SampleInfo *samples;
static const SongInfo *songs;
static const SongEvent *events;

// Release (in the SPU's shift units) for each instrument when its note ends,
// matching the PC synth's release times; 0 = cut at once (xylophone, bell).
static const uint8_t release[N_INSTS] = { 11, 12, 0, 10, 0, 11, 10, 13 };

static int song = MUSIC_NONE;
static int song_t, song_ev;
static int now;

typedef struct {
	int off_at;    // when to release (ms), or 0
	int free_at;   // when it's probably silent again
	int started;
} Voice;
static Voice voices[24];
static int next_sfx;

static void upload(const uint8_t *data, int size) {
	SpuSetTransferStartAddr(SPU_BASE);
	while (size > 0) {
		int n = size > 32768 ? 32768 : size;
		SpuWrite((const uint32_t *) data, n);
		SpuIsTransferCompleted(SPU_TRANSFER_WAIT);
		data += n;
		size -= n;
		SpuSetTransferStartAddr(SpuGetTransferStartAddr() + n);
	}
}

// (Key-on restarts a voice that's still playing, so there's no key-off first.)
static void key_on(int v, int sample, int pitch, int vl, int vr, int rel) {
	SPU_CH_ADDR(v) = getSPUAddr(SPU_BASE + samples[sample].off);
	SPU_CH_FREQ(v) = pitch > 0x3fff ? 0x3fff : pitch;
	SPU_CH_VOL_L(v) = vl > 0x3fff ? 0x3fff : vl;
	SPU_CH_VOL_R(v) = vr > 0x3fff ? 0x3fff : vr;
	// Instant attack, hold at full level, linear release.
	SPU_CH_ADSR1(v) = 0x000f;
	SPU_CH_ADSR2(v) = 0x1fc0 | rel;
	SpuSetKey(1, 1 << v);
}

void audio_init(const void *blob, int size) {
	hdr = (const SoundHeader *) sound_bin;
	samples = (const SampleInfo *) (hdr + 1);
	songs = (const SongInfo *) (samples + hdr->n_samples);
	events = (const SongEvent *) (songs + 5);
	SpuInit();
	SPU_CTRL |= 0xc000;   // on, not muted
	SPU_MASTER_VOL_L = 0x3fff;
	SPU_MASTER_VOL_R = 0x3fff;
	upload(blob, size);
	// The cart's rattle loops forever; its volume follows the cart's speed.
	int cart = hdr->n_samples - 1;
	key_on(CART_VOICE, cart, samples[cart].pitch, 0, 0, 0);
}

static int pick_music_voice(void) {
	int best = 0, best_t = 0x7fffffff;
	for (int v = 0; v < MUSIC_VOICES; v++) {
		if (voices[v].free_at <= now)
			return v;
		if (voices[v].started < best_t)
			best_t = voices[v].started, best = v;
	}
	return best;
}

void audio_update(int ms) {
	now += ms;
	for (int v = 0; v < MUSIC_VOICES; v++)
		if (voices[v].off_at && voices[v].off_at <= now) {
			SpuSetKey(0, 1 << v);
			voices[v].off_at = 0;
		}
	if (song < 0)
		return;
	const SongInfo *s = &songs[song];
	const SongEvent *ev = (const SongEvent *) ((const uint8_t *) events + s->off);
	song_t += ms;
	for (;;) {
		if (song_ev >= (int) s->n) {
			if (!s->looping || song_t < (int) s->len_ms)
				return;
			song_t -= s->len_ms;
			song_ev = 0;
		}
		const SongEvent *e = &ev[song_ev];
		if ((int) e->t_ms > song_t)
			return;
		song_ev++;
		int v = pick_music_voice();
		int inst = e->sample;
		int rel = inst < N_INSTS ? release[inst] : 0;
		key_on(v, inst, e->pitch, e->vol_l, e->vol_r, rel);
		voices[v].started = now;
		if (inst < N_INSTS) {
			voices[v].off_at = now + e->dur_ms;
			voices[v].free_at = now + e->dur_ms + (rel ? (1 << (rel > 11 ? rel - 11 : 0)) * 93 : 10);
		} else {
			voices[v].off_at = 0;
			voices[v].free_at = now + 300;
		}
	}
}

void audio_music(int which) {
	if (which == song)
		return;
	for (int v = 0; v < MUSIC_VOICES; v++) {
		SpuSetKey(0, 1 << v);
		voices[v].off_at = 0;
		voices[v].free_at = now + 300;
	}
	song = which;
	song_t = 0;
	song_ev = 0;
}

void audio_sfx(int sfx, float vol, float pan, float pitch) {
	if (sfx < 0 || sfx >= SFX_COUNT || !hdr)
		return;
	int si = hdr->first_sfx + sfx;
	const SampleInfo *s = &samples[si];
	// Prefer a voice that's done; otherwise take turns.
	int v = -1;
	for (int k = 0; k < SFX_VOICES; k++) {
		int c = SFX_FIRST + (next_sfx + k) % SFX_VOICES;
		if (voices[c].free_at <= now) {
			v = c;
			break;
		}
	}
	if (v < 0)
		v = SFX_FIRST + next_sfx % SFX_VOICES;
	next_sfx = (v - SFX_FIRST + 1) % SFX_VOICES;
	if (pan < -1) pan = -1;
	if (pan > 1) pan = 1;
	float g = vol * (float) s->gain * (1.0f / 65536.0f) * 16383.0f;
	int vl = (int) (g * (1.0f - pan * 0.7f)), vr = (int) (g * (1.0f + pan * 0.7f));
	key_on(v, si, (int) (s->pitch * pitch), vl, vr, 0);
	voices[v].started = now;
	// Samples are 22050 Hz ADPCM, 28 samples per 16 bytes.
	int len_ms = (int) ((s->len / 16) * 28 * 1000 / 22050);
	voices[v].free_at = now + (int) (len_ms / (pitch > 0.1f ? pitch : 0.1f)) + 20;
}

void audio_cart(float vol) {
	if (!hdr)
		return;
	const SampleInfo *s = &samples[hdr->n_samples - 1];
	int v = (int) (vol * 0.95f * (float) s->gain * (1.0f / 65536.0f) * 16383.0f);
	if (v < 0) v = 0;
	if (v > 0x3fff) v = 0x3fff;
	SPU_CH_VOL_L(CART_VOICE) = v;
	SPU_CH_VOL_R(CART_VOICE) = v;
}
