/*
 * ux_screen.c - Unix interface, screen manipulation
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

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "ux_defines.h"

#ifdef USE_NCURSES_H
#include <ncurses.h>
#else
#include <curses.h>
#endif

#include "ux_frotz.h"

extern void restart_header(void);

static WINDOW *saved_screen = NULL;

/*
 * os_erase_area
 *
 * Fill a rectangular area of the screen with the current background
 * colour. Top left coordinates are (1,1). The cursor does not move.
 *
 * The final argument gives the window being changed, -1 if only a
 * portion of a window is being erased, or -2 if the whole screen is
 * being erased.  This is not relevant for the curses interface.
 *
 */
void os_erase_area(int top, int left, int bottom, int right, int UNUSED(win))
{
	int y, x, i, j;

	/* Catch the most common situation and do things the easy way */
	if ((top == 1) && (bottom == z_header.screen_rows) &&
		(left == 1) && (right == z_header.screen_cols)) {
#ifdef COLOR_SUPPORT
		/* Only set the curses background when doing an erase,
		 * so it won't interfere with the copying we do in
		 * os_scroll_area.
		 */
		bkgdset(u_setup.current_color | ' ');
		erase();
		bkgdset(0);
#else
		erase();
#endif
	} else {
		/* Sigh... */
		int saved_style = u_setup.current_text_style;
		os_set_text_style(u_setup.current_color);
		getyx(stdscr, y, x);
		top--; left--; bottom--; right--;
		for (i = top; i <= bottom; i++) {
			move(i, left);
			for (j = left; j <= right; j++)
				addch(' ');
		}
		move(y, x);
		os_set_text_style(saved_style);
	}
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
	top--; left--; bottom--; right--;

	if ((left == 0) && (right == z_header.screen_cols - 1)) {
		static int old_scroll_top = 0;
		static int old_scroll_bottom = 0;

		if (!((old_scroll_top == top) && (old_scroll_bottom == bottom))) {
			old_scroll_top = top; old_scroll_bottom = bottom;
			setscrreg(top, bottom);
		}
		scrollok(stdscr, TRUE);
		scrl(units);
		scrollok(stdscr, FALSE);
	} else {
		int row, col, x, y;
		chtype ch;

		getyx(stdscr, y, x);
		/* Must turn off attributes during copying.  */
		attrset(0);
		if (units > 0) {
			for (row = top; row <= bottom - units; row++) {
				for (col = left; col <= right; col++) {
					ch = mvinch(row + units, col);
					mvaddch(row, col, ch);
				}
			}
		} else if (units < 0) {
			for (row = bottom; row >= top - units; row--) {
				for (col = left; col <= right; col++) {
					ch = mvinch(row + units, col);
					mvaddch(row, col, ch);
				}
			}
		}
		/* Restore attributes.  */
		os_set_text_style(u_setup.current_text_style);
		move(y, x);
	}
	if (units > 0)
		os_erase_area(bottom - units + 2, left + 1, bottom + 1, right + 1, 0);
	else if (units < 0)
		os_erase_area(top + 1, left + 1, top - units, right + 1, 0);
} /* os_scroll_area */


static void save_screen(void)
{
	if ((saved_screen = newpad(z_header.screen_rows, z_header.screen_cols))
		&& overwrite(stdscr, saved_screen) == ERR) {
		delwin(saved_screen);
		saved_screen = NULL;
	}
	if (saved_screen) {
		int y, x;
		getyx(stdscr, y, x);
		wmove(saved_screen, y, x);
	}
} /* save_screen */


static void resize_restore_screen(void)
{
	unix_get_terminal_size();
	resize_screen();
	if (zmp != NULL)
		restart_header();
	if (saved_screen) {
		delwin(saved_screen);
		saved_screen = NULL;
	}
} /* resize_restore_screen */


/**
 * Resize the display and redraw.  Retain the old screen starting from the
 * top left.  Call resize_screen, which may repaint more accurately.
 */
void unix_resize_display(void)
{
	save_screen();
	endwin();
	refresh();
	resize_restore_screen();
} /* unix_redraw_display */


/**
 * Suspend ourselves.  Save the screen and raise SIGTSTP.
 * Upon continuing restore the screen as in unix_resize_display; the terminal
 * size may have changed while we were stopped.
 */
void unix_suspend_program(void)
{
	save_screen();
	raise(SIGTSTP);
	resize_restore_screen();
} /* unix_suspend_program */


/**
 * Repaint a window.
 *
 * This can only be called from resize_screen.  It copies part of the screen
 * as it was before the resize onto the current screen.  The source and
 * destination rectangles may start at different rows but the columns
 * are the same.  Positions are 1-based.  win should be the index
 * of the window that is being repainted.  If it equals the current window,
 * the saved cursor position adjusted by ypos_new - ypos_old is also restored.
 *
 * The copied rectangle is clipped to the saved window size.  Returns true
 * on success, false if anything went wrong.
 */
bool os_repaint_window(int win, int ypos_old, int ypos_new, int xpos,
                       int ysize, int xsize)
{
	int lines, cols;
	if (!saved_screen)
		return FALSE;
	if (xsize == 0 || ysize == 0)
		return TRUE;
	getmaxyx(saved_screen, lines, cols);
	ypos_old--, ypos_new--, xpos--;
	if (xpos + xsize > cols)
		xsize = cols - xpos;
	if (ypos_old + ysize > lines)
		ysize = lines - ypos_old;
	/* Most of the time we are in os_read_line, where the cursor
	 * position is different from that in the window properties.
	 * So use the real cursor position.
	 */
	if (win == cwin) {
		int y, x;
		getyx(saved_screen, y, x);
		y += ypos_new - ypos_old;
		if (y >= ypos_new && y< ypos_new + ysize
			&& x >= xpos && x < xpos + xsize)
			move(y, x);
	}
	if (xsize <= 0 || ysize <= 0)
		return FALSE;
	return copywin(saved_screen, stdscr, ypos_old, xpos, ypos_new, xpos,
		ypos_new + ysize - 1, xpos + xsize - 1, FALSE) != ERR;
} /* os_repaint_window */


/* this assumes RGBA with lsb = R */
/* Move to a static inline in ux_frotz.h later */
zlong RGB5ToTrue(zword w)
{
        int _r = w & 0x001F;
        int _g = (w & 0x03E0) >> 5;
        int _b = (w & 0x7C00) >> 10;
        _r = (_r << 3) | (_r >> 2);
        _g = (_g << 3) | (_g >> 2);
        _b = (_b << 3) | (_b >> 2);
        return (zlong) (_r | (_g << 8) | (_b << 16));
} /* RGB5ToTrue */


/* Move to a static inline in ux_frotz.h later */
zword TrueToRGB5(zlong u)
{
	return (zword) (((u >> 3) & 0x001f) | ((u >> 6) & 0x03e0) |
			((u >> 9) & 0x7c00));
} /* TrueToRGB5 */


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
} /* os_from_true_colour */


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
} /* os_to_true_colour */
