// The world, on the Wii: it all fits in memory, so "streaming" just hands out
// meshlets from the embedded WORLD.BIN.

#include <stdint.h>
#include "stream.h"

static const PvsHeader *pvs;
static const uint32_t *seg_start;
static const uint16_t *seg_list;
static const uint8_t *world;

void stream_init(int unused) {
	(void) unused;
	extern const uint8_t world_bin[];
	world = world_bin;
	pvs = (const PvsHeader *) pvs_bin;
	const uint8_t *p = (const uint8_t *) (pvs + 1);
	p += pvs->n_palette * 4;
	p += pvs->n_meshlets * 4;   // (spans: only the PS1 needs them)
	seg_start = (const uint32_t *) p;
	p += (pvs->n_segs + 1) * 4;
	seg_list = (const uint16_t *) p;
}

const uint8_t *stream_palette(void) {
	return (const uint8_t *) (pvs + 1);
}

int stream_segments(void) {
	return pvs->n_segs;
}

void stream_reset(void) {
}

void stream_update(int seg) {
	(void) seg;
}

int stream_list(int seg, const uint16_t **list) {
	if (seg < 0) seg = 0;
	if (seg >= pvs->n_segs) seg = pvs->n_segs - 1;
	*list = seg_list + seg_start[seg];
	return seg_start[seg + 1] - seg_start[seg];
}

const MeshletHeader *stream_get(int meshlet) {
	return (const MeshletHeader *) (world + meshlet * SECTOR);
}

int stream_ready(int seg) {
	(void) seg;
	return 1;
}

int stream_resident(void) {
	return pvs->n_meshlets;
}

void *stream_scratch(void) {
	return 0;
}

int stream_scratch_size(void) {
	return 0;
}
