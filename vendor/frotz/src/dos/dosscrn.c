/*
 * owscreen.c - DOS interface, screen manipulation
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
#include <conio.h>
#endif

#include <dos.h>
#include <mem.h>
#include "frotz.h"
#include "dosfrotz.h"


/*
 * get_scrnptr
 *
 * Return a pointer to the given line in video RAM.
 *
 */
volatile byte _far *get_scrnptr(int y)
{
	if (display == _CGA_)
		return MK_FP((y & 1) ? 0xba00 : 0xb800, 40 * (y & ~1));
	else if (display == _MCGA_)
		return MK_FP(0xa000, 320 * y);
	else
		return MK_FP(0xa000, 80 * y);
} /* get_scrnptr */


/*
 * clear_byte
 *
 * Helper function for clear_line.
 *
 */
static void clear_byte(volatile byte _far * scrn, word mask)
{
	if (display == _CGA_)
		if (scrn_attr == 0)
			*scrn &= ~mask;
		else
			*scrn |= mask;
	else {
#ifdef __WATCOMC__
		outpw(0x03ce, 0x0205);
		outp(0x03ce, 0x08);
		outp(0x03cf, mask);
		video_latch(*scrn);
		*scrn = scrn_attr;
#else
		outport(0x03ce, 0x0205);
		outportb(0x03ce, 0x08);
		outportb(0x03cf, mask);
		asm les bx, scrn
		asm mov al, es:[bx]
		asm mov al, scrn_attr
		asm mov es:[bx], al
#endif
	}
} /* clear_byte */


/*
 * clear_line
 *
 * Helper function for os_erase_area.
 *
 */
static void clear_line(int y, int left, int right)
{
	volatile byte _far *scrn = get_scrnptr(y);
	if (display == _MCGA_)

#ifdef __WATCOMC__
		_fmemset((byte _far *)scrn + left, scrn_attr,
			 right - left + 1);
#else
		_fmemset(scrn + left, scrn_attr, right - left + 1);
#endif
	else {
		word mask1 = 0x00ff >> (left & 7);
		word mask2 = 0xff80 >> (right & 7);

		int x = right / 8 - left / 8;

		scrn += left / 8;

		if (x == 0) {
			mask1 &= mask2;
			mask2 = 0;
		}

		/* Clear first byte */
		clear_byte(scrn++, mask1);

		/* Clear middle bytes */
		if (display >= _EGA_)
			outport(0x03ce, 0xff08);
		while (--x > 0)
			*scrn++ = scrn_attr;

		/* Clear last byte */
		clear_byte(scrn, mask2);
	}
} /* clear_line */


/*
 * os_erase_area
 *
 * Fill a rectangular area of the screen with the current background
 * colour. Top left coordinates are (1,1). The cursor does not move.
 *
 * The final argument gives the window being changed, -1 if only a
 * portion of a window is being erased, or -2 if the whole screen is
 * being erased.  This is not relevant for the DOS interface, and so
 * this function ignores that argument.
 *
 */
void os_erase_area(int top, int left, int bottom, int right, int win)
{
	int y;

	top--;
	left--;
	bottom--;
	right--;

	if (display <= _TEXT_) {
#ifdef __WATCOMC__
		bios_video_ax_bh_ch_cl_dh_dl(0x0600, scrn_attr,
					     top, left, bottom, right);
#else
		asm mov ax, 0x0600
		asm mov ch, byte ptr top
		asm mov cl, byte ptr left
		asm mov dh, byte ptr bottom
		asm mov dl, byte ptr right
		asm mov bh, scrn_attr
		asm int 0x10
#endif
	} else
		for (y = top; y <= bottom; y++)
			clear_line(y, left, right);
} /* os_erase_area */


/*
 * copy_byte
 *
 * Helper function for copy_line.
 *
 */
