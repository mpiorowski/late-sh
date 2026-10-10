/*
 * doutput.c - Dumb interface, output functions
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

#define DEFAULT_DUMB_COLOUR 31

extern f_setup_t f_setup;

static bool show_line_numbers = FALSE;
static bool show_line_types = FALSE;
static bool show_pictures = TRUE;
static bool visual_bell = TRUE;
static bool plain_ascii = FALSE;

static char latin1_to_ascii[] =
	"    !   c   L   >o< Y   |   S   ''  C   a   <<  not -   R   _   "
	"^0  +/- ^2  ^3  '   my  P   .   ,   ^1  o   >>  1/4 1/2 3/4 ?   "
	"A   A   A   A   Ae  A   AE  C   E   E   E   E   I   I   I   I   "
	"Th  N   O   O   O   O   Oe  *   O   U   U   U   Ue  Y   Th  ss  "
	"a   a   a   a   ae  a   ae  c   e   e   e   e   i   i   i   i   "
	"th  n   o   o   o   o   oe  :   o   u   u   u   ue  y   th  y   "
;

static char frotz_to_dumb [256];

static char* bbcode_colour[] = {
	"black", "red", "green", "yellow", "blue", "magenta",
	"cyan", "white", "lightgrey", "grey", "dimgrey"
};

/* z_header.screen_rows * z_header.screen_cols */
static int screen_cells;

/* The in-memory state of the screen.  */
/* Each cell contains a style in the lower byte and a zchar above. */

typedef struct cell_struct {
	int style;
	short fg;
	short bg;
	zchar c;
} cell_t;

static cell_t *screen_data;

/* A cell's style is REVERSE_STYLE, normal (0), or PICTURE_STYLE.
 * PICTURE_STYLE means the character is part of an ascii image outline
 * box.  (This just buys us the ability to turn box display on and off
 * with immediate effect.  No, not very useful, but I wanted to give
 * the rv bit some company in that huge byte I allocated for it.)  */

static int current_style = 0;
static char current_fg = DEFAULT_DUMB_COLOUR;
static char current_bg = DEFAULT_DUMB_COLOUR;

/* Which cells have changed (1 byte per cell).  */
static char *screen_changes;

static int cursor_row = 0, cursor_col = 0;

/* Compression styles.  */
static enum {
	COMPRESSION_NONE,
	COMPRESSION_SPANS,
	COMPRESSION_MAX,
} compression_mode = COMPRESSION_SPANS;
static char *compression_names[] = {"NONE", "SPANS", "MAX"};
static int hide_lines = 0;

/* Reverse-video display styles.  */
static enum {
        RV_NONE,
	RV_DOUBLESTRIKE,
	RV_UNDERLINE,
	RV_CAPS,
} rv_mode = RV_NONE;
static char *rv_names[] = {"NONE", "DOUBLESTRIKE", "UNDERLINE", "CAPS"};
static char rv_blank_str[5] = {' ', 0, 0, 0, 0};


/*
 * Local functions
 */
#ifdef USE_UTF8
static void zputchar(zchar);
void zputchar(zchar c)
{
	if(c > 0x7ff) {
		putchar(0xe0 | ((c >> 12) & 0xf));
		putchar(0x80 | ((c >> 6) & 0x3f));
		putchar(0x80 | (c & 0x3f));
	} else if(c > 0x7f) {
		putchar(0xc0 | ((c >> 6) & 0x1f));
		putchar(0x80 | (c & 0x3f));
	} else {
		putchar(c);
	}
} /* zputchar */
#else
#define zputchar(x) putchar(x)
#endif


/* if val is '0' or '1', set *var accordingly, else toggle it.  */
static void toggle(bool *var, char val)
{
	*var = val == '1' || (val != '0' && !*var);
} /* toggle */


