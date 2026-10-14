/*
 * x_screen.c
 *
 * X interface, screen manipulation
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
#include <stdio.h>


/*
 * os_erase_area
 *
 * Fill a rectangular area of the screen with the current background
 * colour. Top left coordinates are (1,1). The cursor does not move.
 *
 */
void os_erase_area(int top, int left, int bottom, int right, int UNUSED(win))
{
	XFillRectangle(dpy, main_window, reversed_gc,
		       left - 1, top - 1, right - left + 1, bottom - top + 1);
} /* os_erase_area */


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
	if (units == 0)
		return;
	else if (units > bottom - top || units < top - bottom)
		XFillRectangle(dpy, main_window, reversed_gc,
			       left - 1, top - 1, right - left + 1,
			       bottom - top + 1);
	else if (units > 0) {
		XCopyArea(dpy, main_window, main_window, current_gc,
			  left - 1, top - 1 + units,
			  right - left + 1, bottom - top - units + 1,
			  left - 1, top - 1);
		XFillRectangle(dpy, main_window, reversed_gc,
			       left - 1, bottom - units, right - left + 1,
			       units);
	} else {
		XCopyArea(dpy, main_window, main_window, current_gc,
			  left - 1, top - 1, right - left + 1,
			  bottom - top + units + 1, left - 1, top - 1 + units);
		XFillRectangle(dpy, main_window, reversed_gc, left - 1, top - 1,
			       right - left + 1, -units);
	}
} /* os_scroll_area */


bool os_repaint_window(int UNUSED (win), int UNUSED (ypos_old),
	int UNUSED (ypos_new), int UNUSED (xpos), int UNUSED (ysize),
	int UNUSED (xsize))
{
	return TRUE;
}


/*
 * os_from_true_colour
 *
 * Given a true colour, return an appropriate colour index.
 *
 * This is pretty brain-dead, and just makes it 3-bit true-color first.
 * Eventually this will be changed to how sfrotz does it.
 *
 */
int os_from_true_colour(zword colour)
{
	if (colour == 0xfffe)
		return 0;
	else if (colour == 0xffff)
		return 1;
	else {
		int r = colour & 0x001F;
		int g = colour & 0x03E0;
		int b = colour & 0x7C00;
		int index = (r ? 4 : 0) | (g ? 2 : 0) | (b ? 1 : 0);

		switch (index) {
		case 0: return 2;
		case 1: return 6;
		case 2: return 4;
		case 3: return 8;
		case 4: return 3;
		case 5: return 7;
		case 6: return 5;
		case 7: return 9;
		default: return 1; /* Can't happen */
		}
	}
}


/*
 * os_to_true_colour
 *
 * Given a colour index, return the appropriate true colour.
 *
 * Eventually this will be changed to how sfrotz does it.
 *
 */
zword os_to_true_colour(int index)
{
	switch (index) {
	case 0: return -2;
	case 1: return -1;
	case 2: return 0x0000;
	case 3: return 0x001D;
	case 4: return 0x0340;
	case 5: return 0x03BD;
	case 6: return 0x59A0;
	case 7: return 0x7C1F;
	case 8: return 0x77A0;
	case 9: return 0x7FFF;
	case 10: return 0x5AD6;
	case 11: return 0x4631;
	case 12: return 0x2D6B;
	default: return 0x0000;
	}
}
