/*
 * dosinit.c - DOS interface, initialization
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

#include <conio.h>
#include <dos.h>
#include <stdio.h>
#include <stdarg.h>
#include <stdlib.h>
#include <string.h>
#include "frotz.h"

#include "dosfrotz.h"
#ifndef NO_BLORB
#include "dosblorb.h"
#endif

extern f_setup_t f_setup;
extern z_header_t z_header;

#define INFORMATION "\
An interpreter for all Infocom and other Z-Machine games.\n\
Complies with standard 1.0 of Graham Nelson's specification.\n\
\n\
Syntax: frotz [options] story-file\n\
  -a   watch attribute setting    \t -O   watch object locating\n\
  -A   watch attribute testing    \t -p   alter piracy opcode\n\
  -b # background colour          \t -q   quiet (disable sound effects)\n\
  -B # reverse background colour  \t -r # right margin\n\
  -c # context lines              \t -R <path> restricted read/write\n\
  -d # display mode (see below)   \t -s # random number seed value\n\
  -e # emphasis colour [mode 1]   \t -S # transcript width\n\
  -f # foreground colour          \t -t   set Tandy bit\n\
  -F # reverse foreground colour  \t -T   bold typing [modes 2+4+5]\n\
  -g # font [mode 5] (see below)  \t -u # slots for multiple undo\n\
  -h # screen height              \t -v   show version information\n\
  -i   ignore runtime errors      \t -w # screen width\n\
  -l # left margin                \t -x   expand abbreviations g/x/z\n\
  -o   watch object movement      \t -Z # error checking (see below)\n"

#define INFO2 "\
Fonts: 0 fixed, 1 sans serif, 2 comic, 3 times, 4 serif.\n\
Display modes:  0 mono, 1 text, 2 CGA, 3 MCGA, 4 EGA, 5 Amiga.\n\
Error reporting: 0 none, 1 first only (default), 2 all, 3 exit after any error."


/* in bcinit.c only.  What is its significance? */
extern unsigned cdecl _heaplen = 0x800 + 4 * BUFSIZ;
extern unsigned cdecl _stklen = 0x800;

extern int zoptind;

static const char *progname = NULL;

extern char script_name[];
extern char command_name[];
extern char save_name[];
extern char auxiliary_name[];

int display = -1;

int user_background = -1;
int user_foreground = -1;
int user_emphasis = -1;
int user_bold_typing = -1;
int user_reverse_bg = -1;
int user_reverse_fg = -1;
int user_screen_height = -1;
int user_screen_width = -1;
int user_random_seed = -1;
int user_font = 1;

static byte old_video_mode = 0;

static void interrupt(*oldvect) () = NULL;

bool at_keybrd;

/* Test for the existance of Enhanced AT keyboard support in BIOS */
static bool test_enhanced_keyboard(unsigned char b)
{
	union REGS regs;
	unsigned char far *shift_status;
	shift_status = MK_FP(0, 0x417);
	shift_status[0] = b;
	regs.h.ah = 0x12;
	int86(0x16, &regs, &regs);
	return b == regs.h.al;
} /* test_enhanced_keyboard */


/*
 * os_init_setup
 *
 * Set or reset various configuration variables.
 *
 */
void os_init_setup(void)
{
	f_setup.sound = 1;

	at_keybrd = (test_enhanced_keyboard(0x40) && test_enhanced_keyboard(0x80));
	test_enhanced_keyboard(0x00);
} /* os_init_setup */


/*
 * dectoi
 *
 * Convert a string containing a decimal number to integer. The string may
 * be NULL, but it must not be empty.
 *
 */
int dectoi(const char *s)
{
	int n = 0;

	if (s != NULL) {
		do {
			n = 10 * n + (*s & 15);
		} while (*++s > ' ');
	}
	return n;
} /* dectoi */


/*
 * hextoi
 *
 * Convert a string containing a hex number to integer. The string may be
 * NULL, but it must not be empty.
 *
 */
int hextoi(const char *s)
{
	int n = 0;

	if (s != NULL) {
		do {

			n = 16 * n + (*s & 15);

			if (*s > '9')
				n += 9;

		} while (*++s > ' ');
	}
	return n;
} /* hextoi */


/*
 * basename
 *
 * A generic and spartan bit of code to extract the filename from a path.
 * This one is so trivial that it's okay to steal.
 */