/* Print a cell to stdout using IRC formatting codes.  */
#ifndef DISABLE_FORMATS
static void show_cell_irc(cell_t cel)
{
	static char lastfg   = DEFAULT_DUMB_COLOUR,
		    lastbg   = DEFAULT_DUMB_COLOUR,
		    lastbold = 0,
		    lastemph = 0;

	char	    fg	     = cel.fg,
		    bg	     = cel.bg;

	if (cel.style & REVERSE_STYLE) {
		fg = cel.bg;
		bg = cel.fg;
	}

	if (fg != lastfg || bg != lastbg) {
		putchar ('\017');	/* ^O cancels all text styles */
		lastbold = 0;
		lastemph = 0;

		if (fg != DEFAULT_DUMB_COLOUR) {
			if (bg != DEFAULT_DUMB_COLOUR)
				printf("\003%u,%s%u", (unsigned char) fg,
					((unsigned char) bg < 10) ? "0" : "", (unsigned char) bg);
			else
				printf("\003%s%u", ((unsigned char) fg < 10) ? "0" : "", (unsigned char) fg);
		}
	}

	if (cel.style & BOLDFACE_STYLE) {
		if (!lastbold)
			putchar('\002');
		lastbold = 1;
	} else {
		if (lastbold)
			putchar('\002');
		lastbold = 0;
	}

	if (cel.style & EMPHASIS_STYLE) {
		if (!lastemph)
			putchar('\037');
		lastemph = 1;
	} else {
		if (lastemph)
			putchar('\037');
		lastemph = 0;
	}

	if (cel.style & PICTURE_STYLE)
		zputchar(show_pictures ? cel.c : ' ');
	else
		zputchar(cel.c);

	lastfg = fg;
	lastbg = bg;
} /* show_cell_irc */


static void show_cell_ansi(cell_t cel)
{
	static char lastfg   = DEFAULT_DUMB_COLOUR,
		    lastbg   = DEFAULT_DUMB_COLOUR,
		    lastbold = 0,
		    lastemph = 0,
		    lastrev  = 0;

	char	    fg	     = cel.fg,
		    bg	     = cel.bg;

	if (cel.c == '\n') {
		printf("\033[0K\n");
		return;
	}

	if (fg == DEFAULT_DUMB_COLOUR)
		fg = frotz_to_dumb [z_header.default_foreground];
	if (bg == DEFAULT_DUMB_COLOUR)
		bg = frotz_to_dumb [z_header.default_background];

	if (fg != lastfg) {
		if (fg < 8)
			printf("\033[%dm", 30 + fg);
		else
			printf("\033[38;5;%dm", 232 + fg);
		lastfg = fg;
	}

	if (bg != lastbg) {
		if (bg < 8)
			printf("\033[%dm", 40 + bg);
		else
			printf("\033[48;5;%dm", 232 + bg);
		lastbg = bg;
	}

	if (cel.style & REVERSE_STYLE) {
		if (!lastrev)
			printf("\033[7m");
		lastrev = 1;
	} else {
		if (lastrev)
			printf("\033[27m");
		lastrev = 0;
	}

	if (cel.style & BOLDFACE_STYLE) {
		if (!lastbold)
			printf("\033[1m");
		lastbold = 1;
	} else {
		if (lastbold)
			printf("\033[22m");
		lastbold = 0;
	}

	if (cel.style & EMPHASIS_STYLE) {
		if (!lastemph)
			printf("\033[4m");
		lastemph = 1;
	} else {
		if (lastemph)
			printf("\033[24m");
		lastemph = 0;
	}

	if (cel.style & PICTURE_STYLE)
		zputchar(show_pictures ? cel.c : ' ');
	else
		zputchar(cel.c);
} /* show_cell_ansi */


