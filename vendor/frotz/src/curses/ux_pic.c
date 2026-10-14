/*
 * ux_pic.c - Unix interface, picture outline functions
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
 * Or visit http://www.fsf.org/
 */

#define __UNIX_PORT_FILE

#include <stdlib.h>
#include <string.h>

#include "ux_defines.h"

#ifdef USE_NCURSES_H
#include <ncurses.h>
#else
#include <curses.h>
#endif

#include "ux_frotz.h"
#include "ux_blorb.h"

#define PIC_FILE_HEADER_FLAGS 1
#define PIC_FILE_HEADER_NUM_IMAGES 4
#define PIC_FILE_HEADER_ENTRY_SIZE 8
#define PIC_FILE_HEADER_VERSION 14

#define PIC_HEADER_NUMBER 0
#define PIC_HEADER_WIDTH 2
#define PIC_HEADER_HEIGHT 4

extern bb_map_t *blorb_map;

static struct {
	int z_num;
	int width;
	int height;
	int orig_width;
	int orig_height;
	uint32 type;
} *pict_info;
static int num_pictures = 0;


#ifndef NO_BLORB

static void safe_mvaddch(int, int, int);

/*
 * Do a rounding division, rounding to even if fraction part is 1/2.
 * We assume x and y are nonnegative.
 *
 */
static int round_div(int x, int y)
{
	int quotient = x / y;
	int dblremain = (x % y) << 1;

	if ((dblremain > y) || ((dblremain == y) && (quotient & 1)))
		quotient++;
	return quotient;
} /* round_div */
#endif


bool unix_init_pictures (void)
{
#ifndef NO_BLORB
	int maxlegalpic = 0;
	int i, x_scale, y_scale;
	bool success = FALSE;

	unsigned char png_magic[8] = {0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A};
	unsigned char ihdr_name[]	  = "IHDR";
	unsigned char jpg_magic[3]	  = {0xFF, 0xD8, 0xFF};
	unsigned char jfif_name[5]	  = {'J', 'F', 'I', 'F', 0x00};

	bb_result_t res;
	bb_resolution_t *reso;
	uint32 pos;

	if (blorb_map == NULL) return FALSE;

	bb_count_resources(blorb_map, bb_ID_Pict, &num_pictures, NULL, &maxlegalpic);
	pict_info = malloc((num_pictures + 1) * sizeof(*pict_info));
	pict_info[0].z_num = 0;
	pict_info[0].height = num_pictures;
	pict_info[0].width = bb_get_release_num(blorb_map);

	reso = bb_get_resolution(blorb_map);
	if (reso) {
		x_scale = reso->px;
		y_scale = reso->py;
	} else {
		y_scale = 200;
		x_scale = 320;
	}

  	for (i = 1; i <= num_pictures; i++) {
		if (bb_load_resource(blorb_map, bb_method_Memory, &res, bb_ID_Pict, i) == bb_err_None) {
			pict_info[i].type = blorb_map->chunks[res.chunknum].type;
			/* Copy and scale. */
			pict_info[i].z_num = i;
			/* Check to see if we're dealing with a PNG file. */
			if (pict_info[i].type == bb_ID_PNG) {
				if (memcmp(res.data.ptr, png_magic, 8) == 0) {
					/* Check for IHDR chunk.  If it's not there, PNG file is invalid. */
					if (memcmp(res.data.ptr+12, ihdr_name, 4) == 0) {
						pict_info[i].orig_width =
						    (*((unsigned char *)res.data.ptr+16) << 24) +
						    (*((unsigned char *)res.data.ptr+17) << 16) +
						    (*((unsigned char *)res.data.ptr+18) <<  8) +
						    (*((unsigned char *)res.data.ptr+19) <<  0);
						pict_info[i].orig_height =
						    (*((unsigned char *)res.data.ptr+20) << 24) +
						    (*((unsigned char *)res.data.ptr+21) << 16) +
						    (*((unsigned char *)res.data.ptr+22) <<  8) +
						    (*((unsigned char *)res.data.ptr+23) <<  0);
					}
				}
			} else if (pict_info[i].type == bb_ID_Rect) {
				pict_info[i].orig_width =
				    (*((unsigned char *)res.data.ptr+0) << 24) +
				    (*((unsigned char *)res.data.ptr+1) << 16) +
				    (*((unsigned char *)res.data.ptr+2) <<  8) +
				    (*((unsigned char *)res.data.ptr+3) <<  0);
				pict_info[i].orig_height =
				    (*((unsigned char *)res.data.ptr+4) << 24) +
				    (*((unsigned char *)res.data.ptr+5) << 16) +
				    (*((unsigned char *)res.data.ptr+6) <<  8) +
				    (*((unsigned char *)res.data.ptr+7) <<  0);
			} else if (pict_info[i].type == bb_ID_JPEG) {
				if (memcmp(res.data.ptr, jpg_magic, 3) == 0) { /* Is it JPEG? */
					if (memcmp(res.data.ptr+6, jfif_name, 5) == 0) { /* Look for JFIF */
						pos = 11;
						while (pos < res.length) {
							pos++;
							if (pos >= res.length) break;	/* Avoid segfault */
							if (*((unsigned char *)res.data.ptr+pos) != 0xFF) continue;
							if (*((unsigned char *)res.data.ptr+pos+1) != 0xC0) continue;
							pict_info[i].orig_width =
							   (*((unsigned char *)res.data.ptr+pos+7)*256) +
							   *((unsigned char *)res.data.ptr+pos+8);
							pict_info[i].orig_height =
							    (*((unsigned char *)res.data.ptr+pos+5)*256) +
							    *((unsigned char *)res.data.ptr+pos+6);
						} /* while */
					} /* JFIF */
				} /* JPEG */
			}
		} /* for */

		pict_info[i].height = round_div(pict_info[i].orig_height * z_header.screen_rows, y_scale);
		pict_info[i].width = round_div(pict_info[i].orig_width *
		z_header.screen_cols, x_scale);

		/* Don't let dimensions get rounded to nothing. */
		if (pict_info[i].orig_height && !pict_info[i].height)
			pict_info[1].height = 1;
		if (pict_info[i].orig_width && !pict_info[i].width)
			pict_info[i].width = 1;

		success = TRUE;
	} /* for */

	if (success) z_header.config |= CONFIG_PICTURES;
	else z_header.flags &= ~GRAPHICS_FLAG;

	return success;
#else
	return FALSE;
#endif
} /* unix_init_pictures */