static void copy_byte(volatile byte _far * scrn1, volatile byte _far * scrn2,
		      byte mask)
{
	int i;
#ifdef __WATCOMC__
	byte t;
#endif

	if (display == _CGA_)
		*scrn1 = (*scrn1 & ~mask) | (*scrn2 & mask);
	else {
		outport(0x03ce, 0x0005);
		outportb(0x03ce, 0x08);
		outportb(0x03cf, mask);
		outportb(0x03ce, 0x04);
		outportb(0x03c4, 0x02);
		for (i = 0; i < 4; i++) {
			outportb(0x03cf, i);
			outportb(0x03c5, 1 << i);
#ifdef __WATCOMC__
			t = *scrn2;
			video_latch(*scrn1);
			*scrn1 = t;
		}
#else
			asm les bx, scrn2
			asm mov ah, es:[bx]
			asm les bx, scrn1
			asm mov al, es:[bx]
			asm mov es:[bx], ah
		}
#endif
		outportb(0x03c5, 0x0f);
	}
} /* copy_byte */


/*
 * copy_line
 *
 * Helper function for os_scroll_area.
 *
 */
static void copy_line(int y1, int y2, int left, int right)
{
	volatile byte _far *scrn1 = get_scrnptr(y1);
	volatile byte _far *scrn2 = get_scrnptr(y2);
	if (display == _MCGA_)
#ifdef __WATCOMC__
		_fmemcpy((byte _far *)(scrn1 + left),
			 (byte _far *)(scrn2 + left), right - left + 1);
#else
		_fmemcpy(scrn1 + left, scrn2 + left, right - left + 1);
#endif
	else {
		word mask1 = 0x00ff >> (left & 7);
		word mask2 = 0xff80 >> (right & 7);

		int x = right / 8 - left / 8;

		scrn1 += left / 8;
		scrn2 += left / 8;

		if (x == 0) {
			mask1 &= mask2;
			mask2 = 0;
		}

		/* Copy first byte */
		copy_byte(scrn1++, scrn2++, mask1);

		/* Copy middle bytes */
		if (display >= _EGA_)
			outport(0x03ce, 0x0105);
		while (--x > 0)
			*scrn1++ = *scrn2++;

		/* Copy last byte */
		copy_byte(scrn1, scrn2, mask2);
	}
} /* copy_line */


/*
 * os_scroll_area
 *
 * Scroll a rectangular area of the screen up (units > 0) or down
 * (units < 0) and fill the empty space with the current background
 * colour. Top left coordinates are (1,1). The cursor stays put.
 *
 */
void os_scroll_area(int top, int left, int bottom, int right, int units)
{
	int y;

	top--;
	left--;
	bottom--;
	right--;

	if (display <= _TEXT_) {
#ifdef __WATCOMC__
		word ax;
		if (units > 0)
			ax = 0x0600 | (byte)units;
		else
			ax = 0x0700 | (byte)-units;
		bios_video_ax_bh_ch_cl_dh_dl(ax, scrn_attr, top, left,
							    bottom, right);
#else
		asm mov ah, 6
		asm mov bx, units
		asm cmp bx, 0
		asm jg scroll
		asm mov ah, 7
		asm neg bx
scroll:
		asm mov al, bl
		asm mov ch, byte ptr top
		asm mov cl, byte ptr left
		asm mov dh, byte ptr bottom
		asm mov dl, byte ptr right
		asm mov bh, scrn_attr
		asm int 0x10
#endif
	} else if (units > 0) {
		for (y = top; y <= bottom; y++) {
			if (y <= bottom - units)
				copy_line(y, y + units, left, right);
			else
				clear_line(y, left, right);
		}
	} else {
		for (y = bottom; y >= top; y--) {
			if (y >= top - units)
				copy_line(y, y + units, left, right);
			else
				clear_line(y, left, right);
		}
	}
} /* os_scroll_area */


bool os_repaint_window(int win, int ypos_old, int ypos_new, int xpos,
		       int ysize, int xsize)
{
	return FALSE;
} /* os_repaint_window */