static void show_cell_bbcode(cell_t cel)
{
	static char lastfg   = DEFAULT_DUMB_COLOUR,
		    lastbg   = DEFAULT_DUMB_COLOUR,
		    lastbold = 0,
		    lastemph = 0,
		    lastfix  = 0;
	char	    fg	     = cel.fg,
		    bg	     = cel.bg,
		    bold      = (cel.style & BOLDFACE_STYLE) ? 1 : 0,
		    emph      = (cel.style & EMPHASIS_STYLE) ? 1 : 0,
		    fix       = (cel.style & FIXED_WIDTH_STYLE) ? 1 : 0,
		    defaultfg = frotz_to_dumb [z_header.default_foreground],
		    defaultbg = frotz_to_dumb [z_header.default_background];

	if (fg == DEFAULT_DUMB_COLOUR)
		fg = defaultfg;
	if (bg == DEFAULT_DUMB_COLOUR)
		bg = defaultbg;
	if (lastfg == DEFAULT_DUMB_COLOUR)
		lastfg = defaultfg;
	if (lastbg == DEFAULT_DUMB_COLOUR)
		lastbg = defaultbg;

	if (cel.style & REVERSE_STYLE) {
		char tmp;
		tmp = fg;
		fg = bg;
		bg = tmp;
	}

	if (lastemph && (fix != lastfix || emph != lastemph || bold != lastbold
		|| bg != lastbg || fg != lastfg)) {
		printf("[/i]");
		lastemph = 0;
	}

	if (lastbold && (fix != lastfix || bold != lastbold || bg != lastbg
		|| fg != lastfg)) {
		printf("[/b]");
		lastbold = 0;
	}

	if (lastfg != defaultfg && (fix != lastfix || bg != lastbg
		|| fg != lastfg)) {
		printf("[/color]");
		lastfg = defaultfg;
	}

	if (lastbg != defaultbg && (fix != lastfix || bg != lastbg)) {
		printf("[/bgcolor]");
		lastbg = defaultbg;
	}

	if (lastfix && fix != lastfix) {
		printf("[/font]");
		lastfix = 0;
	}

	if (fix != lastfix) {
		printf("[font=monospace]");
		lastfix = 1;
	}

	if (bg != lastbg) {
		printf("[bgcolor=%s]", bbcode_colour[(short)bg]);
		lastbg = bg;
	}

	if (fg != lastfg) {
		printf("[color=%s]", bbcode_colour[(short)fg]);
		lastfg = fg;
	}

	if (bold != lastbold) {
		printf("[b]");
		lastbold = 1;
	}

	if (emph != lastemph) {
		printf("[i]");
		lastemph = 1;
	}

	if (cel.style & PICTURE_STYLE)
		zputchar(show_pictures ? cel.c : ' ');
	else if (strchr("[]\\`*|_#<>=-.+~&", cel.c))
		printf("\\%c", cel.c);
	else if (cel.c == ' ' && fix)
		printf("&nbsp;");
	else
		zputchar(cel.c);
} /* show_cell_bbcode */
#endif /* DISABLE_FORMATS */


/* Print a cell to stdout without using formatting codes.  */
static void show_cell_normal(cell_t cel)
{
	if (cel.style & REVERSE_STYLE) {
		if (cel.c == ' ')
			printf("%s", rv_blank_str);
		else {
			switch (rv_mode) {
			case RV_CAPS:
				if (cel.c <= 0x7f) {
					zputchar(toupper(cel.c));
					break;
				}
				/* fall through */
			case RV_NONE:
				zputchar(cel.c);
				break;
			case RV_UNDERLINE:
				putchar('_');
				putchar('\b');
				zputchar(cel.c);
				break;
			case RV_DOUBLESTRIKE:
				zputchar(cel.c);
				putchar('\b');
				zputchar(cel.c);
				break;
			}
		}
	}
	else if (cel.style & PICTURE_STYLE)
		zputchar(show_pictures ? cel.c : ' ');
	else	/* Only NORMAL_STYLE and FIXED_WIDTH_STYLE are left. */
		zputchar(cel.c);
} /* show_cell_normal */


static void show_cell(cell_t cel)
{
#ifndef DISABLE_FORMATS
	if (f_setup.format == FORMAT_IRC)
		show_cell_irc(cel);
	else if (f_setup.format == FORMAT_ANSI)
		show_cell_ansi(cel);
	else if (f_setup.format == FORMAT_BBCODE)
		show_cell_bbcode(cel);
	else
#endif
		show_cell_normal(cel);
} /* show_cell */


static bool will_print_blank(cell_t c)
{
#ifndef DISABLE_FORMATS
	if (f_setup.format != FORMAT_NORMAL)
		return FALSE;
#endif
	return (((c.style == PICTURE_STYLE) && !show_pictures)
		|| ((c.c == ' ')
		&& ((c.style != REVERSE_STYLE)
		|| (*rv_blank_str == ' '))));
} /* will_print_blank */


static cell_t make_cell(int style, short fg, short bg, zchar c)
{
	cell_t	cel;

	cel.style = style;
	cel.c = c;

	if (f_setup.format != FORMAT_NORMAL) {
		cel.bg = bg;
		cel.fg = fg;
	}
	return cel;
} /* make_cell */


