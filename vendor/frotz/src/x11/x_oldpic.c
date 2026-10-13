/*
 * x_oldpic.c
 *
 * X interface, stubs for picture functions
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
 *
 * Copyright (c) 1998-2000 Daniel Schepler
 *
 */

#include "x_frotz.h"

#ifdef NO_BLORB

#include <X11/Xutil.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

struct picture_data {
	int number, width, height, flags, data, colour;
};


static FILE *graphics_fp = NULL;
static struct {
	int fileno, flags, unused1, images, link, entry_size,
	    padding, checksum, unused2, version;
} gheader;
struct picture_data *gtable;


static int read_word(FILE * fp)
{
	int result;

	result = fgetc(fp);
	result |= fgetc(fp) << 8;
	return result;
}


static int open_graphics_file(void)
{
	char *graphics_name;
	const char *base_start, *base_end;
	int i;

	base_start = strrchr(f_setup.story_name, '/');
	if (base_start)
		base_start++;
	else
		base_start = f_setup.story_name;
	base_end = strrchr(base_start, '.');
	if (!base_end)
		base_end = strchr(base_start, 0);

	if (f_setup.blorb_file == NULL) {
		graphics_name = malloc(base_end - f_setup.story_name + 5);
		sprintf(graphics_name, "%.*s.mg1", (int)(base_end - f_setup.story_name),
			f_setup.story_name);
	} else
		graphics_name = f_setup.blorb_file;

	if ((graphics_fp = fopen(graphics_name, "r")) == NULL)
		os_fatal("Unable to open graphics file: %s", graphics_name);

	gheader.fileno = fgetc(graphics_fp);
	gheader.flags = fgetc(graphics_fp);
	gheader.unused1 = read_word(graphics_fp);
	gheader.images = read_word(graphics_fp);
	gheader.link = read_word(graphics_fp);
	gheader.entry_size = fgetc(graphics_fp);
	gheader.padding = fgetc(graphics_fp);
	gheader.checksum = read_word(graphics_fp);
	gheader.unused2 = read_word(graphics_fp);
	gheader.version = read_word(graphics_fp);

	gtable = malloc(sizeof(struct picture_data) * gheader.images);
	for (i = 0; i < gheader.images; i++) {
		gtable[i].number = read_word(graphics_fp);
		gtable[i].width = read_word(graphics_fp);
		gtable[i].height = read_word(graphics_fp);
		gtable[i].flags = read_word(graphics_fp);
		gtable[i].data = fgetc(graphics_fp) << 16;
		gtable[i].data |= fgetc(graphics_fp) << 8;
		gtable[i].data |= fgetc(graphics_fp);
		if (gheader.entry_size >= 14) {
			gtable[i].colour = fgetc(graphics_fp) << 16;
			gtable[i].colour |= fgetc(graphics_fp) << 8;
			gtable[i].colour |= fgetc(graphics_fp);
		} else
			gtable[i].colour = 0;
		fseek(graphics_fp,
		      gheader.entry_size - (gheader.entry_size >= 14 ? 14 : 11),
		      SEEK_CUR);
	}

	return 1;
}


static struct picture_data *find_picture(int picture)
{
	int i;

	for (i = 0; i < gheader.images; i++)
		if (gtable[i].number == picture)
			return gtable + i;
	return NULL;
}


static unsigned long colourmap[16];
static unsigned long ega_colourmap[16];
static int old_colourmap_pos = -1;

void load_colourmap(int pos)
{
	XColor color_dat;
	int num_colours, i;

	/*  if (pos == old_colourmap_pos) */
	if (pos == old_colourmap_pos || (pos == 0 && old_colourmap_pos != -1))
		return;
	if (old_colourmap_pos == -1) {
		for (i = 0; i < 16; i++) {
			if (i & 1)
				color_dat.blue = 0xaaaa;
			else
				color_dat.blue = 0x0000;
			if (i & 2)
				color_dat.green = 0xaaaa;
			else
				color_dat.green = 0x0000;
			if (i & 4)
				color_dat.red = 0xaaaa;
			else
				color_dat.red = 0x0000;
			if (i & 8) {
				color_dat.blue += 0x5555;
				color_dat.green += 0x5555;
				color_dat.red += 0x5555;
			}
			XAllocColor(dpy,
				    DefaultColormap(dpy, DefaultScreen(dpy)),
				    &color_dat);
			ega_colourmap[i] = color_dat.pixel;
		}
	}

	old_colourmap_pos = pos;

	memcpy(colourmap, ega_colourmap, sizeof(colourmap));
	if (pos == 0)
		return;

	fseek(graphics_fp, pos, SEEK_SET);

	num_colours = fgetc(graphics_fp);
	if (num_colours > 14)
		num_colours = 14;

	for (i = 0; i < num_colours; i++) {
		color_dat.red = fgetc(graphics_fp) * 0x101;
		color_dat.green = fgetc(graphics_fp) * 0x101;
		color_dat.blue = fgetc(graphics_fp) * 0x101;
		XAllocColor(dpy, DefaultColormap(dpy, DefaultScreen(dpy)),
			    &color_dat);
		colourmap[i + 2] = color_dat.pixel;
	}
}


static unsigned char last[3840];
static int prev_seq[3840];
static int current_code, prev_code;
static int bits_left, byte_read, bits_per_code;
static unsigned char buf[512];
static int bufpos;


static void init_decompress(void)
{
	bufpos = 0;
	bits_per_code = 9;
	bits_left = 0;
}


