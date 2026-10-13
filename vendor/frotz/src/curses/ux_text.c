/*
 * ux_text.c - Unix interface, text functions
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

/* When color_enabled is FALSE, we still minimally keep track of colors by
 * setting current_color to A_REVERSE if the game reads the default
 * foreground and background colors and swaps them.  If we don't do this,
 * Strange Results can happen when playing certain V6 games when
 * color_enabled is FALSE.
 */
bool color_enabled = FALSE;

/* int current_color = 0; */

static char latin1_to_ascii[] =
	"   !  c  L  >o<Y  |  S  '' C  a  << not-  R  _  "
	"^0 +/-^2 ^3 '  my P  .  ,  ^1 o  >> 1/41/23/4?  "
	"A  A  A  A  Ae A  AE C  E  E  E  E  I  I  I  I  "
	"Th N  O  O  O  O  Oe *  O  U  U  U  Ue Y  Th ss "
	"a  a  a  a  ae a  ae c  e  e  e  e  i  i  i  i  "
	"th n  o  o  o  o  oe :  o  u  u  u  ue y  th y  ";


/*
 * os_font_data
 *
 * Return true if the given font is available. The font can be
 *
 *    TEXT_FONT
 *    PICTURE_FONT
 *    GRAPHICS_FONT
 *    FIXED_WIDTH_FONT
 *
 * The font size should be stored in "height" and "width".
 * If the given font is unavailable then these values must
 * _not_ be changed.
 *
 */
int os_font_data (int font, int *height, int *width)
{
	if (font == TEXT_FONT) {
		*height = 1;
		*width = 1;
		return 1; /* Truth in advertising */
	}
	return 0;
} /* os_font_data */


#ifdef COLOR_SUPPORT
/*
 * unix_convert
 *
 * Converts frotz's (and Infocom's) color values to ncurses color values.
 *
 */
static int unix_convert(int color)
{
	switch(color) {
		case BLACK_COLOUR: return COLOR_BLACK;
		case RED_COLOUR: return COLOR_RED;
		case GREEN_COLOUR: return COLOR_GREEN;
		case YELLOW_COLOUR: return COLOR_YELLOW;
		case BLUE_COLOUR: return COLOR_BLUE;
		case MAGENTA_COLOUR: return COLOR_MAGENTA;
		case CYAN_COLOUR: return COLOR_CYAN;
		case WHITE_COLOUR: return COLOR_WHITE;
	}
	return 0;
} /* unix_convert */
#endif


/*
 * os_set_colour
 *
 * Set the foreground and background colours which can be:
 *
 *     DEFAULT_COLOUR
 *     BLACK_COLOUR
 *     RED_COLOUR
 *     GREEN_COLOUR
 *     YELLOW_COLOUR
 *     BLUE_COLOUR
 *     MAGENTA_COLOUR
 *     CYAN_COLOUR
 *     WHITE_COLOUR
 *
 *     MS-DOS 320 columns MCGA mode only:
 *
 *     GREY_COLOUR
 *
 *     Amiga only:
 *
 *     LIGHTGREY_COLOUR
 *     MEDIUMGREY_COLOUR
 *     DARKGREY_COLOUR
 *
 * There may be more colours in the range from 16 to 255; see the
 * remarks on os_peek_colour.
 *
 */
void os_set_colour (int new_foreground, int new_background)
{
	if (new_foreground == 1) new_foreground = z_header.default_foreground;
	if (new_background == 1) new_background = z_header.default_background;
	if (u_setup.color_enabled) {
#ifdef COLOR_SUPPORT
		static int colorspace[10][10];
		static int n_colors = 0;

		if (!colorspace[new_foreground][new_background]) {
			init_pair(++n_colors, unix_convert(new_foreground),
				unix_convert(new_background));
			colorspace[new_foreground][new_background] = n_colors;
		}
		u_setup.current_color = COLOR_PAIR(colorspace[new_foreground][new_background]);
#endif
	} else
		u_setup.current_color = (((new_foreground == z_header.default_background)
			&& (new_background == z_header.default_foreground))
			? A_REVERSE : 0);
	os_set_text_style(u_setup.current_text_style);

} /* os_set_colour */


/*
 * os_get_text_style
 *
 * Return the current text style.  Following flags can be set:
 *
 *     REVERSE_STYLE
 *     BOLDFACE_STYLE
 *     EMPHASIS_STYLE (aka underline aka italics)
 *     FIXED_WIDTH_STYLE
 *
 */
int os_get_text_style (void)
{
	return u_setup.current_text_style;
} /* os_get_text_style */

/*
 * os_set_text_style
 *
 * Set the current text style.  Following flags can be set:
 *
 *     REVERSE_STYLE
 *     BOLDFACE_STYLE
 *     EMPHASIS_STYLE (aka underline aka italics)
 *     FIXED_WIDTH_STYLE
 *
 */
void os_set_text_style (int new_style)
{
	int temp = 0;

	u_setup.current_text_style = new_style;
	if (new_style & REVERSE_STYLE) temp |= A_REVERSE;
	if (new_style & BOLDFACE_STYLE) temp |= A_BOLD;
	if (new_style & EMPHASIS_STYLE) {
		switch(u_setup.emphasis_mode) {
		case EMPHASIS_ITALIC:
#ifdef ITALIC_SUPPORT
#if (NCURSES_VERSION_MAJOR >= 6)

			if (termattrs() & A_ITALIC) temp |= A_ITALIC;
			else temp |= A_UNDERLINE;
#endif
#else
			temp |= A_UNDERLINE;
#endif /* ITALICS_SUPPORT */
			break;
		case EMPHASIS_UNDERLINE:
			temp |= A_UNDERLINE;
			break;
		case EMPHASIS_NONE: /* do nothing */
		default:
			break;
		}
	}
	attrset(temp ^ u_setup.current_color);
} /* os_set_text_style */