/* Convert a Z picture number to an index into pict_info.  */
static int z_num_to_index(int n)
{
	int i;
	if (pict_info == NULL)
		return -1;
	for (i = 0; i <= num_pictures; i++) {
		if (pict_info[i].z_num == n)
			return i;
	}
	return -1;
} /* z_num_to_index */


/*
 * os_picture_data
 *
 * Return true if the given picture is available. If so, write the
 * width and height of the picture into the appropriate variables.
 * Only when picture 0 is asked for, write the number of available
 * pictures and the release number instead.
 *
 */
bool os_picture_data(int num, int *height, int *width)
{
	int index;

	*height = 0;
	*width = 0;

	if (pict_info == NULL || !pict_info)
		return FALSE;

	if ((index = z_num_to_index(num)) == -1)
		return FALSE;

	*height = pict_info[index].height;
	*width = pict_info[index].width;

	return TRUE;
} /* os_picture_data */


#ifndef NO_BLORB
/*
 * Do a mvaddch if the coordinates aren't too large.
 *
 */
static void safe_mvaddch(int y, int x, int ch)
{
	if ((y < z_header.screen_rows) && (x < z_header.screen_cols))
		mvaddch(y, x, ch);
} /* safe_mvaddch */


/*
 * Set n chars starting at (x, y), doing bounds checking.
 *
 */
static void safe_scrnset(int y, int x, int ch, int n)
{
	if ((y < z_header.screen_rows) && (x < z_header.screen_cols)) {
		move(y, x);
		if (x + n > z_header.screen_cols)
			n = z_header.screen_cols - x;
		while (n--)
			addch(ch);
	}
} /* safe_scrnset */
#endif


/*
 * os_draw_picture
 *
 * Display a picture at the given coordinates. Top left is (1,1).
 *
 */
/* TODO: handle truncation correctly.  Spec 8.8.3 says all graphics should
 * be clipped to the current window.  To do that, we should probably
 * modify z_draw_picture in the frotz core to pass some extra parameters.
 */