static int read_code(void)
{
	int i, code;

	code = 0;
	i = bits_per_code;
	do {
		if (bits_left <= 0) {
			byte_read = fgetc(graphics_fp);
			bits_left = 8;
		}

		code |= (byte_read >> (8 - bits_left)) << (bits_per_code - i);

		i -= bits_left;
		bits_left = -i;
	} while (i > 0);

	return code & (0xfff >> (12 - bits_per_code));
}


static int decompress(void)
{
	int code, val;

	if (bufpos == 0) {
		while ((code = read_code()) == 256) {
			bits_per_code = 9;
			current_code = 257;
		}

		if (code == 257)
			return -1;

		if (current_code < 4096)
			prev_seq[current_code - 256] = prev_code;
		for (val = code; val > 256; val = prev_seq[val - 256])
			buf[bufpos++] = last[val - 256];
		buf[bufpos++] = val;
		if (code == current_code)
			buf[0] = val;

		if (current_code < 4096)
			last[current_code - 256] = val;
		prev_code = code;

		if (++current_code == 1 << bits_per_code && bits_per_code < 12)
			bits_per_code++;
	}

	return buf[--bufpos];
}


/*
 * os_draw_picture
 *
 * Display a picture at the given coordinates. Top left is (1,1).
 *
 */
void os_draw_picture(int picture, int y, int x)
{
	struct picture_data *data;
	XImage *contents_image, *mask_image;
	char *contents, *mask;
	int transparent = -1;
	int xpos, ypos, im_x, im_y;
	int image_value;
	unsigned long pixel_value;

	if (graphics_fp == NULL)
		if (!open_graphics_file())
			return;

	if ((data = find_picture(picture)) == NULL)
		return;

	load_colourmap(data->colour);
	contents_image =
	    XCreateImage(dpy, DefaultVisual(dpy, DefaultScreen(dpy)),
			 DefaultDepth(dpy, DefaultScreen(dpy)), ZPixmap, 0,
			 NULL, data->width * 5 / 2, data->height * 3, 32, 0);
	contents =
	    malloc(contents_image->height * contents_image->bytes_per_line);
	contents_image->data = contents;
	if (data->flags & 1) {
		transparent = data->flags >> 12;
		mask_image =
		    XCreateImage(dpy, DefaultVisual(dpy, DefaultScreen(dpy)),
				 /* DefaultDepth(dpy, DefaultScreen(dpy)), */ 1,
				 XYBitmap, 0, NULL,
				 data->width * 5 / 2, data->height * 3, 8, 0);
		mask =
		    calloc(mask_image->height * mask_image->bytes_per_line, 1);
		mask_image->data = mask;
	}

	fseek(graphics_fp, data->data, SEEK_SET);
	init_decompress();
	for (ypos = 0; ypos < data->height; ypos++)
		for (xpos = 0; xpos < data->width; xpos++) {
			image_value = decompress();
			pixel_value = colourmap[image_value];
			if (image_value == transparent)
				pixel_value = 0;
			for (im_y = ypos * 3; im_y < ypos * 3 + 3; im_y++)
				for (im_x = xpos * 5 / 2;
				     im_x < (xpos + 1) * 5 / 2; im_x++)
					XPutPixel(contents_image, im_x, im_y,
						  pixel_value);
			if (image_value == transparent)
				for (im_y = ypos * 3; im_y < ypos * 3 + 3;
				     im_y++)
					for (im_x = xpos * 5 / 2;
					     im_x < (xpos + 1) * 5 / 2; im_x++)
						XPutPixel(mask_image, im_x,
							  im_y, 1);
		}

	if (data->flags & 1) {
		XSetFunction(dpy, bw_gc, GXand);
		XPutImage(dpy, main_window, bw_gc, mask_image, 0, 0,
			  x - 1, y - 1, data->width * 5 / 2, data->height * 3);
	}
	XSetFunction(dpy, bw_gc, (data->flags & 1 ? GXor : GXcopy));
	XPutImage(dpy, main_window, bw_gc, contents_image, 0, 0,
		  x - 1, y - 1, data->width * 5 / 2, data->height * 3);

	XSetFunction(dpy, normal_gc, GXcopy);
	XDestroyImage(contents_image);
	if (data->flags & 1)
		XDestroyImage(mask_image);
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
extern long pixel_values[17];

int os_peek_colour(void)
{
	XImage *point;

	point = XGetImage(dpy, main_window, curr_x, curr_y, 1, 1,
			  AllPlanes, ZPixmap);

	pixel_values[16] = XGetPixel(point, 0, 0);
	return 16;
} /* os_peek_colour */


/*
 * os_picture_data
 *
 * Return true if the given picture is available. If so, write the
 * width and height of the picture into the appropriate variables.
 * Only when picture 0 is asked for, write the number of available
 * pictures and the release number instead.
 *
 */
int os_picture_data(int picture, int *height, int *width)
{
	struct picture_data *entry;

	if (graphics_fp == NULL)
		if (!open_graphics_file())
			return 0;

	if (picture == 0) {
		*height = gheader.images;
		*width = gheader.version;
		return 1;
	}

	entry = find_picture(picture);
	if (entry == NULL)
		return 0;
	*height = entry->height * 3;	/* 600 / 200 */
	*width = entry->width * 5 / 2;	/* 800 / 320 */
	return 1;

} /* os_picture_data */

bool x_init_pictures(void)
{
	return TRUE;
}

#endif /* NO_BLORB */