static void show_line_prefix(int row, char c)
{
	if (show_line_numbers) {
		if (row == -1) {
			show_cell(make_cell(0, DEFAULT_DUMB_COLOUR, DEFAULT_DUMB_COLOUR, '.'));
			show_cell(make_cell(0, DEFAULT_DUMB_COLOUR, DEFAULT_DUMB_COLOUR, '.'));
		}
		else {
			char s[4];
			sprintf(s, "%02d", (row + 1) % 100);
			show_cell(make_cell(0, DEFAULT_DUMB_COLOUR, DEFAULT_DUMB_COLOUR, s[0]));
			show_cell(make_cell(0, DEFAULT_DUMB_COLOUR, DEFAULT_DUMB_COLOUR, s[1]));
		}
	}
	if (show_line_types)
		show_cell(make_cell(0, DEFAULT_DUMB_COLOUR, DEFAULT_DUMB_COLOUR, c));

	/* Add a separator char (unless there's nothing to separate).  */
	if (show_line_numbers || show_line_types)
		show_cell(make_cell(0, DEFAULT_DUMB_COLOUR, DEFAULT_DUMB_COLOUR, ' '));
} /* show_line_prefix */


static cell_t *dumb_row(int r)
{
	return screen_data + r * z_header.screen_cols;
} /* dumb_row */


/* Print a row to stdout.  */
static void show_row(int r)
{
	if (r == -1) {
		show_line_prefix(-1, '.');
	} else {
		int c, last;
		show_line_prefix(r, (r == cursor_row) ? ']' : ' ');
		/* Don't print spaces at end of line.  */
		/* (Saves bandwidth and printhead wear.)  */
		/* TODO: compress spaces to tabs.  */
		for (last = z_header.screen_cols - 1; last >= 0; last--) {
			if (!will_print_blank(dumb_row(r)[last]))
				break;
		}

		for (c = 0; c <= last; c++)
			show_cell(dumb_row(r)[c]);
	}
	show_cell(make_cell (0, DEFAULT_DUMB_COLOUR, DEFAULT_DUMB_COLOUR, '\n'));
} /* show_row */


static char *dumb_changes_row(int r)
{
	return screen_changes + r * z_header.screen_cols;
} /* dumb_changes_row */


/* Set a cell and update screen_changes.  */
static void dumb_set_cell(int row, int col, cell_t c)
{
	cell_t test;
	bool result = FALSE;

	test = dumb_row(row)[col];

	if (c.style  == test.style &&
		c.fg == test.fg    &&
		c.bg == test.bg    &&
		c.c  == test.c) {
		result = TRUE;
	}

	dumb_changes_row(row)[col] = (!result);
	dumb_row(row)[col] = c;
} /* dumb_set_cell */


/* put a character in the cell at the cursor and advance the cursor.  */
static void dumb_display_char(zchar c)
{
	dumb_set_cell(cursor_row, cursor_col, make_cell(current_style, current_fg, current_bg, c));
	if (++cursor_col == z_header.screen_cols) {
		if (cursor_row == z_header.screen_rows - 1)
			cursor_col--;
		else {
			cursor_row++;
			cursor_col = 0;
		}
	}
} /* dumb_display_char */


static void mark_all_unchanged(void)
{
	memset(screen_changes, 0, screen_cells);
} /* mark_all_unchanged */


/* Check if a cell is a blank or will display as one.
 * (Used to help decide if contents are worth printing.)  */
static bool is_blank(cell_t c)
{
	return ((c.c == ' ')
		|| ((c.style == PICTURE_STYLE) && !show_pictures));
} /* is_blank */


static void dumb_copy_cell(int dest_row, int dest_col,
			int src_row, int src_col)
{
	dumb_row(dest_row)[dest_col] = dumb_row(src_row)[src_col];
	dumb_changes_row(dest_row)[dest_col] = dumb_changes_row(src_row)[src_col];
} /* dumb_copy_cell */



/*
 * Public functions, mostly for the core.
 */
void os_display_char (zchar c)
{
	if (c >= ZC_LATIN1_MIN) {
		if (plain_ascii) {
			char *ptr = latin1_to_ascii + 4 * (c - ZC_LATIN1_MIN);
			do
				dumb_display_char(*ptr++);
			while (*ptr != ' ');
		} else
			dumb_display_char(c);
	} else if (c >= 32 && c <= 126) {
		dumb_display_char(c);
	} else if (c == ZC_GAP) {
		dumb_display_char(' ');
		dumb_display_char(' ');
	} else if (c == ZC_INDENT) {
		dumb_display_char(' ');
		dumb_display_char(' ');
		dumb_display_char(' ');
	}
	return;
} /* os_display_char */


