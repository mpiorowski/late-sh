/*
 * x_text.c
 *
 * X interface, text functions
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

#include <stdio.h>
#include <stdlib.h>
#include <X11/Intrinsic.h>
#include "x_frotz.h"

int curr_x = 0;
int curr_y = 0;

unsigned long bg_pixel = 0, fg_pixel = 0, def_bg_pixel = 0, def_fg_pixel = 0;

static int char_width(const XFontStruct *, zchar);


/* Try to allocate the specified standard colour. */
static int x_alloc_colour(int index, unsigned long *result)
{
	char *str_type_return;
	char *class_buf, *name_buf, *colour_name;
	static char *fallback_names[] = {
		"black", "red3", "green3", "yellow3", "blue3", "magenta3",
		    "cyan3",
		"gray80", "gray50"
	};
	XColor screen, exact;
	int retval;
	XrmValue value;

	class_buf = (char *)malloc(strlen(x_class) + 9);
	name_buf = (char *)malloc(strlen(x_name) + 9);
	sprintf(class_buf, "%s.Color%d", x_class, index);
	sprintf(name_buf, "%s.color%d", x_name, index);
	if (XrmGetResource(XtDatabase(dpy), name_buf, class_buf,
			   &str_type_return, &value))
		colour_name = (char *)value.addr;
	else
		colour_name = fallback_names[index - 2];
	retval = XAllocNamedColor(dpy, DefaultColormap(dpy, DefaultScreen(dpy)),
				  colour_name, &screen, &exact);
	if (retval)
		*result = screen.pixel;
	free(class_buf);
	free(name_buf);
	return retval;
}


unsigned long pixel_values[17];

/* Process the given color name, and return an index corresponding to
   it */
static int x_bg_fg_color(unsigned long *pixel, int index, char *color_name,
			 int def_index)
{
	char extra;
	int z_num;
	XColor screen, exact;

	if (color_name == NULL) {
		*pixel = pixel_values[index] = pixel_values[def_index];
		return def_index;
	}
	if (sscanf(color_name, "z:%d%c", &z_num, &extra) == 1) {
		if (z_num < BLACK_COLOUR || z_num > GREY_COLOUR) {
			fprintf(stderr, "Invalid Z-machine color %d\n", z_num);
			os_quit(EXIT_FAILURE);
		}
		*pixel = pixel_values[index] = pixel_values[z_num];
		return z_num;
	}
	if (XAllocNamedColor(dpy, DefaultColormap(dpy, DefaultScreen(dpy)),
			     color_name, &screen, &exact)) {
		pixel_values[index] = screen.pixel;
		for (z_num = BLACK_COLOUR; z_num <= GREY_COLOUR; z_num++)
			if (pixel_values[z_num] == pixel_values[index])
				return z_num;
		return index;
	} else {
		fprintf(stderr, "Warning: could not allocate user color %s\n",
			color_name);
		*pixel = pixel_values[index] = pixel_values[def_index];
		return def_index;
	}
}


/* Try to allocate the standard Z machine colours, and in any case try
   to allocate the default background and foreground colours. */
void x_init_colour(char *bg_name, char *fg_name)
{
	int use_color, i;

	for (i = 0; i < 17; i++)
		pixel_values[i] = BlackPixel(dpy, DefaultScreen(dpy));
	if (DefaultDepth(dpy, DefaultScreen(dpy)) == 1)
		use_color = 0;
	else {
		for (i = BLACK_COLOUR; i <= GREY_COLOUR; i++) {
			use_color = x_alloc_colour(i, &pixel_values[i]);
			if (!use_color)
				break;
		}
		if (!use_color) {
			fprintf(stderr,
				"Warning: could not allocate standard colors\n");
			XFreeColors(dpy,
				    DefaultColormap(dpy, DefaultScreen(dpy)),
				    &pixel_values[BLACK_COLOUR],
				    i - BLACK_COLOUR, AllPlanes);
			for (i--; i >= BLACK_COLOUR; i--)
				pixel_values[i] =
				    BlackPixel(dpy, DefaultScreen(dpy));
		}
	}
	if (use_color) {
		z_header.config |= CONFIG_COLOUR;
		z_header.flags |= COLOUR_FLAG;
	} else {
		z_header.flags &= ~COLOUR_FLAG;
		pixel_values[WHITE_COLOUR] =
		    WhitePixel(dpy, DefaultScreen(dpy));
	}

	z_header.default_background =
	    x_bg_fg_color(&def_bg_pixel, 14, bg_name, 9);
	z_header.default_foreground =
	    x_bg_fg_color(&def_fg_pixel, 15, fg_name, 2);
}


char *font_names[9] = {
	"-adobe-times-medium-r-normal--14-*-*-*-*-*-iso8859-1",
	"-adobe-times-bold-r-normal--14-*-*-*-*-*-iso8859-1",
	"-adobe-times-medium-i-normal--14-*-*-*-*-*-iso8859-1",
	"-adobe-times-bold-i-normal--14-*-*-*-*-*-iso8859-1",
	"-adobe-courier-medium-r-normal--12-*-*-*-*-*-iso8859-1",
	"-adobe-courier-bold-r-normal--12-*-*-*-*-*-iso8859-1",
	"-adobe-courier-medium-o-normal--12-*-*-*-*-*-iso8859-1",
	"-adobe-courier-bold-o-normal--12-*-*-*-*-*-iso8859-1",
	"-*-Zork-*-*-*--13-*-*-*-*-*-*-*"
};


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
 * The font size should be stored in "height" and "width". If
 * the given font is unavailable then these values must _not_
 * be changed.
 *
 */