char *basename(const char *path)
{
	const char *s;
	const char *p;
	p = s = path;

	while (*s) {
		if (*s++ == '\\') {
			p = s;
		}
	}
	return (char *)p;
} /* basename */


/*
 * cleanup
 *
 * Shut down the IO interface: free memory, close files, restore
 * interrupt pointers and return to the previous video mode.
 *
 */
static void cleanup(void)
{
#ifndef NO_SOUND
	dos_reset_sound();
#endif
	reset_pictures();

#ifdef __WATCOMC__
	bios_video_ax((word)old_video_mode);
	_dos_setvect(0x1b, oldvect);
#else
	asm mov ah, 0
	asm mov al, old_video_mode
	asm int 0x10
	setvect(0x1b, oldvect);
#endif
} /* cleanup */


/*
 * fast_exit
 *
 * Handler routine to be called when the crtl-break key is pressed.
 *
 */
static void interrupt fast_exit()
{
	cleanup();
	exit(EXIT_FAILURE);
} /* fast_exit */


/*
 * os_quit
 *
 * Immediately and cleanly exit, passing along exit status.
 *
 */

void os_quit(int status)
{
    cleanup();
    exit(status);
} /* os_quit */


static void print_c_string (const char *s)
{
	zchar c;

	while ((c = *s++) != 0)
		os_display_char (c);
} /* print_c_string */


/*
 * os_warn
 *
 * Display a warning message and continue with the game.
 *
 */
void os_warn (const char *s, ...)
{
	va_list m;
	char errorstring[81];
	int style;

	va_start(m, s);
	vsprintf(errorstring, s, m);
	va_end(m);

	os_beep(BEEP_HIGH);
	style = os_get_text_style();
	os_set_text_style(BOLDFACE_STYLE);
	print_c_string("Warning: ");
	os_set_text_style(NORMAL_STYLE);
	print_c_string(errorstring);
	new_line();
	os_set_text_style(style);

	return;
} /* os_warn */


/*
 * os_fatal
 *
 * Display error message and exit program.
 *
 */
void os_fatal(const char *s, ...)
{
	if (z_header.interpreter_number)
		os_reset_screen();

	/* Display error message */
	fputs("\nFatal error: ", stderr);
	fputs(s, stderr);
	fputs("\n", stderr);

	/* Abort program */
	exit(EXIT_FAILURE);
} /* os_fatal */


/*
 * parse_options
 *
 * Parse program options and set global flags accordingly.
 *
 */
static void parse_options(int argc, char **argv)
{
	int c;

	do {
		int num = 0;

		c = zgetopt(argc, argv,
			   "aAb:B:c:d:e:f:F:g:h:il:oOpqr:R:s:S:tTu:vw:xZ:");

		if (zoptarg != NULL)
			num = dectoi(zoptarg);

		if (c == 'a')
			f_setup.attribute_assignment = 1;
		if (c == 'A')
			f_setup.attribute_testing = 1;
		if (c == 'b')
			user_background = num;
		if (c == 'B')
			user_reverse_bg = num;
		if (c == 'c')
			f_setup.context_lines = num;
		if (c == 'd') {
			display = zoptarg[0] | 32;
			if ((display < '0' || display > '5')
			    && (display < 'a' || display > 'e')) {
				display = -1;
			}
		}
		if (c == 'e')
			user_emphasis = num;
		if (c == 'T')
			user_bold_typing = 1;
		if (c == 'f')
			user_foreground = num;
		if (c == 'F')
			user_reverse_fg = num;
		if (c == 'g') {
			if (num >= 0 && num <= 4)
				user_font = num;
		}
		if (c == 'h')
			user_screen_height = num;
		if (c == 'i')
			f_setup.ignore_errors = 1;
		if (c == 'l')
			f_setup.left_margin = num;
		if (c == 'o')
			f_setup.object_movement = 1;
		if (c == 'O')
			f_setup.object_locating = 1;
		if (c == 'p')
			f_setup.piracy = 1;
		if (c == 'q')
			f_setup.quiet = 1;
		if (c == 'r')
			f_setup.right_margin = num;
		if (c == 'R')
			f_setup.restricted_path = strdup(zoptarg);
		if (c == 's')
			user_random_seed = num;
		if (c == 'S')
			f_setup.script_cols = num;
		if (c == 't')
			f_setup.tandy = 1;
		if (c == 'u')
			f_setup.undo_slots = num;
		if (c == 'v') {
			printf("FROTZ V%s - MSDOS / PCDOS Edition\n", VERSION);
			printf("Commit date:    %s\n", GIT_DATE);
			printf("Git commit:     %s\n", GIT_HASH);
			printf("Notes:          %s\n", RELEASE_NOTES);
			printf("  Frotz was originally written by Stefan Jokisch.\n");
			printf("  It complies with standard 1.1 of the Z-Machine Standard.\n");
			printf("  It was ported to Unix by Galen Hazelwood.\n");
			printf("  The core and DOS port are maintained by David Griffith.\n");
			printf("  Frotz's homepage is https://661.org/proj/if/frotz/\n");
			exit(0);
		}
		if (c == 'w')
			user_screen_width = num;
		if (c == 'x')
			f_setup.expand_abbreviations = 1;
		if (c == 'Z') {
			if (num >= ERR_REPORT_NEVER && num <= ERR_REPORT_FATAL)
				f_setup.err_report_mode = num;
		}
		if (c == '?')
			zoptind = argc;
	} while (c != EOF && c != '?');

} /* parse_options */


