/*
 * dospic.c - DOS interface, picture functions
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

#ifdef __WATCOMC__
#include <malloc.h>
#include <conio.h>
#else
#include <alloc.h>
#endif

#include <dos.h>
#include <stdio.h>
#include <string.h>
#include "frotz.h"
#include "dosfrotz.h"

int scaler = 1;

#ifndef NO_GRAPHICS

#define PIC_NUMBER 0
#define PIC_WIDTH 2
#define PIC_HEIGHT 4
#define PIC_FLAGS 6
#define PIC_DATA 8
#define PIC_COLOUR 11

#ifdef __WATCOMC__
#define READ_BYTE(v,p,o)	((v) = *(byte _huge *)((p) + (o)))
#define READ_WORD(v,p,o)	((v) = *(word _huge *)((p) + (o)))
#else
#define READ_BYTE(v,p,o)	v = *(byte far *)(p+o)
#define READ_WORD(v,p,o)	v = *(word far *)(p+o)
#endif

extern volatile byte _far *get_scrnptr (int);

extern FILE *os_path_open(const char *, const char *);

static struct {
	byte fileno;
	byte flags;
	word unused1;
	word images;
	word link;
	byte entry_size;
	byte padding;
	word checksum;
	word unused2;
	word version;
} gheader;

static word pic_width = 0;
static word pic_height = 0;
static word pic_flags = 0;
static long pic_data = 0;
static long pic_colour = 0;

static byte _far *table_val = NULL;
static word _far *table_ref = NULL;

static FILE *file = NULL;

#ifdef __WATCOMC__
static byte _huge *info = NULL;
#else
static byte far *info = NULL;
#endif


/*
 * open_graphics_file
 *
 * Open a graphics file. EGA pictures may be stored in two separate
 * graphics files.
 *
 */
static bool open_graphics_file(int number)
{
	char fname[MAX_FILE_NAME + 1];
	char extension[4 + 1];

#ifdef __WATCOMC__
	word images;
	byte entry_size;
	byte _huge *p;
#endif
	/* Build graphics file name */
	extension[0] = '.';
	extension[1] = "cmem"[display - 2];
	extension[2] = 'g';
	extension[3] = '0' + number;
	extension[4] = 0;

	/* Why does DOS not like "graphics\\" anymore? */
	strcpy(fname, "graphics/");
	strcat(fname, f_setup.story_name);
	strcat(fname, extension);

	/* Open file, load header, allocate memory, load picture directory */
	if ((file = fopen(fname, "rb")) == NULL)
		goto failure1;
	if (fread(&gheader, sizeof(gheader), 1, file) != 1)
		goto failure2;
#ifdef __WATCOMC__
	images = gheader.images;
	entry_size = gheader.entry_size;
	if ((info = halloc((long)images * entry_size, 1)) == NULL)
		goto failure2;
	p = info;
	while (images != 0) {
		if (fread((void _far *)p, gheader.entry_size, 1, file) != 1)
			goto failure3;
		p += entry_size;
		--images;
	}
#else
	if ((info = farmalloc(gheader.images * gheader.entry_size)) == NULL)
		goto failure2;
	if (fread(info, gheader.entry_size, gheader.images, file) !=
	    gheader.images)
		goto failure3;
#endif
	return TRUE;

failure3:
#ifdef __WATCOMC__
	hfree(info);
#else
	farfree(info);
#endif
	info = NULL;

failure2:
	fclose(file);
	file = NULL;

failure1:
	return FALSE;
} /* open_graphics_file */


/*
 * close_graphics_file
 *
 * Free resources allocated for pictures.
 *
 */
static void close_graphics_file(void)
{
	if (file != NULL) {
		fclose(file);
		file = NULL;
	}
	if (info != NULL) {
#ifdef __WATCOMC__
		hfree(info);
#else
		farfree(info);
#endif
		info = NULL;
	}
} /* close_graphics_file */


/*
 * init_pictures
 *
 * Prepare to draw pictures. Return true if pictures are available.
 *
 */
