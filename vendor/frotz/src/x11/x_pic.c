/*
 * x_pic.c
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
#ifndef NO_BLORB

#include <X11/Xutil.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>


#include <math.h>
#include <stdint.h>

#include "png.h"
#include <setjmp.h>

#include "x_blorb.h"

#define PIC_FILE_HEADER_FLAGS 1
#define PIC_FILE_HEADER_NUM_IMAGES 4
#define PIC_FILE_HEADER_ENTRY_SIZE 8
#define PIC_FILE_HEADER_VERSION 14

#define PIC_HEADER_NUMBER 0
#define PIC_HEADER_WIDTH 2
#define PIC_HEADER_HEIGHT 4

extern bb_map_t *blorb_map;
static bool m_adaptiveMode = FALSE;

typedef struct
{
	int z_num;
	int width;
	int height;
	int orig_width;
	int orig_height;
	uint32_t type;
	uint8_t *pixels;
	uint32_t palette[16];
	int palette_entries;
	int transparentcolor;
	bool	adaptive;
	bool	usespalette;
} pict_info_t;
static pict_info_t *pict_info;
static int num_pictures = 0;

static int x_loadpic(int picture, pict_info_t * graphic);

/* Default picture resource width and height */
static int blrb_width = 512;
static int blrb_height = 400;

/* Dimension of the colorcube for the color table */
#define CCUBEDIM 6
static unsigned long colortab[CCUBEDIM * CCUBEDIM * CCUBEDIM];
static unsigned red_max, green_max, blue_max;
static unsigned red_mult, green_mult, blue_mult;
static unsigned long red_mask, green_mask, blue_mask;
static unsigned red_bits, green_bits, blue_bits;
static unsigned red_shift, green_shift, blue_shift;
static unsigned is_truecolor = 0;
static void init_colormap()
{
	int red, green, blue;
	XColor color;
	int i;
	XVisualInfo *vinfo, vinfotplt;
	int nitems;
	unsigned long tmpmask;

	vinfotplt.visualid = XVisualIDFromVisual(DefaultVisual(dpy,DefaultScreen(dpy)));

	vinfo = XGetVisualInfo(dpy,VisualIDMask,&vinfotplt,&nitems);
	if (nitems) {
		if (vinfo->class == TrueColor) {
			is_truecolor = 1;
			red_mask = vinfo->red_mask;
			green_mask = vinfo->green_mask;
			blue_mask = vinfo->blue_mask;
			tmpmask = red_mask;
			red_shift = 0;
			red_bits = 0;
			while(!(tmpmask & 1)) {
				tmpmask >>= 1;
				red_shift++;
			}
			while((tmpmask & 1)) {
				tmpmask >>= 1;
				red_bits++;
			}

			tmpmask = green_mask;
			green_shift = 0;
			green_bits = 0;
			while(!(tmpmask & 1)) {
				tmpmask >>= 1;
				green_shift++;
			}
			while((tmpmask & 1)) {
				tmpmask >>= 1;
				green_bits++;
			}

			tmpmask = blue_mask;
			blue_shift = 0;
			blue_bits = 0;
			while(!(tmpmask & 1)) {
				tmpmask >>= 1;
				blue_shift++;
			}
			while((tmpmask & 1)) {
				tmpmask >>= 1;
				blue_bits++;
			}
		} else {
			is_truecolor = 0;
			red_max = CCUBEDIM - 1;
			green_max = CCUBEDIM - 1;
			blue_max = CCUBEDIM - 1;
			red_mult = CCUBEDIM * CCUBEDIM;
			green_mult = CCUBEDIM;
			blue_mult = 1;

			i=0;
			for(red = 0; red <= red_max; red++) {
				for(green = 0; green <= green_max; green++) {
					for(blue = 0; blue <= blue_max; blue++) {
						color.red = (0xffff * red)/red_max;
						color.green = (0xffff * green)/green_max;
						color.blue = (0xffff * blue)/blue_max;
						XAllocColor(dpy,
							    DefaultColormap(dpy,
									    DefaultScreen(dpy)),
									    &color);
						colortab[i++] = color.pixel;
					}
				}
			}
		}
	}
	if (vinfo) XFree((char *) vinfo);
}

