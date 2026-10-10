/* redirect.c - Output redirection to Z-machine memory
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

#define MAX_NESTING 16

extern zword get_max_width(zword);

static int depth = -1;

static struct {
	zword xsize;
	zword table;
	zword width;
	zword total;
} redirect[MAX_NESTING];


/*
 * memory_open
 *
 * Begin output redirection to the memory of the Z-machine.
 *
 */
void memory_open(zword table, zword xsize, bool buffering)
{
#ifdef TOPS20
	short sxs;

	if (++depth < MAX_NESTING) {
		if (!buffering)
			xsize = 0xffff;
		sxs = s16(xsize);
		if (buffering && sxs <= 0)
			xsize = get_max_width (((zword) (-sxs)) & 0xffff);

		storew(table, 0);
		redirect[depth].table = table;
		redirect[depth].width = 0;
		redirect[depth].total = 0;
		redirect[depth].xsize = xsize;
		ostream_memory = TRUE;
#else
	if (++depth < MAX_NESTING) {
		if (!buffering)
			xsize = 0xffff;
		else {
			if ((short)xsize >= 0)
				xsize = get_max_width(xsize);
			else
				xsize = -xsize;
		}
		storew(table, 0);
		redirect[depth].table = table;
		redirect[depth].width = 0;
		redirect[depth].total = 0;
		redirect[depth].xsize = xsize;
		ostream_memory = TRUE;
#endif
	} else
		runtime_error(ERR_STR3_NESTING);
} /* memory_open */


/*
 * memory_new_line
 *
 * Redirect a newline to the memory of the Z-machine.
 *
 */
void memory_new_line(void)
{
	zword size;
	zword addr;

	redirect[depth].total += redirect[depth].width;
	redirect[depth].width = 0;

	addr = redirect[depth].table;

	LOW_WORD(addr, size)
	addr += 2;

	if (redirect[depth].xsize != 0xffff) {
		redirect[depth].table = addr + size;
		size = 0;

	} else
		storeb((zword) (addr + (size++)), 13);

	storew(redirect[depth].table, size);
} /* memory_new_line */


/*
 * memory_word
 *
 * Redirect a string of characters to the memory of the Z-machine.
 *
 */
void memory_word(const zchar * s)
{
	zword size;
	zword addr;
	zchar c;

	if (z_header.version == V6) {
		int width = os_string_width(s);

		if (redirect[depth].xsize != 0xffff)
			if (redirect[depth].width + width >
			    redirect[depth].xsize) {
				if (*s == ' ' || *s == ZC_INDENT ||
				    *s == ZC_GAP)
					width = os_string_width(++s);
				memory_new_line();
			}
		redirect[depth].width += width;
	}
	addr = redirect[depth].table;
	LOW_WORD(addr, size)
	addr += 2;

	while ((c = *s++) != 0)
		storeb((zword) (addr + (size++)), translate_to_zscii(c));

	storew(redirect[depth].table, size);
} /* memory_word */


/*
 * memory_close
 *
 * End of output redirection.
 *
 */
void memory_close(void)
{
	if (depth >= 0) {
		if (redirect[depth].xsize != 0xffff)
			memory_new_line();
		if (z_header.version == V6) {

			z_header.line_width = (redirect[depth].xsize != 0xffff) ?
			    redirect[depth].total : redirect[depth].width;

			SET_WORD(H_LINE_WIDTH, z_header.line_width)
		}
		if (depth == 0)
			ostream_memory = FALSE;
		depth--;
	}
} /* memory_close */