static char *malloc_filename(char *story_name, char *extension)
{
	int length = strlen(story_name) + strlen(extension) + 2;
	char *filename = malloc(length);
	if (filename) {
		strcpy(filename, story_name);
		strcat(filename, extension);
	}
	return filename;
} /* malloc_filename */


/*
 * os_process_arguments
 *
 * Handle command line switches. Some variables may be set to activate
 * special features of Frotz:
 *
 *     option_attribute_assignment
 *     option_attribute_testing
 *     option_context_lines
 *     option_object_locating
 *     option_object_movement
 *     option_left_margin
 *     option_right_margin
 *     option_ignore_errors
 *     option_piracy
 *     option_undo_slots
 *     option_expand_abbreviations
 *     option_script_cols
 *
 * The global pointer "story_name" is set to the story file name.
 *
 */
void os_process_arguments(int argc, char *argv[])
{
	const char *p;
	int i;
	char stripped_story_name[10];

	/* Parse command line options */
	parse_options(argc, argv);

	if (zoptind != argc - 1) {
		printf("FROTZ V%s - MSDOS / PCDOS Edition.  ", VERSION);
#ifndef NO_SOUND
		printf("Audio output enabled.\n");
#else
		printf("Audio output disabled.\n");
#endif
		puts(INFORMATION);
		puts(INFO2);
		exit(EXIT_SUCCESS);
	}

	/* Set the story file name */
	f_setup.story_file = strdup(argv[zoptind]);

	/* Strip path and extension off the story file name */
	p = strdup(f_setup.story_file);

	for (i = 0; f_setup.story_file[i] != 0; i++)
		if (f_setup.story_file[i] == '\\'
		    || f_setup.story_file[i] == '/'
		    || f_setup.story_file[i] == ':')
			p = f_setup.story_file + i + 1;

	for (i = 0; p[i] != 0 && p[i] != '.'; i++)
		stripped_story_name[i] = p[i];
	stripped_story_name[i] = 0;
	f_setup.story_name = strdup(stripped_story_name);

	/* Create nice default file names */
	f_setup.script_name = malloc_filename(f_setup.story_name, EXT_SCRIPT);
	f_setup.command_name = malloc_filename(f_setup.story_name, EXT_COMMAND);
	f_setup.save_name = malloc_filename(f_setup.story_name, EXT_SAVE);
	f_setup.aux_name = malloc_filename(f_setup.story_name, EXT_AUX);

	/* Save the executable file name */
	progname = argv[0];
} /* os_process_arguments */


/*
 * standard_palette
 *
 * Set palette registers to EGA default values and call VGA BIOS to
 * use DAC registers #0 to #63.
 *
 */
static void standard_palette(void)
{

	static byte palette[] = {
		0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x14, 0x07,
		0x38, 0x39, 0x3a, 0x3b, 0x3c, 0x3d, 0x3e, 0x3f,
		0x00	/* last one is the overscan register */
	};

#ifdef __WATCOMC__
	if (display == _AMIGA_) {
		bios_video_ax_esdx_bh(0x1002, palette, 0);
		bios_video_ax_bx(0x1013, 0x0001);
	}
#else
	if (display == _AMIGA_) {
		asm mov ax, 0x1002
		asm lea dx, palette
		asm push ds
		asm pop es
		asm int 0x10
		asm mov ax, 0x1013
		asm mov bx, 0x0001
		asm int 0x10
	}
#endif
} /* standard_palette */