int os_font_data(int font, int *height, int *width)
{
	const XFontStruct *font_info;

	if (font != TEXT_FONT && font != GRAPHICS_FONT
	    && font != FIXED_WIDTH_FONT)
		return 0;
	font_info = get_font(font, 0);
	if (!font_info)
		return 0;
	*height = font_info->ascent + font_info->descent;
	*width = char_width(font_info, '0');
	return 1;
} /* os_font_data */


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
void os_set_colour(int new_foreground, int new_background)
{
	if (new_foreground == 1)
		new_foreground = z_header.default_foreground;
	if (new_background == 1)
		new_background = z_header.default_background;

	fg_pixel = pixel_values[new_foreground];
	bg_pixel = pixel_values[new_background];

	XSetForeground(dpy, normal_gc, fg_pixel);
	XSetBackground(dpy, normal_gc, bg_pixel);
	XSetForeground(dpy, reversed_gc, bg_pixel);
	XSetBackground(dpy, reversed_gc, fg_pixel);
	XSetForeground(dpy, cursor_gc, bg_pixel ^ fg_pixel);
} /* os_set_colour */


static XFontStruct *font_info_cache[9];

const XFontStruct *get_font(int font, int style)
{
	style = (style >> 1) & 7;	/* Ignore REVERSE_STYLE bit (= 1) */
	if (font == FIXED_WIDTH_FONT)
		style |= (FIXED_WIDTH_STYLE >> 1);
	if (font == GRAPHICS_FONT)
		style = 8;
	if (!font_info_cache[style])
		font_info_cache[style] = XLoadQueryFont(dpy, font_names[style]);
	return font_info_cache[style];
}

static int current_zfont = TEXT_FONT;
static int current_style = 0;

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
	return current_style;
}


/*
 * os_set_text_style
 *
 * Set the current text style. Following flags can be set:
 *
 *     REVERSE_STYLE
 *     BOLDFACE_STYLE
 *     EMPHASIS_STYLE (aka underline aka italics)
 *     FIXED_WIDTH_STYLE
 *
 */
void os_set_text_style(int new_style)
{
	if (new_style & REVERSE_STYLE)
		current_gc = reversed_gc;
	else
		current_gc = normal_gc;

	current_font_info = get_font(current_zfont, new_style);
	if (current_font_info == NULL) {
		fprintf(stderr, "Could not find font for style %d\n",
			new_style);
		current_font_info = get_font(current_zfont, 0);
	}
	XSetFont(dpy, current_gc, current_font_info->fid);

	current_style = new_style;
} /* os_set_text_style */


/*
 * os_set_font
 *
 * Set the font for text output. The interpreter takes care not to
 * choose fonts which aren't supported by the interface.
 *
 */
void os_set_font(int new_font)
{
	current_zfont = new_font;
	os_set_text_style(current_style);
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
void os_display_char(zchar c)
{
	switch (c) {
	case ZC_GAP:
		os_display_char(' ');
		c = ' ';
		break;
	case ZC_INDENT:
		os_display_char(' ');
		os_display_char(' ');
		c = ' ';
		break;
	}
	XDrawImageString(dpy, main_window, current_gc,
			 curr_x, curr_y + current_font_info->ascent, (char *)&c,
			 1);
	curr_x += os_char_width(c);
} /* os_display_char */


/* Delete the given character backwards from the current position --
   used to implement backspace in os_read_line */
void x_del_char(zchar c)
{
	int char_width = os_char_width(c);

	curr_x -= char_width;
	os_erase_area(curr_y + 1, curr_x + 1,
		      curr_y + current_font_info->ascent +
		      current_font_info->descent, curr_x + char_width, 0);
}


/*
 * os_display_string
 *
 * Pass a string of characters to os_display_char.
 *
 */
void os_display_string(const zchar * s)
{
	while (*s) {
		if (*s == ZC_NEW_FONT || *s == ZC_NEW_STYLE) {
			s++;
			if (s[-1] == ZC_NEW_FONT)
				os_set_font(*s++);
			else
				os_set_text_style(*s++);
		} else
			os_display_char(*s++);
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
	/* Assume full input and output. */
	return 3;
}


/*
 * os_char_width
 *
 * Return the width of the character in screen units.
 *
 */
static int char_width(const XFontStruct * font_info, zchar c)
{
	int direction, font_ascent, font_descent;
	XCharStruct size;

	XTextExtents((XFontStruct *) font_info, (char *)&c, 1, &direction,
		     &font_ascent, &font_descent, &size);
	return size.width;
}

int os_char_width(zchar c)
{
	return char_width(current_font_info, c);
} /* os_char_width */


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
int os_string_width(const zchar * s)
{
	int width = 0;

	while (*s) {
		if (*s == ZC_NEW_STYLE || *s == ZC_NEW_FONT)
			s += 2;
		else
			width += os_char_width(*s++);
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
	curr_y = y - 1;
	curr_x = x - 1;
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
	int saved_x, new_x;
	const char *p = "[MORE]";

	/* Save some useful information */
	saved_x = curr_x;

	/*  os_set_text_style(0); */
	while (*p)
		os_display_char((zchar) (*p++));
	os_read_key(0, TRUE);

	new_x = curr_x;
	curr_x = saved_x;
	os_erase_area(curr_y + 1, saved_x + 1,
		      curr_y + current_font_info->ascent +
		      current_font_info->descent, new_x + 1, 0);
	/*  os_set_text_style(saved_style); */
} /* os_more_prompt */