static void lookup_color(XColor *x)
{
	unsigned r,g,b;
	if (is_truecolor) {
		r = x->red >> (16 - red_bits);
		g = x->green >> (16 - green_bits);
		b = x->blue >> (16 - blue_bits);
		x->pixel = ((r << red_shift) & red_mask) |
			   ((g << green_shift) & green_mask) |
			   ((b << blue_shift) & blue_mask);
	} else {
		r = x->red * (red_max + 1);
		g = x->green * (green_max + 1);
		b = x->blue * (blue_max + 1);
		if (r) r -= 1;
		if (g) g -= 1;
		if (b) b -= 1;
		x->pixel = colortab[((r/0xffff) * red_mult) +
				    ((g/0xffff) * green_mult) +
				    ((b/0xffff) * blue_mult) ];
	}
}

bool x_init_pictures (void)
{
	int maxlegalpic = 0;
	int i,j;
	bool success = FALSE;
	bb_result_t res;
	bb_resolution_t *reso;

	init_colormap();
	if (blorb_map == NULL) return FALSE;

	reso = bb_get_resolution(blorb_map);

	if (reso)
	{
		if (reso->px)
			blrb_width = reso->px;
		if (reso->py)
                        blrb_height = reso->py;
	}
	bb_count_resources(blorb_map, bb_ID_Pict, &num_pictures, NULL, &maxlegalpic);
	pict_info = malloc((num_pictures + 1) * sizeof(*pict_info));
	pict_info[0].z_num = 0;
	pict_info[0].height = num_pictures;
	pict_info[0].width = bb_get_release_num(blorb_map);
	if (bb_load_chunk_by_type (blorb_map,
					   bb_method_Memory,
					   &res,
					   bb_ID_APal,
					   0) == bb_err_None) {
			m_adaptiveMode = TRUE;
			bb_unload_chunk(blorb_map, res.chunknum);
	}


	for (i = 0, j = 0; (i < num_pictures) && (j <= maxlegalpic); j++) {
		if (bb_load_resource(blorb_map, bb_method_Memory, &res, bb_ID_Pict, j) == bb_err_None) {
			i++;
			pict_info[i].type = blorb_map->chunks[res.chunknum].type;
			pict_info[i].transparentcolor = -1;
			/* Copy and scale. */
			pict_info[i].z_num = j;
			x_loadpic(j, &pict_info[i]);
			success = TRUE;
		} /* if */

	} /* for */

	if (success) z_header.config |= CONFIG_PICTURES;
	else z_header.flags &= ~GRAPHICS_FLAG;

	return success;
} /* x_init_pictures */


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



static uint8_t toLinear[256];
static uint8_t fromLinear[256];
extern bool m_adaptiveMode;

uint32_t x_blend(int a, uint32_t s, uint32_t d)
{
	uint32_t r;
	r = fromLinear[(toLinear[s & 0xff] * a +
			toLinear[d & 0xff] * (256 - a)) >> 8];
	s >>= 8;
	d >>= 8;
	r |= (fromLinear
	      [(toLinear[s & 0xff] * a +
		toLinear[d & 0xff] * (256 - a)) >> 8]) << 8;
	s >>= 8;
	d >>= 8;
	r |= (fromLinear
	      [(toLinear[s & 0xff] * a +
		toLinear[d & 0xff] * (256 - a)) >> 8]) << 16;
	return r;
}


/* Set the screen gamma and build gamma correction tables */
void x_setgamma(double gamma)
{
	int i;

	for (i = 0; i < 256; i++)
		toLinear[i] = (int)((pow(i / 255.0, gamma) * 255.0) + 0.5);
	gamma = 1.0 / gamma;
	for (i = 0; i < 256; i++)
		fromLinear[i] = (int)((pow(i / 255.0, gamma) * 255.0) + 0.5);
}

/****************************************************************************
 * Loader for PNG images
 ****************************************************************************
 */

typedef struct {
	uint8_t *gfxData;
	unsigned long offset;
} PNGData;


static void readPNGData(png_structp png_ptr, png_bytep data, png_size_t length)
{
	PNGData *pngData = (PNGData *) png_get_io_ptr(png_ptr);
	memmove(data, pngData->gfxData + pngData->offset, length);
	pngData->offset += length;
}