/*
 * special_palette
 *
 * Set palette register #i to value i and call VGA BIOS to use DAC
 * registers #64 to #127.
 *
 */
static void special_palette(void)
{

	static byte palette[] = {
		0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07,
		0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f,
		0x00	/* last one is the overscan register */
	};

#ifdef __WATCOMC__
	if (display == _AMIGA_) {
		bios_video_ax_esdx_bh(0x1002, palette, 0);
		bios_video_ax_bx(0x1013, 0x0101);
	}
#else
	if (display == _AMIGA_) {
		asm mov ax, 0x1002
		asm mov dx, offset palette
		asm push ds
		asm pop es
		asm int 0x10
		asm mov ax, 0x1013
		asm mov bx, 0x0101
		asm int 0x10
	}
#endif
} /* special_palette */


/*
 * os_init_screen
 *
 * Initialise the IO interface. Prepare the screen and other devices
 * (mouse, sound board). Set various OS depending story file header
 * entries:
 *
 *     z_header.config (aka flags 1)
 *     z_header.flags (aka flags 2)
 *     z_header.screen_cols (aka screen width in characters)
 *     z_header.screen_rows (aka screen height in lines)
 *     z_header.screen_width
 *     z_header.screen_height
 *     z_header.font_height (defaults to 1)
 *     z_header.font_width (defaults to 1)
 *     z_header.default_foreground
 *     z_header.default_background
 *     z_header.interpreter_number
 *     z_header.interpreter_version
 *     z_header.user_name (optional; not used by any game)
 *
 * Finally, set reserve_mem to the amount of memory (in bytes) that
 * should not be used for multiple undo and reserved for later use.
 *
 */