/*
 * os_set_font
 *
 * Set the font for text output.  The interpreter takes care not to
 * choose fonts which aren't supported by the interface.
 *
 */
void os_set_font (int UNUSED(new_font))
{
	/* Not implemented */
} /* os_set_font */


/*
 * os_display_char
 *
 * Display a character of the current font using the current colours and
 * text style. The cursor moves to the next position. Printable codes are
 * all ASCII values from 32 to 126, ISO Latin-1 characters from 160 to
 * 255, ZC_GAP (gap between two sentences) and ZC_INDENT (paragraph
 * indentation). The screen should not be scrolled after printing to the
 * bottom right corner.
 *
 */
void os_display_char (zchar c)
{
	if (c >= ZC_LATIN1_MIN) {
		if (u_setup.plain_ascii) {
			char *ptr = latin1_to_ascii + 3 * (c - ZC_LATIN1_MIN);
			char c1 = *ptr++;
			char c2 = *ptr++;
			char c3 = *ptr++;
			addch(c1);

			if (c2 != ' ')
				addch(c2);
			if (c3 != ' ')
				addch(c3);

#ifndef USE_UTF8
		} else
			addch(c);
#else
		} else {
			if(c > 0x7ff) {
				addch(0xe0 | ((c >> 12) & 0xf));
				addch(0x80 | ((c >> 6) & 0x3f));
				addch(0x80 | (c & 0x3f));
			} else {
				addch(0xc0 | ((c >> 6) & 0x1f));
				addch(0x80 | (c & 0x3f));
			}
		}
#endif /* USE_UTF8 */
		return;
	}
	if (c >= ZC_ASCII_MIN && c <= ZC_ASCII_MAX) {
		addch(c);
		return;
	}
	if (c == ZC_INDENT) {
		addch(' '); addch(' '); addch(' ');
		return;
	}
	if (c == ZC_GAP) {
		addch(' '); addch(' ');
		return;
	}
} /* os_display_char */


/*
 * os_display_string
 *
 * Pass a string of characters to os_display_char.
 *
 */
void os_display_string (const zchar *s)
{
	zchar c;

	while ((c =  *s++) != 0) {
		if (c == ZC_NEW_FONT || c == ZC_NEW_STYLE) {
			int arg = (unsigned char) *s++;
			if (c == ZC_NEW_FONT)
				os_set_font (arg);
			if (c == ZC_NEW_STYLE)
				os_set_text_style (arg);
		} else
			os_display_char (c);
	}
} /* os_display_string */


/*
 * os_check_unicode
 *
 * Return with bit 0 set if the Unicode character can be
 * displayed, and bit 1 if it can be input.
 *
 */
int os_check_unicode(int UNUSED(font), zchar UNUSED(c))
{
	/* Assume full input and output.  */
	return 3;
} /* os_check_unicode */


/*
 * os_char_width
 *
 * Return the width of the character in screen units.
 *
 */
int os_char_width(zchar c)
{
	if (c >= ZC_LATIN1_MIN && u_setup.plain_ascii) {
		int width = 0;
		const char *ptr = latin1_to_ascii + 3 * (c - ZC_LATIN1_MIN);
		char c2, c3;

		ptr++;
		c2 = *ptr++;
		c3 = *ptr++;

		width++;
		if (c2 != ' ')
			width++;
		if (c3 != ' ')
			width++;
		return width;
	}
	return 1;
} /* os_char_width*/


/*
 * os_string_width
 *
 * Calculate the length of a word in screen units. Apart from letters,
 * the word may contain special codes:
 *
 *    NEW_STYLE - next character is a new text style
 *    NEW_FONT  - next character is a new font
 *
 */
int os_string_width(const zchar *s)
{
	int width = 0;
	zchar c;

	while ((c = *s++) != 0) {
		if (c == ZC_NEW_STYLE || c == ZC_NEW_FONT) {
			s++;
			/* No effect */
		} else
			width += os_char_width(c);
	}
	return width;
} /* os_string_width */


/*
 * os_set_cursor
 *
 * Place the text cursor at the given coordinates. Top left is (1,1).
 *
 */
void os_set_cursor(int y, int x)
{
	/* Curses thinks the top left is (0,0) */
	move(--y, --x);
} /* os_set_cursor */


/*
 * os_more_prompt
 *
 * Display a MORE prompt, wait for a keypress and remove the MORE
 * prompt from the screen.
 *
 */
void os_more_prompt(void)
{
	int saved_style, saved_x, saved_y;

	/* Save some useful information */
	saved_style = u_setup.current_text_style;
	getyx(stdscr, saved_y, saved_x);

	os_set_text_style(0);
	addstr("[MORE]");
	os_read_key(0, TRUE);

	move(saved_y, saved_x);
	addstr("      ");
	move(saved_y, saved_x);
	os_set_text_style(saved_style);
} /* os_more_prompt */