static int loadpng(uint8_t * data, int UNUSED (length), pict_info_t * graphic)
{
	png_bytep *rowPointers = NULL;
	png_structp png_ptr = NULL;
	png_infop info_ptr = NULL;
	png_infop end_info = NULL;
	PNGData pngData;
	png_uint_32 width, height;
	int color_type, size, bit_depth;
	double gamma;

	graphic->pixels = NULL;
	graphic->orig_width = graphic->orig_height = 0;

	if (!png_check_sig(data, 8))
		return 0;

	png_ptr = png_create_read_struct
	    (PNG_LIBPNG_VER_STRING, (png_voidp) NULL, NULL, NULL);
	if (!png_ptr)
		return 0;

	info_ptr = png_create_info_struct(png_ptr);
	if (!info_ptr) {
		png_destroy_read_struct(&png_ptr,
					(png_infopp) NULL, (png_infopp) NULL);
		return 0;
	}

	end_info = png_create_info_struct(png_ptr);
	if (!end_info) {
		png_destroy_read_struct(&png_ptr, &info_ptr, (png_infopp) NULL);
		return 0;
	}

	if (setjmp(png_jmpbuf(png_ptr))) {
		png_destroy_read_struct(&png_ptr, &info_ptr, &end_info);
		if (rowPointers)
			free(rowPointers);
		if (graphic->pixels) {
			free(graphic->pixels);
			graphic->pixels = NULL;
		}
		return 0;
	}

	pngData.gfxData = data;
	pngData.offset = 8;
	png_set_read_fn(png_ptr, &pngData, readPNGData);

	png_set_sig_bytes(png_ptr, 8);
	png_read_info(png_ptr, info_ptr);

	width = png_get_image_width(png_ptr, info_ptr);
	height = png_get_image_height(png_ptr, info_ptr);
	bit_depth = png_get_bit_depth(png_ptr,info_ptr);
	color_type = png_get_color_type(png_ptr, info_ptr);

	graphic->orig_width = width;
	graphic->orig_height = height;
	graphic->usespalette = FALSE;

	if (png_get_gAMA(png_ptr, info_ptr, &gamma))
		png_set_gamma(png_ptr, 2.2, gamma);

	if (m_adaptiveMode && (color_type == PNG_COLOR_TYPE_PALETTE) && bit_depth <= 4) {
		graphic->usespalette = TRUE;
		png_set_packing(png_ptr);

		/* Check for transparency.  In practice, the transparent
		 * color will always be color 0.
		 */
		png_bytep trans;
		int num_trans;
		png_color_16p trans_values;

		if (png_get_tRNS
		    (png_ptr, info_ptr, &trans, &num_trans, &trans_values)
		    && num_trans >= 1)
			graphic->transparentcolor = trans[0];

		size = width * height;
		graphic->pixels = (uint8_t *) malloc(size);

		rowPointers = malloc(sizeof(png_bytep) * height);
		for (int i = 0; i < (int)height; i++)
			rowPointers[i] = graphic->pixels + (width * i);
		png_read_image(png_ptr, rowPointers);

		/* Get the palette after reading the image, so that the gamma
		 * correction is applied.
		 */
		png_colorp palette;
		int num_palette;
		if (png_get_PLTE(png_ptr, info_ptr, &palette, &num_palette)) {
			graphic->palette_entries = num_palette;
			for (int i = 0; i < num_palette; i++) {
				uint32_t color =
				    palette[i].red | (palette[i].
						      green << 8) | (palette[i].
								     blue <<
								     16);
				graphic->palette[i] = color;
			}
		}
	} else {
		if (graphic->adaptive)
			os_fatal("Non-paletted graphics cannot be adaptive");
		if (color_type == PNG_COLOR_TYPE_PALETTE && bit_depth <= 8)
			png_set_palette_to_rgb(png_ptr);
		if (color_type == PNG_COLOR_TYPE_GRAY && bit_depth < 8)
			png_set_expand_gray_1_2_4_to_8(png_ptr);
		if (png_get_valid(png_ptr,info_ptr,PNG_INFO_tRNS))
			png_set_tRNS_to_alpha(png_ptr);

		if (bit_depth == 16)
			png_set_strip_16(png_ptr);
		if (bit_depth < 8)
			png_set_packing(png_ptr);
		if (color_type == PNG_COLOR_TYPE_GRAY || color_type == PNG_COLOR_TYPE_GRAY_ALPHA)
			png_set_gray_to_rgb(png_ptr);

		png_set_filler(png_ptr,0xff,PNG_FILLER_AFTER);
		size = width*height*4;
		graphic->pixels = (uint8_t *) malloc(size);
		rowPointers = malloc(sizeof(png_bytep) * height);
		for (int i = 0; i < (int)height; i++)
			rowPointers[i] = graphic->pixels + (width * i * 4);
		png_read_image(png_ptr, rowPointers);
	}

	/* Reading done. */
	png_read_end(png_ptr, end_info);
	png_destroy_read_struct(&png_ptr, &info_ptr, &end_info);
	if (rowPointers)
		free(rowPointers);

	return 1;
}

