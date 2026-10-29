// Camera playback: the PC game's ride camera, sampled 20 times a second, with
// the samples in between interpolated.

#include "cam.h"

static const TrackHeader *hdr;
static const Pose *poses;

void cam_init(void) {
	hdr = (const TrackHeader *) track_bin;
	poses = (const Pose *) (hdr + 1);
}

static void from_pose(const Pose *p, Cam *c) {
	c->eye.x = p->eye[0], c->eye.y = p->eye[1], c->eye.z = p->eye[2];
	for (int i = 0; i < 3; i++)
		c->dir[i] = p->dir[i], c->up[i] = p->up[i];
	c->seg = p->seg;
	c->zone = p->zone;
	c->speed = p->speed;
}

static void sample(const Pose *list, int n, int t_ms, Cam *c) {
	int pos = t_ms * hdr->hz;          // 1/1000 sample
	int i = pos / 1000, f = ((pos % 1000) << 12) / 1000;
	if (i < 0) i = 0, f = 0;
	if (i >= n - 1) i = n - 1, f = 0;
	const Pose *a = &list[i], *b = &list[i + (f ? 1 : 0)];
	from_pose(f < 2048 ? a : b, c);
	c->eye.x = a->eye[0] + (((b->eye[0] - a->eye[0]) * f) >> 12);
	c->eye.y = a->eye[1] + (((b->eye[1] - a->eye[1]) * f) >> 12);
	c->eye.z = a->eye[2] + (((b->eye[2] - a->eye[2]) * f) >> 12);
	for (int k = 0; k < 3; k++) {
		c->dir[k] = a->dir[k] + (((b->dir[k] - a->dir[k]) * f) >> 12);
		c->up[k] = a->up[k] + (((b->up[k] - a->up[k]) * f) >> 12);
	}
}

void cam_ride(int t_ms, Cam *c) {
	sample(poses + 3, hdr->n_ride, t_ms, c);
}

void cam_outro(int t_ms, Cam *c) {
	sample(poses + 3 + hdr->n_ride, hdr->n_outro, t_ms, c);
}

void cam_pose(int which, Cam *c) {
	from_pose(&poses[which], c);
}