bool init_pictures(void)
{

	/* Allocate memory for decompression */
	table_val = (byte _far *) _fmalloc(3 * 3840);
	table_ref = (word _far *) (table_val + 3840);
	if (table_val == NULL)
		return FALSE;

	/* Open the [first of two] graphics file[s] */
	return open_graphics_file(1);

} /* init_pictures */


/*
 * reset_pictures
 *
 * Free resources allocated for decompression of pictures.
 *
 */
void reset_pictures(void)
{
	if (table_val != NULL) {
#ifdef __WATCOMC__
		_ffree(table_val);
#else
		farfree(table_val);
#endif
		table_val = NULL;
	}
	if (file != NULL) {
		fclose(file);
		file = NULL;
	}
	if (info != NULL) {
#ifdef __WATCOMC__
		hfree(info);
#else
		farfree(info);
#endif
		info = NULL;
	}
} /* reset_pictures */


/*
 * load_picture_info
 *
 * Helper function for os_picture_data. Load all information about
 * the given picture from the graphics file and store it in global
 * variables.
 *
 */
static bool load_picture_info(int picture)
{
#ifdef __WATCOMC__
	byte _huge *ptr;
#else
	byte far *ptr;
#endif
	byte fileno;

	fileno = gheader.fileno;

	do {
		int i;

		/* Abort if there is a problem with the graphics file */
		if (file == NULL)
			return FALSE;

		/* Scan the directory of the current graphics file */
		ptr = info;
		for (i = 0; i < gheader.images; i++) {
			if (picture == *(int far *)ptr) {
				READ_WORD(pic_width, ptr, PIC_WIDTH);
				READ_WORD(pic_height, ptr, PIC_HEIGHT);
				READ_WORD(pic_flags, ptr, PIC_FLAGS);

				pic_height *= scaler;
				pic_width *= scaler;

				READ_BYTE(byte0(pic_data), ptr, PIC_DATA + 2);
				READ_BYTE(byte1(pic_data), ptr, PIC_DATA + 1);
				READ_BYTE(byte2(pic_data), ptr, PIC_DATA);

				if (gheader.entry_size > PIC_COLOUR + 2) {
					READ_BYTE(byte0(pic_colour), ptr,
						  PIC_COLOUR + 2);
					READ_BYTE(byte1(pic_colour), ptr,
						  PIC_COLOUR + 1);
					READ_BYTE(byte2(pic_colour), ptr,
						  PIC_COLOUR);
				} else
					pic_colour = 0;

				return TRUE;
			}
			ptr += gheader.entry_size;
		}

		/* Close current graphics file */
		close_graphics_file();

		/* Open next graphics file */
		open_graphics_file((gheader.link !=
				    0) ? gheader.fileno + 1 : 1);

	} while (fileno != gheader.fileno);

	return FALSE;
} /* load_picture_info */


/*
 * load_colour_map
 *
 * Helper function for os_draw_picture. Load a colour map from the
 * graphics file then copy it to the palette registers.
 *
 */
static void load_colour_map(int first_colour)
{
	byte rgb[42];
	int n, i;

	fseek(file, pic_colour, SEEK_SET);

	/* Some pictures from Arthur mistakenly claim to have 16 colours */
	if ((n = fgetc(file)) == 16)
		n = 14;

	/* Each colour is stored in three bytes R-G-B */
	fread(rgb, 3, n, file);

	/* MCGA boards can only handle R-G-B values from 0 to 63 */
	for (i = 0; i < 42; i++)
		rgb[i] = (rgb[i] * 63 + 128) / 255;

	/* Synchronise with vertical retrace */
#ifdef __WATCOMC__
	while ((inp(0x03da) & 8) == 0) ;
	while ((inp(0x03da) & 8) == 8) ;
#else
	while ((inportb(0x03da) & 8) == 0) ;
	while ((inportb(0x03da) & 8) == 8) ;
#endif
	/* Copy colours to palette registers */
#ifdef __WATCOMC__
	bios_video_ax_bx_cx_esdx(0x1012, first_colour, n, rgb);
#else
	asm mov ax, 0x1012
	asm mov bx, first_colour
	asm mov cx, n
	asm lea dx, rgb
	asm push ss
	asm pop es
	asm int 0x10
#endif
} /* load_colour_map */