/****************************************************************************
 * Loader for JPEG images
 ****************************************************************************
 */

#include <jpeglib.h>

/* Error Handling */
struct JPEGErrorInfo {
	struct jpeg_error_mgr base;
	jmp_buf errorJump;
};


static void errorJPEGExit(j_common_ptr cinfo)
{
	struct JPEGErrorInfo *error = (struct JPEGErrorInfo *)cinfo->err;
	(*cinfo->err->output_message) (cinfo);
	longjmp(error->errorJump, 1);
}


static void outputJPEGMessage(j_common_ptr cinfo)
{
	char buffer[JMSG_LENGTH_MAX];
	(*cinfo->err->format_message) (cinfo, buffer);
}


/* Memory Data Source */
static void memJPEGInit(j_decompress_ptr UNUSED (unused))
{
	/* Nothing here */
}


static int memJPEGFillInput(j_decompress_ptr UNUSED (unused))
{
	return 0;
}


static void memJPEGSkipInput(j_decompress_ptr cinfo, long num_bytes)
{
	if (num_bytes > 0) {
		if (num_bytes > (long)cinfo->src->bytes_in_buffer)
			num_bytes = (long)cinfo->src->bytes_in_buffer;

		cinfo->src->next_input_byte += num_bytes;
		cinfo->src->bytes_in_buffer -= num_bytes;
	}
}


static void memJPEGTerm(j_decompress_ptr UNUSED (unused))
{
	/* Nothing here */
}


static int loadjpeg(uint8_t * data, int length, pict_info_t * graphic)
{
	struct jpeg_decompress_struct info;
	struct JPEGErrorInfo error;
	int width, height, size;
	JSAMPARRAY buffer;

	graphic->pixels = NULL;
	graphic->orig_width = graphic->orig_height = 0;

	info.err = jpeg_std_error(&(error.base));
	error.base.error_exit = errorJPEGExit;
	error.base.output_message = outputJPEGMessage;
	if (setjmp(error.errorJump)) {
		jpeg_destroy_decompress(&info);
		if (graphic->pixels) {
			free(graphic->pixels);
			graphic->pixels = NULL;
		}
		return 0;
	}

	jpeg_create_decompress(&info);

	info.src = (struct jpeg_source_mgr *)(info.mem->alloc_small)
	    ((j_common_ptr) (&info), JPOOL_PERMANENT,
	     sizeof(struct jpeg_source_mgr));
	info.src->init_source = memJPEGInit;
	info.src->fill_input_buffer = memJPEGFillInput;
	info.src->skip_input_data = memJPEGSkipInput;
	info.src->resync_to_restart = jpeg_resync_to_restart;
	info.src->term_source = memJPEGTerm;
	info.src->bytes_in_buffer = length;
	info.src->next_input_byte = data;

	jpeg_read_header(&info, TRUE);
	jpeg_calc_output_dimensions(&info);
	width = info.output_width;
	height = info.output_height;

	graphic->orig_width = width;
	graphic->orig_height = height;
	size = width * height * 4;
	graphic->pixels = (uint8_t *) malloc(size);

	/* Force RGB output */
	info.out_color_space = JCS_RGB;

	/* Get an output buffer */
	buffer = (*info.mem->alloc_sarray)
	    ((j_common_ptr) & info, JPOOL_IMAGE, width * 3, 1);

	jpeg_start_decompress(&info);
	while ((int)info.output_scanline < height) {
		uint8_t *pixelRow;
		int i;
		jpeg_read_scanlines(&info, buffer, 1);

		pixelRow = graphic->pixels +
		    (width * (info.output_scanline - 1) * 4);
		for (i = 0; i < width; i++) {
			pixelRow[(i * 4) + 0] = (*buffer)[(i * 3) + 0];
			pixelRow[(i * 4) + 1] = (*buffer)[(i * 3) + 1];
			pixelRow[(i * 4) + 2] = (*buffer)[(i * 3) + 2];
			pixelRow[(i * 4) + 3] = 0xFF;
		}
	}
	jpeg_finish_decompress(&info);
	jpeg_destroy_decompress(&info);

	return 1;
}

/****************************************************************************
 * Loader for simple rectangles
 ****************************************************************************
 */

static int loadrect(uint8_t * data, int UNUSED (length), pict_info_t * graphic)
{
	graphic->orig_width =
	    (data[0] << 24) | (data[1] << 16) | (data[2] << 8) | data[3];
	graphic->orig_height =
	    (data[4] << 24) | (data[5] << 16) | (data[6] << 8) | data[7];
	graphic->pixels = NULL;
	return 1;
}