void os_init_screen(void)
{
	static byte zcolour[] = {
		BLACK_COLOUR,
		BLUE_COLOUR,
		GREEN_COLOUR,
		CYAN_COLOUR,
		RED_COLOUR,
		MAGENTA_COLOUR,
		BROWN + 16,
		LIGHTGRAY + 16,
		GREY_COLOUR,
		LIGHTBLUE + 16,
		LIGHTGREEN + 16,
		LIGHTCYAN + 16,
		LIGHTRED + 16,
		LIGHTMAGENTA + 16,
		YELLOW_COLOUR,
		WHITE_COLOUR
	};

	static struct {	/* information on modes 0 to 5 */
		byte vmode;
		word width;
		word height;
		byte font_width;
		byte font_height;
		byte fg;
		byte bg;
	} info[] = {
		{0x07, 80, 25, 1, 1, LIGHTGRAY + 16, BLACK_COLOUR},	/* MONO  */
		{0x03, 80, 25, 1, 1, LIGHTGRAY + 16, BLUE_COLOUR},	/* TEXT  */
		{0x06, 640, 200, 8, 8, WHITE_COLOUR, BLACK_COLOUR},	/* CGA   */
		{0x13, 320, 200, 5, 8, WHITE_COLOUR, GREY_COLOUR},	/* MCGA  */
		{0x0e, 640, 200, 8, 8, WHITE_COLOUR, BLUE_COLOUR},	/* EGA   */
		{0x12, 640, 400, 8, 16, WHITE_COLOUR, BLACK_COLOUR}	/* AMIGA */
	};

	static struct {	/* information on modes A to E */
		word vesamode;
		word width;
		word height;
	} subinfo[] = {
		{0x001, 40, 25},
		{0x109, 132, 25},
		{0x10b, 132, 50},
		{0x108, 80, 60},
		{0x10c, 132, 60}
	};

	int subdisplay;

	/* Get the current video mode. This video mode will be selected
	   when the program terminates. It's also useful to auto-detect
	   monochrome boards. */

#ifdef __WATCOMC__
	old_video_mode = (byte)bios_video_ah(0x0f);
#else
	asm mov ah, 15
	asm int 0x10
	asm mov old_video_mode, al
#endif

	/* If the display mode has not already been set by the user then see
	   if this is a monochrome board. If so, set the display mode to 0.
	   Otherwise check the graphics flag of the story. Select a graphic
	   mode if it is set or if this is a V6 game. Select text mode if it
	   is not. */
	 if (display == -1) {
		if (old_video_mode == 7)
			display = '0';
		else if (z_header.version == V6 || (z_header.flags & GRAPHICS_FLAG))
			display = '5';
		else
			display = '1';
	}

	/* Activate the desired display mode. All VESA text modes are very
	   similar to the standard text mode; in fact, only here we need to
	   know which VESA mode is used. */
	if (display >= '0' && display <= '5') {
		subdisplay = -1;
		display -= '0';
#ifdef __WATCOMC__
		bios_video_ax((word)info[display].vmode);
#else
		_AL = info[display].vmode;
		_AH = 0;
#endif
	} else if (display == 'a') {
		subdisplay = 0;
		display = 1;
#ifdef __WATCOMC__
		bios_video_ax(0x0001);
#else
		_AL = 0x01;
		_AH = 0;
#endif
	} else if (display >= 'b' && display <= 'e') {
		subdisplay = display - 'a';
		display = 1;
#ifdef __WATCOMC__
		bios_video_ax_bx(0x4f02, subinfo[subdisplay].vesamode);
#else
		_BX = subinfo[subdisplay].vesamode;
		_AX = 0x4f02;
#endif
	}

#ifndef __WATCOMC__
	geninterrupt(0x10);
#endif
	load_fonts();

	/* Make various preparations */
	if (display <= _TEXT_) {

#ifdef __WATCOMC__
		/* Enable bright background colours */
		bios_video_ax_bl(0x1003, 0);
		/* Turn off hardware cursor */
		bios_video_ah_cx(1, 0xffff);
#else
		/* Enable bright background colours */
		asm mov ax, 0x1003
		asm mov bl, 0
		asm int 0x10
		/* Turn off hardware cursor */
		asm mov ah, 1
		asm mov cx, 0xffff
		asm int 0x10
#endif
	} else {
		if (display == _AMIGA_) {
			scaler = 2;

			/* Use resolution 640 x 400 instead of 640 x 480.
			   BIOS doesn't help us here since this is not a
			   standard resolution. */

			outportb(0x03c2, 0x63);
			outport(0x03d4, 0x0e11);
			outport(0x03d4, 0xbf06);
			outport(0x03d4, 0x1f07);
			outport(0x03d4, 0x9c10);
			outport(0x03d4, 0x8f12);
			outport(0x03d4, 0x9615);
			outport(0x03d4, 0xb916);
		}

	}
#if !defined(__SMALL__) && !defined (__TINY__) && !defined (__MEDIUM__)
	/* Set the amount of memory to reserve for later use. It takes
	   some memory to open command, script and game files. If Frotz
	   is compiled in a small memory model then memory for opening
	   files is allocated on the "near heap" while other allocations
	   are made on the "far heap", i.e. we need not reserve memory
	   in this case. */ reserve_mem = 4 * BUFSIZ;

#endif

	/* Amiga emulation under V6 needs special preparation. */
	if (display == _AMIGA_ && z_header.version == V6) {
		user_reverse_fg = -1;
		user_reverse_bg = -1;
		zcolour[LIGHTGRAY] = LIGHTGREY_COLOUR;
		zcolour[DARKGRAY] = DARKGREY_COLOUR;

		special_palette();
	}

	/* Set various bits in the configuration byte. These bits tell
	   the game which features are supported by the interpreter. */
	if (z_header.version == V3 && f_setup.tandy != -1)
		z_header.config |= CONFIG_TANDY;
	if (z_header.version == V3)
		z_header.config |= CONFIG_SPLITSCREEN;
	if (z_header.version == V3
	    && (display == _MCGA_ || (display == _AMIGA_ && user_font != 0)))
		z_header.config |= CONFIG_PROPORTIONAL;
	if (z_header.version >= V4 && display != _MCGA_
	    && (user_bold_typing != -1 || display <= _TEXT_))
		z_header.config |= CONFIG_BOLDFACE;
	if (z_header.version >= V4)
		z_header.config |= CONFIG_EMPHASIS | CONFIG_FIXED | CONFIG_TIMEDINPUT;
	if (z_header.version >= V5 && display != _MONO_ && display != _CGA_)
		z_header.config |= CONFIG_COLOUR;
	if (z_header.version >= V5 && display >= _CGA_ && init_pictures())
		z_header.config |= CONFIG_PICTURES;

	/* Handle various game flags. These flags are set if the game wants
	   to use certain features. The flags must be cleared if the feature
	   is not available. */
	if (z_header.flags & GRAPHICS_FLAG)
		if (display <= _TEXT_)
			z_header.flags &= ~GRAPHICS_FLAG;
	if (z_header.version == V3 && (z_header.flags & OLD_SOUND_FLAG))
#ifdef NO_SOUND
		if (!dos_init_sound())
#endif
			z_header.flags &= ~OLD_SOUND_FLAG;
	if (z_header.flags & SOUND_FLAG)
#ifdef NO_SOUND
		if (!dos_init_sound())
#endif
			z_header.flags &= ~SOUND_FLAG;
	if (z_header.version >= V5 && (z_header.flags & UNDO_FLAG))
		if (!f_setup.undo_slots)
			z_header.flags &= ~UNDO_FLAG;
	if (z_header.flags & MOUSE_FLAG)
		if (subdisplay != -1 || !detect_mouse())
			z_header.flags &= ~MOUSE_FLAG;
	if (z_header.flags & COLOUR_FLAG)
		if (display == _MONO_ || display == _CGA_)
			z_header.flags &= ~COLOUR_FLAG;
	z_header.flags &= ~MENU_FLAG;

	/* Set the screen dimensions, font size and default colour */
	z_header.screen_width = info[display].width;
	z_header.screen_height = info[display].height;
	z_header.font_height = info[display].font_height;
	z_header.font_width = info[display].font_width;
	z_header.default_foreground = info[display].fg;
	z_header.default_background = info[display].bg;

	if (subdisplay != -1) {
		z_header.screen_width = subinfo[subdisplay].width;
		z_header.screen_height = subinfo[subdisplay].height;
	}

	if (user_screen_width != -1)
		z_header.screen_width = user_screen_width;
	if (user_screen_height != -1)
		z_header.screen_height = user_screen_height;

	z_header.screen_cols = z_header.screen_width / z_header.font_width;
	z_header.screen_rows = z_header.screen_height / z_header.font_height;

	if (user_foreground != -1)
		z_header.default_foreground = zcolour[user_foreground];
	if (user_background != -1)
		z_header.default_background = zcolour[user_background];

	/* Set the interpreter number (a constant telling the game which
	   operating system it runs on) and the interpreter version. The
	   interpreter number has effect on V6 games and "Beyond Zork". */
	z_header.interpreter_number = INTERP_MSDOS;
	z_header.interpreter_version = 'F';

	if (display == _AMIGA_)
		z_header.interpreter_number = INTERP_AMIGA;

	/* Install the fast_exit routine to handle the ctrl-break key */
	oldvect = getvect(0x1b);
	setvect(0x1b, fast_exit);
} /* os_init_screen */


