// World streaming from the CD (see stream.c).

#pragma once

#include <stdint.h>
#include "data.h"

void stream_init(int world_lba);
// Forgets everything and starts reading from the beginning of the ride again.
void stream_reset(void);
// Call every frame with the camera's segment of track.
void stream_update(int seg);
// Meshlets to draw from a segment.
int  stream_list(int seg, const uint16_t **list);
// A resident meshlet, or NULL if it isn't loaded.
const MeshletHeader *stream_get(int meshlet);
// True once everything seen from `seg` is loaded.
int  stream_ready(int seg);
int  stream_resident(void);
int  stream_segments(void);
const uint8_t *stream_palette(void);
// The slot memory, free to use as scratch before streaming starts.
void *stream_scratch(void);
int  stream_scratch_size(void);