/*****************************/
/* If true, this picture has an adaptive palette */
bool x_IsAdaptive(int picture)
{
	bb_result_t result;
	bool adaptive = FALSE;

	if (blorb_map == NULL) return FALSE;

	if (bb_load_chunk_by_type
	    (blorb_map, bb_method_Memory, &result, bb_ID_APal, 0) == bb_err_None) {
		for (int i = 0; i < (int)result.length; i += 4) {
			unsigned char *data =
			    ((unsigned char *)result.data.ptr) + i;
			int entry =
			    (data[0] << 24) | (data[1] << 16) | (data[2] << 8) |
			    data[3];
			if (picture == entry) {
				adaptive = TRUE;
				break;
			}
		}
	}
	bb_unload_chunk(blorb_map, result.chunknum);
	return adaptive;
}


/* Get a picture from the Blorb resource map */
static int x_loadpic(int picture, pict_info_t * graphic)
{
	myresource res;
	int st = 0;

	/* Set whether graphic has an adaptive palette */
	graphic->adaptive = x_IsAdaptive(picture) ? TRUE : FALSE;

	if (bb_load_resource(blorb_map, bb_method_Memory, (bb_result_t *) &res, bb_ID_Pict, picture) == bb_err_None) {
		uint8_t *data = (uint8_t *) res.bbres.data.ptr;
		int length = res.bbres.length;
		unsigned int id = blorb_map->chunks[res.bbres.chunknum].type;

		/* Look for a recognized format */
		if (id == bb_ID_PNG) {
			st = loadpng(data, length, graphic);
			if (!st)
				st = loadjpeg(data, length, graphic);
		} else if (id == bb_ID_JPEG) {
			st = loadjpeg(data, length, graphic);
			if (!st)
				st = loadpng(data, length, graphic);
		} else if (id == bb_ID_Rect)
			st = loadrect(data, length, graphic);
		if (graphic->orig_width > blrb_width)
		{
		    blrb_height = blrb_height * graphic->orig_width / blrb_width;
		    blrb_width = graphic->orig_width;
		}
		graphic->height = (graphic->orig_height * X_HEIGHT) /blrb_height;
		graphic->width = (graphic->orig_width * X_WIDTH) / blrb_width;
	}

	return st;
}

#if 0
/*******************
 * CACHE
 */

#define MAXCACHE 1

static x_picture cached[MAXCACHE];
static int cacheinited = 0;

static void cacheflush()
{
	int i;
	if (!cacheinited)
		return;
	for (i = 0; i < MAXCACHE; i++) {
		cached[i].number = -1;
		if (cached[i].pixels)
			free(cached[i].pixels);
		cached[i].pixels = NULL;
	}
	cacheinited = 0;
}


static void cacheinit()
{
	int i;
	if (cacheinited)
		return;
	CLEANREG(cacheflush);
	for (i = 0; i < MAXCACHE; i++) {
		cached[i].number = -1;
		cached[i].pixels = NULL;
	}
	%cacheinited = 1;
}


static x_picture *cachefind(int n)
{
	int i;
	for (i = 0; i < MAXCACHE; i++)
		if (cached[i].number == n)
			return (cached + i);
	if (n < 0) {
		cached[0].number = -1;
		if (cached[0].pixels)
			free(cached[0].pixels);
		cached[0].pixels = NULL;
		return (cached + 0);
	}
	return NULL;
}

#endif

pict_info_t *x_getpic(int num)
{
	pict_info_t *res;
	int index;

	if ((index = z_num_to_index(num)) == -1)
		return NULL;

	res = &pict_info[z_num_to_index(num)];

	if (res->pixels)
		return res;
	if (x_loadpic(num, res))
		return res;
	return NULL;
}

static unsigned long screen_palette[16];

/* Apply the picture's palette to the screen palette. */
/* Adapted from FrotzGfx::ApplyPalette() in Windows Frotz. */
static bool ApplyPalette(pict_info_t * graphic)
{
	bool changed = FALSE;
	int i, colors;

	memset(&screen_palette, 0, sizeof(unsigned long));

	if (graphic->usespalette) {
		colors = graphic->palette_entries;
		if (colors > 16)
			colors = 16;
		for (i = 0; i < colors; i++) {
			if (screen_palette[i] != graphic->palette[i]) {
				changed = TRUE;
				screen_palette[i] = graphic->palette[i];
			}
		}
	}
	return changed;
}

