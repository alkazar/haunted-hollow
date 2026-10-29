// Streams the world from WORLD.BIN as the cart moves. Meshlets are on the disc in
// the order the ride first needs them, so reading is (almost) always straight
// ahead: whenever there's room, read the next few that the coming stretch of
// track needs into a staging buffer, then hand each one a free slot. A meshlet's
// slot is freed once the cart is past the last stretch that sees it.

#include <stdint.h>
#include <string.h>
#include <psxcd.h>
#include "stream.h"

// Slots for resident meshlets (the exporter reports the most needed at once).
#define MAX_SLOTS   304
#define MAX_MESHLETS 2048
#define STAGE       8

static uint8_t slots[MAX_SLOTS][SECTOR] __attribute__((aligned(4)));
static uint8_t stage[STAGE][SECTOR] __attribute__((aligned(4)));
static int16_t slot_of[MAX_MESHLETS];
static int16_t owner[MAX_SLOTS];
static int16_t free_list[MAX_SLOTS];
static int n_free;

static const PvsHeader *pvs;
static const uint16_t *span;
static const uint32_t *seg_start;
static const uint16_t *seg_list;
static int lba;
static int next_load;
static int reading, read_first, read_count;
static int cur_seg;

void *stream_scratch(void) {
	return slots;
}

int stream_scratch_size(void) {
	return sizeof(slots);
}

void stream_init(int world_lba) {
	pvs = (const PvsHeader *) pvs_bin;
	const uint8_t *p = (const uint8_t *) (pvs + 1);
	p += pvs->n_palette * 4;
	span = (const uint16_t *) p;
	p += pvs->n_meshlets * 4;
	seg_start = (const uint32_t *) p;
	p += (pvs->n_segs + 1) * 4;
	seg_list = (const uint16_t *) p;
	lba = world_lba;
	stream_reset();
}

const uint8_t *stream_palette(void) {
	return (const uint8_t *) (pvs + 1);
}

int stream_segments(void) {
	return pvs->n_segs;
}

void stream_reset(void) {
	if (reading) {
		CdReadSync(0, 0);
		reading = 0;
	}
	for (int i = 0; i < MAX_MESHLETS; i++)
		slot_of[i] = -1;
	n_free = 0;
	for (int i = MAX_SLOTS - 1; i >= 0; i--) {
		owner[i] = -1;
		free_list[n_free++] = i;
	}
	next_load = 0;
	cur_seg = 0;
}

static void finish_read(void) {
	for (int k = 0; k < read_count; k++) {
		int m = read_first + k;
		if (span[m * 2 + 1] + 1 < cur_seg || n_free == 0)
			continue;   // already passed (or no room, which the planner rules out)
		int s = free_list[--n_free];
		memcpy(slots[s], stage[k], SECTOR);
		owner[s] = m;
		slot_of[m] = s;
	}
	reading = 0;
}

void stream_update(int seg) {
	cur_seg = seg;
	// Free what's behind us (keeping one segment of slack).
	for (int s = 0; s < MAX_SLOTS; s++) {
		int m = owner[s];
		if (m >= 0 && span[m * 2 + 1] + 1 < seg) {
			owner[s] = -1;
			slot_of[m] = -1;
			free_list[n_free++] = s;
		}
	}
	if (reading) {
		int r = CdReadSync(1, 0);
		if (r > 0)
			return;
		if (r < 0) {
			// Read error: try the same run again.
			next_load = read_first;
			reading = 0;
		} else {
			finish_read();
		}
	}
	int n = pvs->n_meshlets;
	// Skip anything the cart is already past (after a jump, say).
	while (next_load < n && span[next_load * 2 + 1] + 1 < seg)
		next_load++;
	int count = 0;
	while (count < STAGE && next_load + count < n && count < n_free &&
			span[(next_load + count) * 2] <= seg + pvs->lookahead)
		count++;
	if (!count)
		return;
	CdlLOC pos;
	CdIntToPos(lba + next_load, &pos);
	CdControl(CdlSetloc, &pos, 0);
	CdReadCallback(0);
	CdRead(count, (uint32_t *) stage, CdlModeSpeed);
	reading = 1;
	read_first = next_load;
	read_count = count;
	next_load += count;
}

int stream_list(int seg, const uint16_t **list) {
	if (seg < 0) seg = 0;
	if (seg >= pvs->n_segs) seg = pvs->n_segs - 1;
	*list = seg_list + seg_start[seg];
	return seg_start[seg + 1] - seg_start[seg];
}

const MeshletHeader *stream_get(int meshlet) {
	int s = slot_of[meshlet];
	return s < 0 ? 0 : (const MeshletHeader *) slots[s];
}

int stream_ready(int seg) {
	const uint16_t *list;
	int n = stream_list(seg, &list);
	for (int i = 0; i < n; i++)
		if (slot_of[list[i]] < 0)
			return 0;
	return 1;
}

int stream_resident(void) {
	return MAX_SLOTS - n_free;
}