/*
 * os_reset_screen
 *
 * Reset the screen before the program stops.
 *
 */
void os_reset_screen(void)
{
	os_set_font(TEXT_FONT);
	os_set_text_style(0);
	os_display_string((zchar *) "[Hit any key to exit.]");
	os_read_key(0, TRUE);

	cleanup();
} /* os_reset_screen */


/*
 * os_restart_game
 *
 * This routine allows the interface to interfere with the process of
 * restarting a game at various stages:
 *
 *     RESTART_BEGIN - restart has just begun
 *     RESTART_WPROP_SET - window properties have been initialised
 *     RESTART_END - restart is complete
 *
 */
void os_restart_game(int stage)
{
	int x, y;

	if (story_id == BEYOND_ZORK) {
		if (stage == RESTART_BEGIN) {
			if ((display == _MCGA_ || display == _AMIGA_)
			    && os_picture_data(1, &x, &y)) {

				special_palette();
#ifdef __WATCOMC__
				bios_video_ax_bx_dh_ch_cl(0x1010, 64, 0, 0, 0);
				bios_video_ax_bx_dh_ch_cl(0x1010, 79,
							  0xff, 0xff, 0xff);
#else				asm mov ax, 0x1010
				asm mov bx, 64
				asm mov dh, 0
				asm mov ch, 0
				asm mov cl, 0
				asm int 0x10
				asm mov ax, 0x1010
				asm mov bx, 79
				asm mov dh, 0xff
				asm mov ch, 0xff
				asm mov cl, 0xff
				asm int 0x10
#endif

				os_draw_picture(1, 1, 1);
				os_read_key(0, FALSE);

				standard_palette();
			}
		}
	}
} /* os_restart_game */