/*
 * os_draw_picture
 *
 * Display a picture at the given coordinates. Top left is (1,1).
 *
 */
void os_draw_picture(int picture, int y, int x)
{
	pict_info_t *data;
	XImage *contents_image, *mask_image;
	char *contents, *mask;
	int xpos, ypos, im_x, im_y;
	int image_value;
	XColor pixel_value;

	mask_image = NULL;
	if ((data = x_getpic(picture)) == NULL)
		return;
	if (data->pixels == NULL)
		return;

	contents_image =
	    XCreateImage(dpy, DefaultVisual(dpy, DefaultScreen(dpy)),
			 DefaultDepth(dpy, DefaultScreen(dpy)), ZPixmap, 0,
			 NULL, data->width, data->height, 32, 0);
	contents =
	    malloc(contents_image->height * contents_image->bytes_per_line);
	contents_image->data = contents;
	if (data->transparentcolor != -1) {
		mask_image =
		    XCreateImage(dpy, DefaultVisual(dpy, DefaultScreen(dpy)),
				 /* DefaultDepth(dpy, DefaultScreen(dpy)), */ 1,
				 XYBitmap, 0, NULL,
				 data->width, data->height, 8, 0);
		mask =
		    calloc(mask_image->height * mask_image->bytes_per_line, 1);
		mask_image->data = mask;
	}

	for (ypos = 0; ypos < data->orig_height; ypos++)
		for (xpos = 0; xpos < data->orig_width; xpos++) {
			if (data->usespalette) {
				image_value = data->pixels[ypos*data->orig_width + xpos];
				if (!data->adaptive)
				    ApplyPalette(data);
				pixel_value.blue = (screen_palette[image_value] & 0xff0000) >> 8;
				pixel_value.green = (screen_palette[image_value] & 0x00ff00);
				pixel_value.red = (screen_palette[image_value] & 0x0000ff) << 8;
			} else {
				pixel_value.blue = data->pixels[(ypos*data->orig_width + xpos)*4 +2] *256;
				pixel_value.green = data->pixels[(ypos*data->orig_width + xpos)*4 +1] *256;
				pixel_value.red = data->pixels[(ypos*data->orig_width + xpos)*4] *256;
				image_value = (pixel_value.blue << 8) + (pixel_value.green) + (pixel_value.red >> 8);
			}
			if (image_value == data->transparentcolor)
			{
				pixel_value.red = 0;
				pixel_value.green = 0;
				pixel_value.blue = 0;
			}
			lookup_color(&pixel_value);
			for (im_y = ypos * X_HEIGHT / blrb_height;
				im_y < (ypos + 1) * X_HEIGHT / blrb_height;
				im_y++)
				for (im_x = xpos * X_WIDTH / blrb_width;
				     im_x < (xpos + 1) * X_WIDTH / blrb_width;
				     im_x++)
					XPutPixel(contents_image, im_x, im_y,
						  pixel_value.pixel);
			if (mask_image && image_value == data->transparentcolor)
				for (im_y = ypos * X_HEIGHT / blrb_height;
					im_y < (ypos + 1) * X_HEIGHT / blrb_height;
					im_y++)
					for (im_x = xpos * X_WIDTH / blrb_width;
					     im_x < (xpos + 1) * X_WIDTH / blrb_width;
					     im_x++)
						XPutPixel(mask_image, im_x,
							  im_y, 1);
		}

	if (mask_image) {
		XSetFunction(dpy, bw_gc, GXand);
		XPutImage(dpy, main_window, bw_gc, mask_image, 0, 0,
			  x - 1, y - 1, data->width, data->height);
	}
	XSetFunction(dpy, bw_gc, ((data->transparentcolor != -1) ? GXor : GXcopy));
	XPutImage(dpy, main_window, bw_gc, contents_image, 0, 0,
		  x - 1, y - 1, data->width, data->height);

	XSetFunction(dpy, normal_gc, GXcopy);
	XDestroyImage(contents_image);
	if (mask_image)
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
bool os_picture_data(int picture, int *height, int *width)
{
	int index;
	*height = 0;
	*width = 0;

	if (pict_info == NULL || !pict_info)
		return FALSE;
	if ((index = z_num_to_index(picture)) == -1)
		return FALSE;

	*height = pict_info[index].height;
	*width = pict_info[index].width;

	return TRUE;

} /* os_picture_data */

#endif /* NO_BLORB */