/*
 * draw_picture
 *
 * Helper function for os_draw_picture. The real work of drawing a
 * picture happens here.
 *
 */
#pragma warn -def

#ifdef __WATCOMC__
static void draw_picture(int y, int x)
#else
static void pascal draw_picture(int y, int x)
#endif
{
	static int raise_bits[4] = {
		0x0100, 0x0300, 0x0700, 0x0000
	};

	byte buf[512];
#ifdef __WATCOMC__
	volatile byte _far *screen;
#else
	unsigned long screen;
#endif
	byte transparent;
	byte colour_shift;
	int first_colour;
	int code, prev_code;
	int next_entry;
	int bits_per_code;
	int bits_shift;
	int bits;
	int current_y;
	int current_x;
	int bufpos;
	int pixels;
	int i;

	bufpos = 0;

	/* When the given picture provides a colour map then activate it.
	   This is only used for MCGA pictures; the colour map affects
	   every picture on the screen. The first colour to be defined is
	   colour 2. Every map defines up to 14 colours (colour 2 to 15).
	   These colours are not related to the standard Z-machine colour
	   scheme which remains unchanged. (This is based on the Amiga
	   interpreter which had to work with 16 colours. Colours 0 and 1
	   were used for text; changing the text colours actually changed
	   palette entries 0 and 1. This interface uses the same trick in
	   Amiga mode.) */
	if (display == _CGA_)
		colour_shift = -2;
	if (display == _EGA_)
		colour_shift = 0;
	if (display == _MCGA_) {
		colour_shift = 32;
		first_colour = 34;
	}
	if (display == _AMIGA_) {
		colour_shift = -1;
		first_colour = 65;
	}

	if (pic_colour != 0)
		load_colour_map(first_colour);

	fseek(file, pic_data, SEEK_SET);

	/* Bit 0 of "flags" indicates that the picture uses a transparent
	   colour, the top four bits tell us which colour it is. For CGA
	   and MCGA pictures this is always 0; for EGA pictures it can be
	   any colour between 0 and 15. */
	transparent = 0xff;

	if (pic_flags & 1)
		transparent = pic_flags >> 12;

	/* Prepare EGA hardware for setting pixels */
	if (display >= _EGA_) {
		outport(0x03ce, 0x0205);
		outport(0x03ce, 0xff08);
	}

	/* The uncompressed picture is a long sequence of bytes. Every
	   byte holds the colour of a pixel, starting at the top left,
	   stopping at the bottom right. We keep track of our position
	   in the current line. (There is a special case: CGA pictures
	   with no transparent colour are stored as bit patterns, i.e.
	   every byte holds the pattern for eight pixels. A pixel must
	   be white if the corresponding bit is set, otherwise it must
	   be black.) */
	current_x = x + pic_width;
	current_y = y - 1;
#ifdef __WATCOMC__
	screen = get_scrnptr(current_y);
#endif

	/* The compressed picture is a stream of bits. We read the file
	   byte-wise, storing the current byte in the variable "bits".
	   Several bits make one code; the variable "bits_shift" helps
	   us to build the next code. */
	bits_shift = 0;
	bits = 0;

reset_table:

	/* Clear the table. We use a table of 3840 entries. Each entry
	   consists of both a value and a reference to another table
	   entry. Following these references we get a sequence of
	   values. At the start of decompression all table entries are
	   undefined. Later we see how entries are set and used. */
	next_entry = 1;

	/* At the start of decompression 9 bits make one code; during
	   the process this can rise to 12 bits per code. 9 bits are
	   sufficient to address both 256 literal values and 256 table
	   entries; 12 bits are sufficient to address both 256 literal
	   values and all 3840 table entries. The number of bits per
	   code rises with the number of table entries. When the table
	   is cleared, the number of bits per code drops back to 9. */
	bits_per_code = 9;

next_code:

	/* Read the next code from the graphics file. This requires
	   some confusing bit operations. Note that low bits always
	   come first. Usually there are a few bits left over from
	   the previous code; these bits must be used before further
	   bits are read from the graphics file. */
	code = bits >> (8 - bits_shift);

	do {
		bits = fgetc(file);
		code |= bits << bits_shift;
		bits_shift += 8;
	} while (bits_shift < bits_per_code);

	bits_shift -= bits_per_code;
	code &= 0xfff >> (12 - bits_per_code);

	/* There are two codes with a special meaning. The first one
	   is 256 which clears the table and sets the number of bits
	   per code to 9. (This is necessary when the table is full.)
	   The second one is 257 which marks the end of the picture.
	   For the sake of efficiency, we decrement the code by 256. */
	code -= 256;

	if (code == 0)
		goto reset_table;
	if (code == 1)
		return;

	/* Codes from 0 to 255 are literals, i.e. they represent a
	   plain byte value. Codes from 258 onwards are references
	   to table entries, i.e. they represent a sequence of byte
	   values (see the remarks on the table above). This means
	   that for each code one or several byte values are added
	   to the decompressed picture. But there is yet more work
	   to do: Every time we read a code one table entry is set.
	   As we said above, a table entry consist of both a value
	   and a reference to another table entry. If the current
	   code is a literal, then the value has to be set to this
	   literal; but if the code refers to a sequence of byte
	   values, then the value has to be set to the last byte of
	   this sequence. In any case, the reference is set to the
	   previous code. Finally, one should be aware that a code
	   may legally refer to the table entry which is currently
	   being set. This requires some extra care. */
	table_ref[next_entry] = prev_code;

	prev_code = code;

	while (code >= 0) {
		buf[bufpos++] = table_val[code];
		code = (short)table_ref[code];
	}

	if (next_entry == prev_code)
		buf[0] = code;

	table_val[next_entry] = code;

	/* The number of bits per code is incremented when the current
	   number of bits no longer suffices to address all defined
	   table entries; but in any case the number of bits may never
	   be greater than 12. */
	next_entry++;

	if (next_entry == raise_bits[bits_per_code - 9])
		bits_per_code++;

reverse_buffer:

	/* Append the sequence of byte values (pixels) to the picture.
	   The order of the sequence must be reversed. (This is why we
	   have stored the sequence in a buffer; experiments show that
	   a buffer of 512 bytes suffices.) The sequence of values may
	   spread over several lines of the picture, so we must take
	   care to start a new line when we reach the right border of
	   the picture. */
	if (current_x == x + pic_width) {
		screen = get_scrnptr(current_y);
		current_x -= pic_width;
		current_y += scaler;
	}

	/* Either add a single pixel or a pattern of eight bits (b/w
	   CGA pictures without a transparent colour) to the current
	   line. Increment our position by 1 or 8 respectively. The
	   pixel may have to be painted several times if the scaling
	   factor is greater than one. */
	if (display == _CGA_ && transparent == 0xff) {
		pixels = x + pic_width - current_x;
		if (pixels > 8)
			pixels = 8;

#ifdef __WATCOMC__
		_asm {
			les bx, screen
			mov dx, current_x
			dec dx
			push dx
			mov cl, 3
			shr dx, cl
			add bx, dx
			mov ax, es:[bx]
			mov dx, 0xffff
			mov cl, byte ptr pixels
			shr dl, cl
			pop cx
			and cl, 7
			ror dx, cl
			and ax, dx
			mov dx, code
			inc dh
			ror dx, cl
			or ax, dx
			mov es:[bx], ax
		}
#else
		asm les bx, screen
		asm mov dx, current_x
		asm dec dx
		asm push dx
		asm mov cl, 3
		asm shr dx, cl
		asm add bx, dx
		asm mov ax, es:[bx]
		asm mov dx, 0xffff
		asm mov cl, byte ptr pixels
		asm shr dl, cl
		asm pop cx
		asm and cl, 7
		asm ror dx, cl
		asm and ax, dx
		asm mov dx, code
		asm inc dh
		asm ror dx, cl
		asm or ax, dx
		asm mov es:[bx], ax
#endif
		current_x += pixels;
	} else for (i = 0; i < scaler; i++) {
#ifdef __WATCOMC__
		int displ = display;
		_asm {
			mov ah, byte ptr code
			cmp ah, transparent
			jz L4
			add ah, colour_shift
			les bx, screen
			mov dx, current_x
			dec dx
			mov si, displ
			cmp si, _MCGA_
			jz L2
			push dx
			mov cl, 3
			shr dx, cl
			pop cx
			and cl, 7
			add bx, dx
			mov al, es:[bx]
			cmp si, _CGA_
			jnz L1
			mov dl, 0x7f
			ror dl, cl
			and al, dl
			xor ah, 1
			ror ah, 1
			shr ah, cl
			or ah, al
			jmp L3
		L1:	mov al, 0x80
			shr al, cl
			mov dx, 0x03cf
			out dx, al
			jmp L3
		L2:	add bx, dx
		L3:	mov es:[bx], ah
			cmp si, _AMIGA_
			jnz L4
			add bx, 80
			mov al, es:[bx]
			mov es:[bx], ah
		L4:
#else
		_AH = code;
		if (_AH != transparent) {
			asm add ah, colour_shift
			asm les bx, screen
			asm mov dx, current_x
			asm dec dx
			if (display != _MCGA_) {
				asm push dx
				asm mov cl, 3
				asm shr dx, cl
				asm pop cx
				asm and cl, 7
				asm add bx, dx
				asm mov al, es:[bx]
				if (display == _CGA_) {
					asm mov dl, 0x7f
					asm ror dl, cl
					asm and al, dl
					asm xor ah, 1
					asm ror ah, 1
					asm shr ah, cl
					asm or ah, al
				} else {
					asm mov al, 0x80
					asm shr al, cl
					asm mov dx, 0x03cf
					asm out dx, al
				}
			} else
				asm add bx, dx

			asm mov es:[bx], ah

			if (display == _AMIGA_) {
				asm add bx, 80
				asm mov al, es:[bx]
				asm mov es:[bx], ah
			}
#endif
		}
		current_x++;
	}

	/* If there are no more values in the buffer then read the
	   next code from the file. Otherwise fetch the next byte
	   value from the buffer and continue painting the picture. */
	if (bufpos == 0)
		goto next_code;

	byte0(code) = buf[--bufpos];
	goto reverse_buffer;
} /* draw_picture */

#pragma warn +def

/*
 * os_draw_picture
 *
 * Display a picture at the given coordinates. Top left is (1,1).
 *
 */
void os_draw_picture(int picture, int y, int x)
{
	if (load_picture_info(picture))
		draw_picture(y, x);
} /* os_draw_picture */


/*
 * os_peek_colour
 *
 * Return the colour of the pixel below the cursor. This is used
 * by V6 games to print text on top of pictures. The colour need
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
	if (display >= _CGA_) {
#ifdef __WATCOMC__
		return (int)(byte)bios_video_ah_bh_cx_dx(13, 0,
				    cursor_x, cursor_y) + 16;
#else
		asm mov ah, 13
		asm mov bh, 0
		asm mov cx, cursor_x
		asm mov dx, cursor_y
		asm int 0x10
		asm mov ah, 0
		return _AX + 16;
#endif
	}
	return current_bg;
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
	bool avail;

	if (picture == 0) {
		avail = FALSE;

		/* This is the special case mentioned above. In practice, only
		   the release number is used; and even this is only used by
		   the DOS version of "Zork Zero". Infocom's Amiga interpreter
		   could not handle this feature, and the Amiga version of the
		   story file does not use it. */

		pic_height = gheader.images;
		pic_width = gheader.version;

	} else
		avail = load_picture_info(picture);

	*height = pic_height;
	*width = pic_width;

	return avail;
} /* os_picture_data */

#else /* NO_GRAPHICS */

bool init_pictures(void) { return FALSE; }
void reset_pictures(void) {}
int os_peek_colour(void) { return current_bg; }
void os_draw_picture(int picture, int y, int x) {}
bool os_picture_data(int picture, int *height, int *width) { return FALSE; }

#endif