/*
 * os_random_seed
 *
 * Return an appropriate random seed value in the range from 0 to
 * 32767, possibly by using the current system time.
 *
 */
int os_random_seed(void)
{
	if (user_random_seed == -1) {
		/* Use the time of day as seed value */
#ifdef __WATCOMC__
		return (int)bios_time_ah(0) & 0x7fff;
#else
		asm mov ah, 0
		asm int 0x1a
		return _DX & 0x7fff;
#endif
	} else
		return user_random_seed;
} /* os_random_seed */


/*
 * os_path_open
 *
 * Open a file in the current directory.  If this fails then
 * search the directories in the ZCODE_PATH environment variable,
 * if it is defined, otherwise search INFOCOM_PATH.
 *
 */
FILE *os_path_open(const char *name, const char *mode)
{
	FILE *fp;
	char buf[MAX_FILE_NAME + 1];
	char *p, *bp, lastch;

	if ((fp = fopen(name, mode)) != NULL)
		return fp;
	if ((p = getenv("ZCODE_PATH")) == NULL)
		p = getenv("INFOCOM_PATH");
	if (p != NULL) {
		while (*p) {
			bp = buf;
			while (*p && *p != OS_SEARCH_PATHSEP)
				lastch = *bp++ = *p++;
			if (lastch != '\\' && lastch != '/')
				*bp++ = '\\';
			strcpy(bp, name);
			if ((fp = fopen(buf, mode)) != NULL)
				return fp;
			if (*p)
				p++;
		}
	}
	return NULL;
} /* os_path_open */


/*
 * os_load_story
 *
 * This is different from os_path_open() because we need to see if the
 * story file is actually a chunk inside a blorb file.  Right now we're
 * looking only at the exact path we're given on the command line.
 *
 * Open a file in the current directory.  If this fails, then search the
 * directories in the ZCODE_PATH environmental variable.  If that's not
 * defined, search INFOCOM_PATH.
 *
 */
FILE *os_load_story(void)
{
#ifndef NO_BLORB
	FILE *fp;

	switch (dos_blorb_init(f_setup.story_file)) {
	case bb_err_NoBlorb:
/*		printf("No blorb file found.\n\n"); */
		break;
	case bb_err_Format:
		printf("Blorb file loaded, but unable to build map.\n\n");
		break;
	case bb_err_NotFound:
		printf("Blorb file loaded, but lacks executable chunk.\n\n");
		break;
	case bb_err_None:
/*		printf("No blorb errors.\n\n"); */
		break;
	}

	fp = fopen(f_setup.story_file, "rb");

	/* Is this a Blorb file containing Zcode? */
	if (f_setup.exec_in_blorb)
		fseek(fp, blorb_res.data.startpos, SEEK_SET);

	return fp;
#else
	return fopen(f_setup.story_file, "rb");
#endif
} /* os_load_story */


/*
 * Seek into a storyfile, either a standalone file or the
 * ZCODE chunk of a blorb file
 */
int os_storyfile_seek(FILE * fp, long offset, int whence)
{
#ifndef NO_BLORB
	/* Is this a Blorb file containing Zcode? */
	if (f_setup.exec_in_blorb) {
		switch (whence) {
		case SEEK_END:
			return fseek(fp, blorb_res.data.startpos + blorb_res.length + offset, SEEK_SET);
			break;
		case SEEK_CUR:
			return fseek(fp, offset, SEEK_CUR);
			break;
		case SEEK_SET:
			/* SEEK_SET falls through to default */
		default:
			return fseek(fp, blorb_res.data.startpos + offset, SEEK_SET);
			break;
		}
	}
#endif
	return fseek(fp, offset, whence);
} /* os_storyfile_seek */


/*
 * Tell the position in a storyfile, either a standalone file
 * or the ZCODE chunk of a blorb file
 */
int os_storyfile_tell(FILE * fp)
{
#ifndef NO_BLORB
	/* Is this a Blorb file containing Zcode? */
	if (f_setup.exec_in_blorb)
		return ftell(fp) - blorb_res.data.startpos;
#endif
	return ftell(fp);
} /* os_storyfile_tell */