void os_draw_picture(int num, int row, int col)
{
#ifndef NO_BLORB
	int width, height, r, c;
	int saved_x, saved_y;
	static int plus, ltee, rtee, ttee, btee, hline, vline, ckboard;
	static int urcorner, ulcorner, llcorner, lrcorner;
	static bool acs_initialized = FALSE;

	if (!acs_initialized) {
		plus     = u_setup.plain_ascii ? '+'  : ACS_PLUS;
		ltee     = u_setup.plain_ascii ? '<'  : ACS_LTEE;
		rtee     = u_setup.plain_ascii ? '>'  : ACS_RTEE;
		ttee     = u_setup.plain_ascii ? '^'  : ACS_TTEE;
		btee     = u_setup.plain_ascii ? 'v'  : ACS_BTEE;
		hline    = u_setup.plain_ascii ? '-'  : ACS_HLINE;
		vline    = u_setup.plain_ascii ? '|'  : ACS_VLINE;
		ckboard  = u_setup.plain_ascii ? ':'  : ACS_CKBOARD;
		urcorner = u_setup.plain_ascii ? '\\' : ACS_URCORNER;
		ulcorner = u_setup.plain_ascii ? '/'  : ACS_ULCORNER;
		llcorner = u_setup.plain_ascii ? '\\' : ACS_LLCORNER;
		lrcorner = u_setup.plain_ascii ? '/'  : ACS_LRCORNER;
		acs_initialized = TRUE;
	}

	if (!os_picture_data(num, &height, &width) || !width || !height)
		return;
	col--, row--;

	getyx(stdscr, saved_y, saved_x);

	/* General case looks like:
	 *                            /----\
	 *                            |::::|
	 *                            |::42|
	 *                            \----/
	 *
	 * Special cases are:  1 x n:   n x 1:   1 x 1:
	 *
	 *                                ^
	 *                                |
	 *                     <----->    |        +
	 *                                |
	 *                                v
	 */

	if ((height == 1) && (width == 1))
		safe_mvaddch(row, col, plus);
	else if (height == 1) {
		safe_mvaddch(row, col, ltee);
		safe_scrnset(row, col + 1, hline, width - 2);
		safe_mvaddch(row, col + width - 1, rtee);
	} else if (width == 1) {
		safe_mvaddch(row, col, ttee);
		for (r = row + 1; r < row + height - 1; r++)
			safe_mvaddch(r, col, vline);
		safe_mvaddch(row + height - 1, col, btee);
	} else {
		safe_mvaddch(row, col, ulcorner);
		safe_scrnset(row, col + 1, hline, width - 2);
		safe_mvaddch(row, col + width - 1, urcorner);
		for (r = row + 1; r < row + height - 1; r++) {
			safe_mvaddch(r, col, vline);
			safe_scrnset(r, col + 1, ckboard, width - 2);
			safe_mvaddch(r, col + width - 1, vline);
		}
		safe_mvaddch(row + height - 1, col, llcorner);
		safe_scrnset(row + height - 1, col + 1, hline, width - 2);
		safe_mvaddch(row + height - 1, col + width - 1, lrcorner);
	}

	/* Picture number.  */
	if (height > 2) {
		for (c = col + width - 2; c > col && num > 0; c--, (num /= 10))
			safe_mvaddch(row + height - 2, c, '0' + num % 10);
	}

	move(saved_y, saved_x);
#endif
} /* os_draw_picture */


/*
 * os_peek_colour
 *
 * Return the colour of the pixel below the cursor. This is used
 * by V6 games to print text on top of pictures. The coulor need
 * not be in the standard set of Z-machine colours. To handle
 * this situation, Frotz extends the colour scheme: Values above
 * 15 (and below 256) may be used by the interface to refer to
 * non-standard colours. Of course, os_set_colour must be able to
 * deal with these colours. Interfaces which refer to characters
 * instead of pixels might return the current background colour
 * instead.
 *
 */
int os_peek_colour(void)
{
	if (u_setup.color_enabled) {
#ifdef COLOR_SUPPORT
		short fg, bg;
		pair_content(PAIR_NUMBER(inch() & A_COLOR), &fg, &bg);
		switch(bg) {
		  case COLOR_BLACK: return BLACK_COLOUR;
		  case COLOR_RED: return RED_COLOUR;
		  case COLOR_GREEN: return GREEN_COLOUR;
		  case COLOR_YELLOW: return YELLOW_COLOUR;
		  case COLOR_BLUE: return BLUE_COLOUR;
		  case COLOR_MAGENTA: return MAGENTA_COLOUR;
		  case COLOR_CYAN: return CYAN_COLOUR;
		  case COLOR_WHITE: return WHITE_COLOUR;
		}
		return 0;
#endif /* COLOR_SUPPORT */
  	} else {
   		 return (inch() & A_REVERSE) ?
			z_header.default_foreground : z_header.default_background;
	}
} /* os_peek_colour */