/* Haxor your boxor? */
void os_display_string (const zchar *s)
{
	zchar c;

	while ((c = *s++) != 0) {
		if (c == ZC_NEW_FONT)
			s++;
		else if (c == ZC_NEW_STYLE)
			os_set_text_style(*s++);
		else {
			os_display_char (c);
		}
	}
} /* os_display_string */


void os_erase_area (int top, int left, int bottom, int right, int UNUSED (win))
{
	int row, col;
	top--; left--; bottom--; right--;
	for (row = top; row <= bottom; row++) {
		for (col = left; col <= right; col++)
			dumb_set_cell(row, col, make_cell(current_style, current_fg, current_bg, ' '));
	}
} /* os_erase_area */


void os_scroll_area (int top, int left, int bottom, int right, int units)
{
	int row, col;

	top--; left--; bottom--; right--;

	if (units > 0) {
		for (row = top; row <= bottom - units; row++) {
			for (col = left; col <= right; col++)
				dumb_copy_cell(row, col, row + units, col);
		}
		os_erase_area(bottom - units + 2, left + 1,
			bottom + 1, right + 1, -1 );
	} else if (units < 0) {
		for (row = bottom; row >= top - units; row--) {
			for (col = left; col <= right; col++)
				dumb_copy_cell(row, col, row + units, col);
		}
		os_erase_area(top + 1, left + 1, top - units, right + 1 , -1);
	}
} /* os_scroll_area */


int os_font_data(int font, int *height, int *width)
{
	if (font == TEXT_FONT) {
		*height = 1;
		*width = 1;
		return 1;
	}
	return 0;
} /* os_font_data */


void os_set_colour (int newfg, int newbg)
{
	current_fg = frotz_to_dumb[newfg];
	current_bg = frotz_to_dumb[newbg];
} /* os_set_colour */


void os_reset_screen(void)
{
	dumb_show_screen(FALSE);
} /* os_reset_screen */


void os_beep (int volume)
{
	if (visual_bell)
		printf("[%s-PITCHED BEEP]\n", (volume == 1) ? "HIGH" : "LOW");
	else
		putchar('\a'); /* so much for dumb.  */
} /* os_beep */


/* To make the common code happy */
void os_set_font (int UNUSED (x)) {}
void os_init_sound(void) {}
void os_prepare_sample (int UNUSED (a)) {}
void os_finish_with_sample (int UNUSED (a)) {}
void os_start_sample (int UNUSED (a), int UNUSED (b), int UNUSED (c), zword UNUSED (d)) {}
void os_stop_sample (int UNUSED (a)) {}

int os_check_unicode(int UNUSED (font), zchar UNUSED (c))
{
	/* Only UTF-8 output, no input yet.  */
	return 1;
} /* os_check_unicode */


int os_char_width (zchar z)
{
	if (plain_ascii && z >= ZC_LATIN1_MIN) {
		char *p = latin1_to_ascii + 4 * (z - ZC_LATIN1_MIN);
		return strchr(p, ' ') - p;
	}
	return 1;
} /* os_char_width */


int os_string_width (const zchar *s)
{
	int width = 0;
	zchar c;

	while ((c = *s++) != 0) {
		if (c == ZC_NEW_STYLE || c == ZC_NEW_FONT)
			s++;
		else
			width += os_char_width(c);
	}
	return width;
} /* os_string_width */


void os_set_cursor(int row, int col)
{
	cursor_row = row - 1; cursor_col = col - 1;
	if (cursor_row >= z_header.screen_rows)
		cursor_row = z_header.screen_rows - 1;
} /* os_set_cursor */


