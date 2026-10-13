/* math.c - Arithmetic, compare and logical opcodes
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

/*
 * z_add, 16bit addition.
 *
 *	zargs[0] = first value
 *	zargs[1] = second value
 *
 */
void z_add(void)
{
#ifdef TOPS20
	store((zword) zargs[0] + zargs[1]);
#else
	store((zword) ((short) zargs[0] + (short) zargs[1]));
#endif
} /* z_add */


/*
 * z_and, bitwise AND operation.
 *
 *	zargs[0] = first value
 *	zargs[1] = second value
 *
 */
void z_and(void)
{
#ifdef TOPS20
	store ((zword) ((zargs[0] & zargs[1]) & 0xffff));
#else
    store ((zword) (zargs[0] & zargs[1]));
#endif
} /* z_and */


/*
 * z_art_shift, arithmetic SHIFT operation.
 *
 *	zargs[0] = value
 *	zargs[1] = #positions to shift left (positive) or right
 *
 */
void z_art_shift(void)
{
#ifdef TOPS20
	short sz0, sz1;
	sz0 = s16(zargs[0]);
	sz1 = s16(zargs[1]);

	if (sz1 > 0)
		store (((zword) ( sz0 << sz1 )) & 0xffff );
	else
		store (((zword) ( sz0 >> -sz1 )) & 0xffff );
#else
	if ((short) zargs[1] > 0)
		store((zword) ((short) zargs[0] << (short) zargs[1]));
	else
		store((zword) ((short) zargs[0] >> - (short) zargs[1]));
#endif
} /* z_art_shift */


/*
 * z_div, signed 16bit division.
 *
 *	zargs[0] = first value
 *	zargs[1] = second value
 *
 */
void z_div(void)
{
#ifdef TOPS20
	short sz0, sz1;
	zword z;

	sz0 = s16(zargs[0]);
	sz1 = s16(zargs[1]);

	if (sz1 == 0)
		runtime_error (ERR_DIV_ZERO);

	z = (zword) (sz0 / sz1);
	z &= 0xffff;
	store (z);
#else
	if (zargs[1] == 0)
		runtime_error(ERR_DIV_ZERO);
	store((zword) ((short) zargs[0] / (short) zargs[1]));
#endif
} /* z_div */


/*
 * z_je, branch if the first value equals any of the following.
 *
 *	zargs[0] = first value
 *	zargs[1] = second value (optional)
 *	...
 *	zargs[3] = fourth value (optional)
 *
 */
void z_je(void)
{
	branch (
		zargc > 1 && (zargs[0] == zargs[1] || (
		zargc > 2 && (zargs[0] == zargs[2] || (
		zargc > 3 && (zargs[0] == zargs[3]))))));
} /* z_je */


/*
 * z_jg, branch if the first value is greater than the second.
 *
 *	zargs[0] = first value
 *	zargs[1] = second value
 *
 */
void z_jg(void)
{
#ifdef TOPS20
	short sz0, sz1;

	sz0 = s16(zargs[0]);
	sz1 = s16(zargs[1]);
	branch (sz0 > sz1);
#else
	branch((short) zargs[0] > (short) zargs[1]);
#endif
}/* z_jg */


/*
 * z_jl, branch if the first value is less than the second.
 *
 *	zargs[0] = first value
 *	zargs[1] = second value
 *
 */
void z_jl(void)
{
#ifdef TOPS20
	short sz0, sz1;

	sz0 = s16(zargs[0]);
	sz1 = s16(zargs[1]);

	branch (sz0 < sz1);
#else
	branch((short) zargs[0] < (short) zargs[1]);
#endif
} /* z_jl */


/*
 * z_jz, branch if value is zero.
 *
 * 	zargs[0] = value
 *
 */
void z_jz(void)
{
#ifdef TOPS20
	short sz;

	sz = s16(zargs[0]);
	branch (sz == 0);
#else
	branch((short) zargs[0] == 0);
#endif
} /* z_jz */


/*
 * z_log_shift, logical SHIFT operation.
 *
 * 	zargs[0] = value
 *	zargs[1] = #positions to shift left (positive) or right (negative)
 *
 */
void z_log_shift(void)
{
#ifdef TOPS20
	short sz1;

	sz1 = s16(zargs[1]);
	if (sz1 > 0)
		store ((zword) (zargs[0] << sz1));
	else
		store ((zword) (zargs[0] >> -sz1));
#else
	if ((short) zargs[1] > 0)
		store((zword) (zargs[0] << (short) zargs[1]));
	else
		store((zword) (zargs[0] >> - (short) zargs[1]));
#endif
} /* z_log_shift */


/*
 * z_mod, remainder after signed 16bit division.
 *
 * 	zargs[0] = first value
 *	zargs[1] = second value
 *
 */
void z_mod (void)
{
#ifdef TOPS20
	short sz0, sz1;

	sz0 = s16(zargs[0]);
	sz1 = s16(zargs[1]);
	if (sz1 == 0)
		runtime_error(ERR_DIV_ZERO);

	store ((zword) (sz0 % sz1));
#else
	if (zargs[1] == 0)
		runtime_error(ERR_DIV_ZERO);

	store((zword) ((short) zargs[0] % (short) zargs[1]));
#endif
} /* z_mod */


/*
 * z_mul, 16bit multiplication.
 *
 * 	zargs[0] = first value
 *	zargs[1] = second value
 *
 */
void z_mul(void)
{
#ifdef TOPS20
	short sz0, sz1;
	sz0 = s16(zargs[0]);
	sz1 = s16(zargs[1]);

	store ((zword) (sz0 * sz1 ));
#else
	store((zword) ((short) zargs[0] * (short) zargs[1]));
#endif
} /* z_mul */


/*
 * z_not, bitwise NOT operation.
 *
 * 	zargs[0] = value
 *
 */
void z_not (void)
{
#ifdef TOPS20
	store (((zword) ~zargs[0]) & 0xffff);
#else
	store((zword) ~zargs[0]);
#endif
} /* z_not */


/*
 * z_or, bitwise OR operation.
 *
 *	zargs[0] = first value
 *	zargs[1] = second value
 *
 */
void z_or(void)
{
#ifdef TOPS20
	store (((zword) (zargs[0] | zargs[1])) & 0xffff);
#else
	store((zword) (zargs[0] | zargs[1]));
#endif
} /* z_or */


/*
 * z_sub, 16bit subtraction.
 *
 *	zargs[0] = first value
 *	zargs[1] = second value
 *
 */
void z_sub(void)
{
#ifdef TOPS20
	short sz0, sz1;

	sz0 = s16(zargs[0]);
	sz1 = s16(zargs[1]);
	store ((zword) (sz0 - sz1));
#else
	store((zword) ((short) zargs[0] - (short) zargs[1]));
#endif
} /* z_sub */


/*
 * z_test, branch if all the flags of a bit mask are set in a value.
 *
 *	zargs[0] = value to be examined
 *	zargs[1] = bit mask
 *
 */
void z_test(void)
{
#ifdef TOPS20
	branch (((zargs[0] & zargs[1]) & 0xffff) == (zargs[1] & 0xffff));
#else
	branch((zargs[0] & zargs[1]) == zargs[1]);
#endif
} /* z_test */

/*
 * If a short is more than 16 bits, just pretend it is.  So far, this
 * makes sense only on PDP10 or "certain DSPs".
 */
#ifdef TOPS20
short s16(zword z) {
	short sz;
	sz = (short) (z & 0xffff);
	if ((sz > 0 ) && (sz > 32767))
		sz = - (65536 - sz );
	return sz;
} /* s16 */
#endif
