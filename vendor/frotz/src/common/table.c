/* table.c - Table handling opcodes
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
 * z_copy_table, copy a table or fill it with zeroes.
 *
 *	zargs[0] = address of table
 * 	zargs[1] = destination address or 0 for fill
 *	zargs[2] = size of table
 *
 * Note: Copying is safe even when source and destination overlap; but
 *       if zargs[1] is negative the table _must_ be copied forwards.
 *
 */
void z_copy_table(void)
{
#ifdef TOPS20
	short ssz;
#endif
	zword addr;
	zword size = zargs[2];
	zbyte value;
	int i;

#ifdef TOPS20
	/* TODO : this looks like it could use some masking.  AT */
	ssz=s16(size);
	if (zargs[1] == 0)	/* zero table */
		for (i = 0; i < (size & 0xffff); i++)
			storeb ((zword) ((zargs[0] + i) & 0xffff), 0);
	else if (ssz < 0 || (zargs[0] & 0xffff) > (zargs[1] & 0xffff)) {
		/*copy forwards */
		for (i = 0; i < ((ssz < 0) ? - ssz : ssz); i++) {
			addr = ((zargs[0] + i) & 0xffff);
			LOW_BYTE (addr, value)
			storeb((zword) ((zargs[1] + i) & 0xfff), value);
		}
	}
#else
	if (zargs[1] == 0)	/* zero table */
		for (i = 0; i < size; i++)
			storeb((zword) (zargs[0] + i), 0);

	else if ((short)size < 0 || zargs[0] > zargs[1]) { /* copy forwards */
		for (i = 0; i < (((short)size < 0) ? -(short)size : size); i++) {
			addr = zargs[0] + i;
			LOW_BYTE(addr, value)
			    storeb((zword) (zargs[1] + i), value);
		}
	}
#endif
	else {	/* copy backwards */
		for (i = size - 1; i >= 0; i--) {
			addr = zargs[0] + i;
			LOW_BYTE(addr, value)
			    storeb((zword) (zargs[1] + i), value);
		}
	}
} /* z_copy_table */


/*
 * z_loadb, store a value from a table of bytes.
 *
 *	zargs[0] = address of table
 *	zargs[1] = index of table entry to store
 *
 */
void z_loadb(void)
{
#ifdef TOPS20
	zword addr = zargs[0] + zargs[1] & 0xffff;
#else
	zword addr = zargs[0] + zargs[1];
#endif
	zbyte value;

	LOW_BYTE(addr, value)
	    store(value);

} /* z_loadb */


/*
 * z_loadw, store a value from a table of words.
 *
 *	zargs[0] = address of table
 *	zargs[1] = index of table entry to store
 *
 */
void z_loadw(void)
{
#ifdef TOPS20
	zword addr = (zargs[0] + 2 * zargs[1]) & 0xffff;
#else
	zword addr = zargs[0] + 2 * zargs[1];
#endif
	zword value;

	LOW_WORD(addr, value)
	    store(value);

} /* z_loadw */


/*
 * z_scan_table, find and store the address of a target within a table.
 *
 *	zargs[0] = target value to be searched for
 *	zargs[1] = address of table
 *	zargs[2] = number of table entries to check value against
 *	zargs[3] = type of table (optional, defaults to 0x82)
 *
 * Note: The table is a word array if bit 7 of zargs[3] is set; otherwise
 *       it's a byte array. The lower bits hold the address step.
 *
 */
void z_scan_table(void)
{
	zword addr = zargs[1];
	int i;

	/* Supply default arguments */
	if (zargc < 4)
		zargs[3] = 0x82;

	/* Scan byte or word array */
	for (i = 0; i < zargs[2]; i++) {
		if (zargs[3] & 0x80) {	/* scan word array */
			zword wvalue;
			LOW_WORD(addr, wvalue)
			    if (wvalue == zargs[0])
				goto finished;
		} else {	/* scan byte array */
			zbyte bvalue;
			LOW_BYTE(addr, bvalue)
			    if (bvalue == zargs[0])
				goto finished;
		}
		addr += zargs[3] & 0x7f;
	}
	addr = 0;
finished:
	store(addr);
	branch(addr);
} /* z_scan_table */


/*
 * z_storeb, write a byte into a table of bytes.
 *
 *	zargs[0] = address of table
 *	zargs[1] = index of table entry
 *	zargs[2] = value to be written
 *
 */
void z_storeb(void)
{
	storeb((zword) (zargs[0] + zargs[1]), zargs[2]);
} /* z_storeb */


/*
 * z_storew, write a word into a table of words.
 *
 *	zargs[0] = address of table
 *	zargs[1] = index of table entry
 *	zargs[2] = value to be written
 *
 */
void z_storew(void)
{
	storew((zword) (zargs[0] + 2 * zargs[1]), zargs[2]);
} /* z_storew */