bool os_repaint_window(int UNUSED(win), int UNUSED(ypos_old),
			int UNUSED(ypos_new), int UNUSED(xpos),
			int UNUSED(ysize), int UNUSED(xsize))
{
	return FALSE;
} /* os_repaint_window */


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
int os_get_text_style(void)
{
	return current_style;
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
void os_set_text_style(int x)
{
	current_style = x;
} /* os_set_text_style */


/*
 * os_from_true_colour
 *
 * Given a true colour, return an appropriate colour index.
 *
 */
int os_from_true_colour(zword UNUSED (colour))
{
	/* Nothing here yet */
	return 0;
} /* os_from_true_colour */


/*
 * os_to_true_colour
 *
 * Given a colour index, return the appropriate true colour.
 *
 */
zword os_to_true_colour(int UNUSED (index))
{
	/* Nothing here yet */
	return 0;
} /* os_to_true_colour */


/*
 * Public functions just for the Dumb interface.
 */
void dumb_set_picture_cell(int row, int col, zchar c)
{
	dumb_set_cell(row, col, make_cell(current_style | PICTURE_STYLE, current_fg, current_bg, c));
} /* dumb_set_picture_cell */


void dumb_init_output(void)
{
#ifndef DISABLE_FORMATS
	if (f_setup.format == FORMAT_IRC) {
		setvbuf(stdout, 0, _IONBF, 0);
		setvbuf(stderr, 0, _IONBF, 0);

		z_header.config |= CONFIG_COLOUR | CONFIG_BOLDFACE | CONFIG_EMPHASIS;

		memset (frotz_to_dumb, 256, DEFAULT_DUMB_COLOUR);
		frotz_to_dumb [BLACK_COLOUR]   = 1;
		frotz_to_dumb [RED_COLOUR]     = 4;
		frotz_to_dumb [GREEN_COLOUR]   = 3;
		frotz_to_dumb [YELLOW_COLOUR]  = 8;
		frotz_to_dumb [BLUE_COLOUR]    = 12;
		frotz_to_dumb [MAGENTA_COLOUR] = 6;
		frotz_to_dumb [CYAN_COLOUR]    = 11;
		frotz_to_dumb [WHITE_COLOUR]   = 0;
		frotz_to_dumb [GREY_COLOUR]    = 14;

		z_header.default_foreground = WHITE_COLOUR;
		z_header.default_background = BLACK_COLOUR;
	} else if (f_setup.format == FORMAT_ANSI) {
		setvbuf(stdout, 0, _IONBF, 0);
		setvbuf(stderr, 0, _IONBF, 0);

		z_header.config |= CONFIG_COLOUR | CONFIG_BOLDFACE | CONFIG_EMPHASIS;

		memset (frotz_to_dumb, 256, DEFAULT_DUMB_COLOUR);
		frotz_to_dumb [BLACK_COLOUR]      = 0;
		frotz_to_dumb [RED_COLOUR]        = 1;
		frotz_to_dumb [GREEN_COLOUR]      = 2;
		frotz_to_dumb [YELLOW_COLOUR]     = 3;
		frotz_to_dumb [BLUE_COLOUR]       = 4;
		frotz_to_dumb [MAGENTA_COLOUR]    = 5;
		frotz_to_dumb [CYAN_COLOUR]       = 6;
		frotz_to_dumb [WHITE_COLOUR]      = 7;
		frotz_to_dumb [LIGHTGREY_COLOUR]  = 17;
		frotz_to_dumb [MEDIUMGREY_COLOUR] = 13;
		frotz_to_dumb [DARKGREY_COLOUR]   = 8;

		z_header.default_foreground = WHITE_COLOUR;
		z_header.default_background = BLACK_COLOUR;
	} else if (f_setup.format == FORMAT_BBCODE) {
		setvbuf(stdout, 0, _IONBF, 0);
		setvbuf(stderr, 0, _IONBF, 0);

		z_header.config |= CONFIG_COLOUR | CONFIG_BOLDFACE | CONFIG_EMPHASIS;

		memset (frotz_to_dumb, 256, DEFAULT_DUMB_COLOUR);
		frotz_to_dumb [BLACK_COLOUR]      = 0;
		frotz_to_dumb [RED_COLOUR]        = 1;
		frotz_to_dumb [GREEN_COLOUR]      = 2;
		frotz_to_dumb [YELLOW_COLOUR]     = 3;
		frotz_to_dumb [BLUE_COLOUR]       = 4;
		frotz_to_dumb [MAGENTA_COLOUR]    = 5;
		frotz_to_dumb [CYAN_COLOUR]       = 6;
		frotz_to_dumb [WHITE_COLOUR]      = 7;
		frotz_to_dumb [LIGHTGREY_COLOUR]  = 8;
		frotz_to_dumb [MEDIUMGREY_COLOUR] = 9;
		frotz_to_dumb [DARKGREY_COLOUR]   = 10;

		z_header.default_foreground = BLACK_COLOUR;
		z_header.default_background = WHITE_COLOUR;
	}
#endif /* DISABLE_FORMATS */
	if (z_header.version == V3) {
		z_header.config |= CONFIG_SPLITSCREEN;
		z_header.flags &= ~OLD_SOUND_FLAG;
	}

	if (z_header.version >= V5) {
		z_header.flags &= ~SOUND_FLAG;
	}

	z_header.screen_height = z_header.screen_rows;
	z_header.screen_width = z_header.screen_cols;
	screen_cells = z_header.screen_rows * z_header.screen_cols;

	z_header.font_width = 1; z_header.font_height = 1;

	screen_data = malloc(screen_cells * sizeof(cell_t));
	screen_changes = malloc(screen_cells);
	os_erase_area(1, 1, z_header.screen_rows, z_header.screen_cols, -2);
	memset(screen_changes, 0, screen_cells);
} /* dumb_init_output */


void dumb_display_user_input(char *s)
{
	/* copy to screen without marking it as a change.  */
	while (*s)
		dumb_row(cursor_row)[cursor_col++] = make_cell(0, DEFAULT_DUMB_COLOUR, DEFAULT_DUMB_COLOUR, *s++);
} /* dumb_display_user_input */


void dumb_discard_old_input(int num_chars)
{
	/* Weird discard stuff.  Grep spec for 'pain in my butt'.  */
	/* The old characters should be on the screen just before the cursor.
	 * Erase them.  */
	cursor_col -= num_chars;

	if (cursor_col < 0)
		cursor_col = 0;
	os_erase_area(cursor_row + 1, cursor_col + 1,
	cursor_row + 1, cursor_col + num_chars, -1);
} /* dumb_discard_old_input */


/* Print the part of the cursor row before the cursor.  */
void dumb_show_prompt(bool show_cursor, char line_type)
{
	int i;
	show_line_prefix(show_cursor ? cursor_row : -1, line_type);
	if (show_cursor) {
		for (i = 0; i < cursor_col; i++)
			show_cell(dumb_row(cursor_row)[i]);
	}
} /* dumb_show_prompt */


/* Show the current screen contents, or what's changed since the last
 * call.
 *
 * If compressing, and show_cursor is true, and the cursor is past the
 * last nonblank character on the last line that would be shown, then
 * don't show that line (because it will be redundant with the prompt
 * line just below it).
 */
void dumb_show_screen(bool show_cursor)
{
	int r, c, first, last;
	char changed_rows[0x100];

	/* Easy case */
	if (compression_mode == COMPRESSION_NONE) {
		for (r = hide_lines; r < z_header.screen_rows; r++)
			show_row(r);
		mark_all_unchanged();
		return;
	}

	/* Check which rows changed, and where the first and last change is. */
	first = last = -1;
	memset(changed_rows, 0, z_header.screen_rows);
	for (r = hide_lines; r < z_header.screen_rows; r++) {
		for (c = 0; c < z_header.screen_cols; c++) {
			if (dumb_changes_row(r)[c] && !is_blank(dumb_row(r)[c]))
				break;
		}

		changed_rows[r] = (c != z_header.screen_cols);
		if (changed_rows[r]) {
			first = (first != -1) ? first : r;
			last = r;
		}
	}

	if (first == -1)
		return;

	/* The show_cursor rule described above */
	if (show_cursor && (cursor_row == last)) {
		for (c = cursor_col; c < z_header.screen_cols; c++) {
			if (!is_blank(dumb_row(last)[c]))
				break;
		}
		if (c == z_header.screen_cols)
			last--;
	}

	/* Display the appropriate rows.  */
	if (compression_mode == COMPRESSION_MAX) {
		for (r = first; r <= last; r++) {
			if (changed_rows[r])
				show_row(r);
		}
	} else {
		/* COMPRESSION_SPANS */
		for (r = first; r <= last; r++) {
			if (changed_rows[r] || changed_rows[r + 1])
				show_row(r);
			else {
				while (!changed_rows[r + 1])
					r++;
				show_row(-1);
			}
		}
		if (show_cursor && (cursor_row > last + 1))
			show_row((cursor_row == last + 2) ? (last + 1) : -1);
	}
	mark_all_unchanged();
} /* dumb_show_screen */


/* Unconditionally show whole screen.  For \s user command.  */
void dumb_dump_screen(void)
{
	int r;
	for (r = 0; r < z_header.screen_height; r++)
		show_row(r);
} /* dumb_dump_screen */


/* Called when it's time for a more prompt but user has them turned off.  */
void dumb_elide_more_prompt(void)
{
	dumb_show_screen(FALSE);
	if (compression_mode == COMPRESSION_SPANS && hide_lines == 0) {
		show_row(-1);
	}
} /* dumb_elide_more_prompt */


bool dumb_output_handle_setting(const char *setting, bool show_cursor,
				bool startup)
{
	char *p;
	int i;
#ifdef USE_UTF8
	unsigned char *q;
#endif

	if (!strncmp(setting, "pb", 2)) {
		toggle(&show_pictures, setting[2]);
		printf("Picture outlines display %s\n", show_pictures ? "ON" : "OFF");
		if (startup)
			return TRUE;
		for (i = 0; i < screen_cells; i++)
			screen_changes[i] = (screen_data[i].style == PICTURE_STYLE);
		dumb_show_screen(show_cursor);
	} else if (!strncmp(setting, "vb", 2)) {
		toggle(&visual_bell, setting[2]);
		printf("Visual bell %s\n", visual_bell ? "ON" : "OFF");
		os_beep(1); os_beep(2);
	} else if (!strncmp(setting, "ln", 2)) {
		toggle(&show_line_numbers, setting[2]);
		printf("Line numbering %s\n", show_line_numbers ? "ON" : "OFF");
	} else if (!strncmp(setting, "lt", 2)) {
		toggle(&show_line_types, setting[2]);
		printf("Line-type display %s\n", show_line_types ? "ON" : "OFF");
	} else if (*setting == 'c') {
		switch (setting[1]) {
			case 'm': compression_mode = COMPRESSION_MAX; break;
			case 's': compression_mode = COMPRESSION_SPANS; break;
			case 'n': compression_mode = COMPRESSION_NONE; break;
			case 'h': hide_lines = atoi(&setting[2]); break;
			default: return FALSE;
		}
		printf("Compression mode %s, hiding top %d lines\n",
		compression_names[compression_mode], hide_lines);
	} else if (*setting == 'r') {
		switch (setting[1]) {
		case 'n': rv_mode = RV_NONE; break;
		case 'o': rv_mode = RV_DOUBLESTRIKE; break;
		case 'u': rv_mode = RV_UNDERLINE; break;
		case 'c': rv_mode = RV_CAPS; break;
		case 'b': strncpy(rv_blank_str, setting[2] ? &setting[2] : " ", 4); break;
		default: return FALSE;
		}
#ifdef USE_UTF8
		for (q = (unsigned char *)&rv_blank_str[1]; *q; q++)
			if (*q < 0x80 || *q >= 0xc0 || (unsigned char)rv_blank_str[0] < 0x80)
				*q = 0;
#else
		rv_blank_str[1] = 0;
#endif
		printf("Reverse-video mode %s, blanks reverse to '%s': ",
			rv_names[rv_mode], rv_blank_str);

		for (p = "sample reverse text"; *p; p++)
			show_cell(make_cell(REVERSE_STYLE, DEFAULT_DUMB_COLOUR, DEFAULT_DUMB_COLOUR, *p));
		putchar('\n');
		for (i = 0; i < screen_cells; i++)
			screen_changes[i] = (screen_data[i].style == REVERSE_STYLE);
		dumb_show_screen(show_cursor);
	} else if (!strcmp(setting, "set")) {
		printf("Compression Mode %s, hiding top %d lines\n",
			compression_names[compression_mode], hide_lines);
		printf("Picture Boxes display %s\n", show_pictures ? "ON" : "OFF");
		printf("Visual Bell %s\n", visual_bell ? "ON" : "OFF");
		os_beep(1); os_beep(2);
		printf("Line Numbering %s\n", show_line_numbers ? "ON" : "OFF");
		printf("Line-Type display %s\n", show_line_types ? "ON" : "OFF");
		printf("Reverse-Video mode %s, Blanks reverse to '%s': ",
			rv_names[rv_mode], rv_blank_str);
		for (p = "sample reverse text"; *p; p++)
			show_cell(make_cell(REVERSE_STYLE, DEFAULT_DUMB_COLOUR, DEFAULT_DUMB_COLOUR, *p));
		putchar('\n');
	} else
		return FALSE;
	return TRUE;
} /* dumb_output_handle_setting */


