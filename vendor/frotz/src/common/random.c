/* random.c - Z-machine random number generator
 *	Copyright (c) 1995-1997 Stefan Jokisch
 *
 * This file is part of Frotz.
 *
 * Frotz is free software; you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation; either version 2 of the License, or
 * (at your option) any later version.
 *
 * Frotz is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program; if not, write to the Free Software
 * Foundation, Inc., 51 Franklin Street, Fifth Floor, Boston, MA 02110-1301 USA
 */

#include "frotz.h"

static long A = 1;

static int interval = 0;
static int counter = 0;

#ifdef LATE_ZORK
/* Modified 2026-10-09: preserve RNG continuation in private door saves. */
void door_random_write(FILE *f)
{
	uint64_t a = (uint64_t)A;
	door_put32(f, a >> 32);
	door_put32(f, a);
	door_put32(f, interval);
	door_put32(f, counter);
}
void door_random_read(FILE *f)
{
	uint64_t a = (uint64_t)door_get32(f) << 32;
	a |= door_get32(f);
	A = (long)a;
	interval = door_get32(f);
	counter = door_get32(f);
}
#endif


/*
 * seed_random
 *
 * Set the seed value for the random number generator.
 *
 */
void seed_random(int value)
{
	if (value == 0) {		/* ask interface for seed value */
		A = os_random_seed();
		interval = 0;
	} else if (value < 1000) {	/* special seed value */
		counter = 0;
		interval = value;
	} else {			/* standard seed value */
		A = value;
		interval = 0;
	}
} /* seed_random */


/*
 * z_random, store a random number or set the random number seed.
 *
 *	zargs[0] = range (positive) or seed value (negative)
 *
 */
void z_random()
{
#ifdef TOPS20
	short sz;
	sz = s16(zargs[0]);
	if (sz <= 0) {		/* set random seed */
		seed_random(-sz);
		store(0);
#else
	if ((short)zargs[0] <= 0) {	/* set random seed */
		seed_random(-(short)zargs[0]);
		store(0);
#endif
	} else {		/* generate random number */
		zword result;

		if (interval != 0) {	/* ...in special mode */
			result = counter++;
			if (counter == interval)
				counter = 0;
		} else {	/* ...in standard mode */
			A = 0x015a4e35L * A + 1;
			result = (A >> 16) & 0x7fff;
		}

#ifdef TOPS20
		store((zword) ((result % zargs[0] + 1) & 0xffff));
#else
		store((zword) (result % zargs[0] + 1));
#endif
	}
} /* z_random */
