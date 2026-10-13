/*
 * dpic.c - Dumb interface, picture outline functions
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

#include "dfrotz.h"
#include "dblorb.h"

extern f_setup_t f_setup;
extern z_header_t z_header;

#ifndef NO_BLORB

static struct {
	int z_num;
	int width;
	int height;
	int orig_width;
	int orig_height;
	uint32 type;
} *pict_info;
static int num_pictures = 0;

extern bb_map_t *blorb_map;

static int round_div(int x, int y)
{
	int quotient = x / y;
	int dblremain = (x % y) << 1;

	if ((dblremain > y) || ((dblremain == y) && (quotient & 1)))
		quotient++;
	return quotient;
} /* round_div */
#endif /* NO_BLORB */


bool dumb_init_pictures (void)
{
#ifndef NO_BLORB
	int maxlegalpic = 0;
	int i, x_scale, y_scale;
	bool success = FALSE;

	unsigned char png_magic[8]	= {0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A};
	unsigned char ihdr_name[]	= "IHDR";
	unsigned char jpg_magic[3]	= {0xFF, 0xD8, 0xFF};
	unsigned char jfif_name[5]	= {'J', 'F', 'I', 'F', 0x00};

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
			} /* if */
		} /* if */

		pict_info[i].height = round_div(pict_info[i].orig_height * z_header.screen_rows, y_scale);
		pict_info[i].width = round_div(pict_info[i].orig_width * z_header.screen_cols, x_scale);

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
} /* dumb_init_pictures */


/* Convert a Z picture number to an index into pict_info.  */
#ifndef NO_BLORB
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
#endif


bool os_picture_data(int num, int *height, int *width)
{
#ifndef NO_BLORB
	int index;

	*height = 0;
	*width = 0;

	if (pict_info == NULL || !pict_info)
		return FALSE;

	if ((index = z_num_to_index(num)) == -1)
		return FALSE;

	*height = pict_info[index].height;
	*width = pict_info[index].width;
#endif
	return TRUE;
} /* os_picture_data */


void os_draw_picture (int num, int row, int col)
{
#ifndef NO_BLORB
	int width, height, r, c;
	if (!os_picture_data(num, &height, &width) || !width || !height)
		return;
	col--, row--;
	/* Draw corners */
	dumb_set_picture_cell(row, col, '+');
	dumb_set_picture_cell(row, col + width - 1, '+');
	dumb_set_picture_cell(row + height - 1, col, '+');
	dumb_set_picture_cell(row + height - 1, col + width - 1, '+');
	/* sides */
	for (c = col + 1; c < col + width - 1; c++) {
		dumb_set_picture_cell(row, c, '-');
		dumb_set_picture_cell(row + height - 1, c, '-');
	}
	for (r = row + 1; r < row + height - 1; r++) {
		dumb_set_picture_cell(r, col, '|');
		dumb_set_picture_cell(r, col + width - 1, '|');
	}
	/* body, but for last line */
	for (r = row + 1; r < row + height - 2; r++) {
		for (c = col + 1; c < col + width - 1; c++)
			dumb_set_picture_cell(r, c, ':');
	}
	/* Last line of body, including picture number.  */
	if (height >= 3) {
		for (c = col + width - 2; c > col; c--, (num /= 10))
			dumb_set_picture_cell(row + height - 2, c, num ? (num % 10 + '0') : ':');
	}
#endif
} /* os_draw_picture */


int os_peek_colour (void) {return BLACK_COLOUR; }
