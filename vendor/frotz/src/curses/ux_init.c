/*
 * ux_init.c - Unix interface, initialisation
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

/* For the MACOS port, get the _DARWIN_C_SOURCE define */
#include "../common/defs.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include <time.h>

#include <unistd.h>
#include <ctype.h>
#include <libgen.h>

#include "ux_defines.h"

#ifdef USE_NCURSES_H
#include <ncurses.h>
#else
#include <curses.h>
#endif

#ifndef WIN32
#include <termios.h>
#endif

#include "ux_frotz.h"
#include "ux_blorb.h"

#ifdef USE_UTF8
#include <locale.h>
#endif

extern f_setup_t f_setup;
extern z_header_t z_header;

volatile sig_atomic_t terminal_resized = 0;

static void sigwinch_handler(int);
static void parse_options(int argc, char **argv);

#define INFORMATION "\
An interpreter for all Infocom and other Z-Machine games.\n\
\n\
Syntax: frotz [options] story-file [blorb file]\n\
  -a   watch attribute setting    \t -o   watch object movement\n\
  -A   watch attribute testing    \t -O   watch object locating\n\
  -b <colorname> background color \t -p   plain ASCII output only\n\
  -c # context lines              \t -P   alter piracy opcode\n\
  -d   disable color              \t -q   quiet (disable sound effects)\n\
  -e   enable sound               \t -r # right margin\n\
  -E <style> emphasis style       \t -R <path> restricted read/write\n\
  -f <colorname> foreground color \t -s # random number seed value\n\
  -F   Force color mode           \t -S # transcript width\n\
  -h # text height                \t -t   set Tandy bit\n\
  -i   ignore fatal errors        \t -u # slots for multiple undo\n\
  -I # interpreter number         \t -v   show version information\n\
  -l # left margin                \t -w # text width\n\
  -L <file> load this save file   \t -x   expand abbreviations g/x/z\n\
  -m   enable mouse support       \t -Z # error checking (see below)\n"

#define INFO2 "\
Error checking: 0 none, 1 first only (default), 2 all, 3 exit after any error.\n\
For more options and explanations, please read the manual page.\n"


/*
char stripped_story_name[FILENAME_MAX+1];
char semi_stripped_story_name[FILENAME_MAX+1];
*/

extern u_setup_t u_setup;

/* static void sigwinch_handler(int); */
static void sigint_handler(int);
/* static void redraw(void); */

static void	usage(void);
static void	print_version(void);
static int	getconfig(char *);
static int	getbool(char *);
static int	getemph(char *);
static int	getcolor(char *);
static int	geterrmode(char *);
static FILE	*pathopen(const char *, const char *, const char *);


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
	vsnprintf(errorstring, sizeof(char) * 80, s, m);
	va_end(m);

	if (u_setup.curses_active) {
		os_beep(BEEP_HIGH);
		style = os_get_text_style();
		os_set_text_style(BOLDFACE_STYLE);
		print_c_string("Warning: ");
		os_set_text_style(NORMAL_STYLE);
		print_c_string(errorstring);
		new_line();
		os_set_text_style(style);
	} else
		printf("Warning: %s\n", errorstring);

	return;
} /* os_warn */


/*
 * os_fatal
 *
 * Display error message and exit program.
 *
 */
void os_fatal (const char *s, ...)
{
	va_list m;
	char errorstring[81];
	int style;

	va_start(m, s);
	vsnprintf(errorstring, sizeof(char) * 80, s, m);
	va_end(m);

	if (u_setup.curses_active) {
		/* Solaris 2.6's cc complains if the below cast is missing */
		print_c_string("\n\n");
		os_beep(BEEP_HIGH);
		style = u_setup.current_text_style;
		os_set_text_style(BOLDFACE_STYLE);
		print_c_string("Fatal error: ");
		os_set_text_style(NORMAL_STYLE);
		print_c_string(errorstring);
		print_c_string("\n");
		new_line();
		os_set_text_style(style);
		if (f_setup.ignore_errors) {
			print_c_string("Continuing anyway...");
			new_line();
			scrollok(stdscr, TRUE);
			scroll(stdscr);
			flush_buffer();
			refresh();
			return;
		} else {
			os_reset_screen();
			ux_blorb_stop();
			os_quit(EXIT_FAILURE);
		}
	}

	fputs ("\nFatal error: ", stderr);
	fputs (errorstring, stderr);
	if (f_setup.ignore_errors) {
		fputs ("\n\rContinuing anyway...", stderr);
		return;
	}

	fputs ("\n\n", stderr);

	os_quit (EXIT_FAILURE);
} /* os_fatal */

/* extern char script_name[]; */
/* extern char command_name[]; */
/* extern char save_name[];*/
/*extern char auxiliary_name[];*/


/*
 * os_process_arguments
 *
 * Handle command line switches.
 * Some variables may be set to activate special features of Frotz.
 *
 */
void os_process_arguments (int argc, char *argv[])
{
	char *p = NULL;
	char *home;
	char configfile[FILENAME_MAX + 1];

	zoptarg = NULL;

#if !defined(WIN32) && !defined(__HAIKU__)
	if ((getuid() == 0) || (geteuid() == 0)) {
		printf("I won't run as root!\n");
		os_quit(EXIT_FAILURE);
	}
#endif

#ifdef WIN32
#define HOMEDIR "USERPROFILE"
#else
#define HOMEDIR "HOME"
#endif

	if ((home = getenv(HOMEDIR)) == NULL) {
		printf("Hard drive on fire!\n");
		os_quit(EXIT_FAILURE);
	}


/*
 * It doesn't look like Frotz can reliably be resized given its current
 * screen-handling code.  While playing with Nitfol, I noticed that it
 * resized itself fairly reliably, even though the terminal looked rather
 * ugly to begin with.  Since Nitfol uses the Glk library for screen I/O,
 * I think there might be something in Glk that can make resizing easier.
 * Something to think about for later.
 *
 */


/*	if (signal(SIGWINCH, SIG_IGN) != SIG_IGN) */
	signal(SIGWINCH, sigwinch_handler);

	if (signal(SIGINT, SIG_IGN) != SIG_IGN)
		signal(SIGINT, sigint_handler);

	if (signal(SIGTTIN, SIG_IGN) != SIG_IGN)
		signal(SIGTTIN, SIG_IGN);

	if (signal(SIGTTOU, SIG_IGN) != SIG_IGN)
		signal(SIGTTOU, SIG_IGN);

	/* First check for a "$HOME/.frotzrc". */
	/* If not found, look for CONFIG_DIR/frotz.conf */
	/* $HOME/.frotzrc overrides CONFIG_DIR/frotz.conf */

	strncpy(configfile, home, FILENAME_MAX);
	strncat(configfile, "/", 2);

	strncat(configfile, USER_CONFIG, strlen(USER_CONFIG) + 1);
	if (!getconfig(configfile)) {
		strncpy(configfile, CONFIG_DIR, FILENAME_MAX);
		strncat(configfile, "/", 2);    /* added by DJP */
		strncat(configfile, MASTER_CONFIG, FILENAME_MAX-10);
		getconfig(configfile);  /* we're not concerned if this fails */
	}

	parse_options(argc, argv);

	/* This section is exceedingly messy and really can't be fixed
	 * without major changes all over the place.
	 */

	/* Save the story file name */

	f_setup.story_file = strdup(argv[zoptind]);
	f_setup.story_name = strdup(basename(argv[zoptind]));

#ifndef NO_BLORB
	if (argv[zoptind+1] != NULL)
		f_setup.blorb_file = strdup(argv[zoptind+1]);
#endif

	/* Now strip off the extension */
	p = strrchr(f_setup.story_name, '.');
	if ( p != NULL )
		*p = '\0';  /* extension removed */

	/* Create nice default file names */
	f_setup.script_name = malloc((strlen(f_setup.story_name) + strlen(EXT_SCRIPT) + 1) * sizeof(char));
	memcpy(f_setup.script_name, f_setup.story_name, (strlen(f_setup.story_name) + strlen(EXT_SCRIPT)) * sizeof(char));
	strncat(f_setup.script_name, EXT_SCRIPT, strlen(EXT_SCRIPT)+1);

	f_setup.command_name = malloc((strlen(f_setup.story_name) + strlen(EXT_COMMAND) + 1) * sizeof(char));
	memcpy(f_setup.command_name, f_setup.story_name, (strlen(f_setup.story_name) + strlen(EXT_COMMAND)) * sizeof(char));
	strncat(f_setup.command_name, EXT_COMMAND, strlen(EXT_COMMAND)+1);

	if (!f_setup.restore_mode) {
		f_setup.save_name = malloc((strlen(f_setup.story_name) + strlen(EXT_SAVE) + 1) * sizeof(char));
		memcpy(f_setup.save_name, f_setup.story_name, (strlen(f_setup.story_name) + strlen(EXT_SAVE)) * sizeof(char));
		strncat(f_setup.save_name, EXT_SAVE, strlen(EXT_SAVE) + 1);
	} else {  /*Set our auto load save as the name_save*/
		f_setup.save_name = malloc((strlen(f_setup.tmp_save_name) + strlen(EXT_SAVE) + 1) * sizeof(char));
		memcpy(f_setup.save_name, f_setup.tmp_save_name, (strlen(f_setup.story_name) + strlen(EXT_SAVE)) * sizeof(char));
		free(f_setup.tmp_save_name);
	}

	f_setup.aux_name = malloc((strlen(f_setup.story_name) + strlen(EXT_AUX) + 1) * sizeof(char));
	memcpy(f_setup.aux_name, f_setup.story_name, (strlen(f_setup.story_name) + strlen(EXT_AUX)) * sizeof(char));
	strncat(f_setup.aux_name, EXT_AUX, strlen(EXT_AUX) + 1);

/*
	for (int i = 0; i < NON_STD_COLS; i++)
		u_setup.nonstdcolours[i] = 0xffffffff;
	u_setup.nonstdindex = 0;
*/

} /* os_process_arguments */


/*
 * parse_options
 *
 * Parse command line options and that's it.
 *
 */
static void parse_options(int argc, char **argv)
{
	int c;
	do {
		c = zgetopt(argc, argv, "aAb:c:deE:f:Fh:iI:l:L:moOpPqr:R:s:S:tu:vw:W:xZ:");
		switch(c) {
		case 'a': f_setup.attribute_assignment = 1; break;
		case 'A': f_setup.attribute_testing = 1; break;
		case 'b':
			u_setup.background_color = getcolor(zoptarg);
			u_setup.force_color = 1;
			u_setup.disable_color = 0;
			if ((u_setup.background_color < 2) ||
			    (u_setup.background_color > 9))
				u_setup.background_color = -1;
			break;
		case 'c': f_setup.context_lines = atoi(zoptarg); break;
		case 'd': u_setup.disable_color = 1; break;
		case 'e': f_setup.sound = 1; break;
		case 'E':
			u_setup.emphasis_mode = getemph(zoptarg); break;
		case 'f':
			u_setup.foreground_color = getcolor(zoptarg);
			u_setup.force_color = 1;
			u_setup.disable_color = 0;
			if ((u_setup.foreground_color < 2) ||
			  (u_setup.foreground_color > 9))
				u_setup.foreground_color = -1;
			break;
		case 'F':
			u_setup.force_color = 1;
			u_setup.disable_color = 0;
			break;
		case 'h': u_setup.screen_height = atoi(zoptarg); break;
		case 'i': f_setup.ignore_errors = 1; break;
		case 'I': f_setup.interpreter_number = atoi(zoptarg); break;
		case 'l': f_setup.left_margin = atoi(zoptarg); break;
		case 'L':
			f_setup.restore_mode = 1;
			f_setup.tmp_save_name = strdup(zoptarg);
			break;
		case 'm': u_setup.mouse_enabled = TRUE; break;
		case 'o': f_setup.object_movement = 1; break;
		case 'O': f_setup.object_locating = 1; break;
		case 'p': u_setup.plain_ascii = 1; break;
		case 'P': f_setup.piracy = 1; break;
		case 'q': f_setup.quiet = 1; break;
		case 'r': f_setup.right_margin = atoi(zoptarg); break;
		case 'R': f_setup.restricted_path = strndup(zoptarg, PATH_MAX); break;
		case 's': u_setup.random_seed = atoi(zoptarg); break;
		case 'S': f_setup.script_cols = atoi(zoptarg); break;
		case 't': f_setup.tandy = 1; break;
		case 'u': f_setup.undo_slots = atoi(zoptarg); break;
		case 'v': print_version(); os_quit(EXIT_SUCCESS); break;
		case 'w': u_setup.screen_width = atoi(zoptarg); break;
		case 'x': f_setup.expand_abbreviations = 1; break;
		case 'Z':
			f_setup.err_report_mode = atoi(zoptarg);
			if ((f_setup.err_report_mode < ERR_REPORT_NEVER) ||
			  (f_setup.err_report_mode > ERR_REPORT_FATAL))
				f_setup.err_report_mode = ERR_DEFAULT_REPORT_MODE;
			break;
		case '?':
			usage();
			os_quit(EXIT_FAILURE);
			break;
		}
	} while (c != EOF);

	if (argv[zoptind] == NULL) {
		usage();
		os_quit(EXIT_SUCCESS);
	}
} /* parse_options */


void unix_get_terminal_size()
{
	int y, x;
	getmaxyx(stdscr, y, x);

	/* 255 disables paging entirely. */
	if (u_setup.screen_height != -1)
		z_header.screen_rows = u_setup.screen_height;
	else
		z_header.screen_rows = MIN(254, y);

	if (u_setup.screen_width != -1)
		z_header.screen_cols = u_setup.screen_width;
	else
		z_header.screen_cols = MIN(255, x);

	if (z_header.screen_cols < 1) {
		endwin();
		u_setup.curses_active = FALSE;
		os_fatal("Invalid screen width. Must be between 1 and 255.");
	}

	z_header.font_width = 1;
	z_header.font_height = 1;

	z_header.screen_width = z_header.screen_cols;
	z_header.screen_height = z_header.screen_rows;
} /* unix_get_terminal */


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
 * (Unix has a non brain-damaged memory model which dosen't require such
 *  ugly hacks, neener neener neener. --GH :)
 *
 */
void os_init_screen (void)
{
	/*trace(TRACE_CALLS);*/

#ifdef USE_UTF8
	setlocale(LC_ALL, "");
#endif

	if (getenv("ESCDELAY") == NULL)
		setenv("ESCDELAY", "50", 1);

	if (initscr() == NULL) {    /* Set up curses */
		os_fatal("Unable to initialize curses. Maybe your $TERM setting is bad.");
		os_quit(EXIT_FAILURE);
	}
	u_setup.curses_active = 1;	/* Let os_fatal know curses is running */
	raw();				/* Raw input mode, no line processing */
	noecho();			/* No input echo */
	nonl();				/* No newline translation */
	intrflush(stdscr, TRUE);	/* Flush output on interrupt */
	keypad(stdscr, TRUE);		/* Enable the keypad and function keys */
	scrollok(stdscr, FALSE);	/* No scrolling unless explicitly asked for */

	/* It's nice to know what terminal we have to work with. */
	u_setup.term = strdup(getenv("TERM"));

	if (z_header.version == V3 && f_setup.tandy != 0)
		z_header.config |= CONFIG_TANDY;

	if (z_header.version == V3)
		z_header.config |= CONFIG_SPLITSCREEN;

	if (z_header.version >= V4) {
		z_header.config |= CONFIG_BOLDFACE | CONFIG_FIXED | CONFIG_TIMEDINPUT;
		if (u_setup.emphasis_mode != EMPHASIS_NONE)
			z_header.config |= CONFIG_EMPHASIS;
	}

	if (z_header.version >= V5)
		z_header.flags &= ~(GRAPHICS_FLAG | MENU_FLAG);

#ifdef NCURSES_MOUSE_VERSION
	/* Limited mouse support */
	if (u_setup.mouse_enabled && z_header.version >= V5)
		mousemask(BUTTON1_CLICKED | BUTTON1_DOUBLE_CLICKED, NULL);
	else
#endif
	if (z_header.version >= V5) {
		z_header.flags &= ~MOUSE_FLAG;
	}

#ifdef NO_SOUND
	if (z_header.version >= V5)
		z_header.flags &= ~SOUND_FLAG;

	if (z_header.version == V3)
		z_header.flags &= ~OLD_SOUND_FLAG;
#else
	if ((z_header.version >= V5) && (z_header.flags & SOUND_FLAG))
		z_header.flags |= SOUND_FLAG;

	if ((z_header.version == V3) && (z_header.flags & OLD_SOUND_FLAG))
		z_header.flags |= OLD_SOUND_FLAG;

	if ((z_header.version == V6) && (f_setup.sound != 0))
		z_header.config |= CONFIG_SOUND;
#endif

	if (z_header.version >= V5 && (z_header.flags & UNDO_FLAG)) {
		if (f_setup.undo_slots == 0)
			z_header.flags &= ~UNDO_FLAG;
	}

	unix_get_terminal_size();

	/* Must be after screen dimensions are computed.  */
	if (z_header.version == V6) {
		if (unix_init_pictures())
			z_header.config |= CONFIG_PICTURES;
		else
			z_header.flags &= ~GRAPHICS_FLAG;
	}

	/* Use the ms-dos interpreter number for v6, because that's the
	 * kind of graphics files we understand.  Otherwise, use DEC.  */
	if (f_setup.interpreter_number == INTERP_DEFAULT)
		z_header.interpreter_number = z_header.version == V6 ? INTERP_MSDOS : INTERP_DEC_20;
	else
		z_header.interpreter_number = f_setup.interpreter_number;

	z_header.interpreter_version = 'F';

#ifdef COLOR_SUPPORT
	/* Enable colors if the terminal supports them, the user did not
	 * disable colors, and the game or user requested colors.  User
	 * requests them by specifying a foreground or background.
	 */
	u_setup.color_enabled = (has_colors() && !u_setup.disable_color
		&& (((z_header.version >= V5) && (z_header.flags & COLOUR_FLAG))
		|| (u_setup.foreground_color != -1)
		|| (u_setup.background_color != -1)));

	/* Maybe we don't want to muck about with changing $TERM to
	 * xterm-color which some supposedly current Unicies still don't
	 * understand.
	 */
	if (u_setup.force_color)
		u_setup.color_enabled = TRUE;

	if (u_setup.color_enabled) {
		z_header.config |= CONFIG_COLOUR;
		z_header.flags |= COLOUR_FLAG; /* FIXME: beyond zork handling? */
		start_color();
		z_header.default_foreground = (u_setup.foreground_color == -1)
			? FOREGROUND_DEF : u_setup.foreground_color;
		z_header.default_background = (u_setup.background_color ==-1)
			? BACKGROUND_DEF : u_setup.background_color;
	} else
#endif /* COLOR_SUPPORT */
	{
		/* Set these per spec 8.3.2. */
		z_header.default_foreground = WHITE_COLOUR;
		z_header.default_background = BLACK_COLOUR;
		if (z_header.flags & COLOUR_FLAG) z_header.flags &= ~COLOUR_FLAG;
	}
	os_set_colour(z_header.default_foreground, z_header.default_background);
	os_erase_area(1, 1, z_header.screen_rows, z_header.screen_cols, 0);
} /* os_init_screen */


/*
 * os_reset_screen
 *
 * Reset the screen before the program stops.
 *
 */
void os_reset_screen (void)
{
#ifdef LATE_ZORK
	/* Modified 2026-10-09: door closes without an extra exit keystroke. */
	if (door_enabled()) return;
#endif
	os_stop_sample(0);
	os_set_text_style(0);
	print_c_string("[Hit any key to exit.]\n");
	os_read_key(0, FALSE);
} /* os_reset_screen */


/*
 * os_quit
 *
 * Immediately and cleanly exit, passing along exit status.
 *
 */
void os_quit(int status)
{
	os_stop_sample(0);
	ux_blorb_stop();
	if (u_setup.curses_active) {
		scrollok(stdscr, TRUE);
		scroll(stdscr);
		refresh();
		endwin();
	}
	exit(status);
} /* os_quit */


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
void os_restart_game (int UNUSED (stage))
{
	/* Nothing here yet */
} /* os_restart_game */


/*
 * os_random_seed
 *
 * Return an appropriate random seed value in the range from 0 to
 * 32767, possibly by using the current system time.
 *
 */
int os_random_seed (void)
{
	/* Use the epoch as seed value */
	if (u_setup.random_seed == -1)
		return (time(0) & 0x7fff);
	else
		return u_setup.random_seed;
} /* os_random_seed */


/*
 * os_path_open
 *
 * Open a file in the current directory.  If this fails, then search the
 * directories in the ZCODE_PATH environmental variable.  If that's not
 * defined, search INFOCOM_PATH.
 *
 */
FILE *os_path_open(const char *name, const char *mode)
{
	FILE *fp;
	char *p;

	/* Let's see if the file is in the currect directory */
	/* or if the user gave us a full path. */
	if ((fp = fopen(name, mode)))
		return fp;

	/* If zcodepath is defined in a config file, check that path. */
	/* If we find the file a match in that path, great. */
	/* Otherwise, check some environmental variables. */
	if (f_setup.zcode_path != NULL) {
		if ((fp = pathopen(name, f_setup.zcode_path, mode)) != NULL)
			return fp;
	}

	if ( (p = getenv(PATH1) ) == NULL)
		p = getenv(PATH2);

	if (p != NULL) {
		fp = pathopen(name, p, mode);
		return fp;
	}
	return NULL;	/* give up */
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

	switch (ux_blorb_init(f_setup.story_file)) {
	case bb_err_NoBlorb:
		/* printf("No blorb file found.\n\n"); */
		break;
	case bb_err_Format:
		printf("Blorb file loaded, but unable to build map.\n\n");
		break;
	case bb_err_NotFound:
		printf("Blorb file loaded, but lacks ZCOD executable chunk.\n\n");
		break;
	case bb_err_None:
		/* printf("No blorb errors.\n\n"); */
		break;
	}

	fp = os_path_open(f_setup.story_file, "rb");

	/* Is this a Blorb file containing Zcode? */
	if (f_setup.exec_in_blorb)
		fseek(fp, blorb_res.data.startpos, SEEK_SET);

	return fp;
#else
	return os_path_open(f_setup.story_file, "rb");
#endif
} /* os_load_story */


/*
 * os_storyfile_seek
 *
 * Seek into a storyfile, either a standalone file or the
 * ZCODE chunk of a blorb file.
 *
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
	} else
		return fseek(fp, offset, whence);
#else
	return fseek(fp, offset, whence);
#endif
} /* os_storyfile_seek */


/*
 * os_storyfile_tell
 *
 * Tell the position in a storyfile, either a standalone file
 * or the ZCODE chunk of a blorb file.
 *
 */
int os_storyfile_tell(FILE * fp)
{
#ifdef NO_BLORB
	/* Is this a Blorb file containing Zcode? */
	if (f_setup.exec_in_blorb)
		return ftell(fp) - blorb_res.data.startpos;
	else
		return ftell(fp);
#else
	return ftell(fp);
#endif
} /* os_storyfile_tell */


/*
 * pathopen
 *
 * Given a standard Unix-style path and a filename, search the path for
 * that file.  If found, return a pointer to that file
 *
 */
static FILE *pathopen(const char *name, const char *path, const char *mode)
{
	FILE *fp;
	char *buf;
	char *bp;

	/*
	 * If the path variable doesn't end in a "/" a "/"
	 * will be added, so the buffer needs to be long enough
	 * for the path + / + name + \0
	 */
	size_t buf_sz = strlen(path) + strlen(name) + sizeof(DIRSEP) + 1;
	buf = malloc(buf_sz);
	
	/*
	 * Split the PATH string where the PATHSEP character appears
	 * and try to open the named file at each portion of the path
	 */
	while(*path) {
		bp = buf;
		while (*path && (*path != PATHSEP))
			*bp++ = *path++;
		if(*(path - 1) != DIRSEP)
			*bp++ = DIRSEP;
		memcpy(bp, name, strlen(name));
		/*
		 * Null terminate the buffer
		 */
		bp[strlen(name)] = 0;
		if ((fp = fopen(buf, mode)) != NULL) {
			free(buf);
			return fp;
		}
		if (*path)
			path++;
	}
	free(buf);
	return NULL;

} /* pathopen */


/*
 * getconfig
 *
 * Parse a <variable> <whitespace> <value> config file.
 * The til-end-of-line comment character is the COMMENT define.  I use '#'
 * here.  This code originally appeared in my q2-wrapper program.  Find it
 * at metalab.cs.unc.edu or assorted Quake2 websites.
 *
 * This section must be modified whenever new options are added to
 * the config file.  Ordinarily I would use yacc and lex, but the grammar
 * is too simple to involve those resources, and I can't really expect all
 * compile targets to have those two tools installed.
 *
 */
static int getconfig(char *configfile)
{
	FILE	*fp;

	size_t	num, num2;

	char	varname[LINELEN + 1];
	char	value[LINELEN + 1];

	/*
	 * We shouldn't care if the config file is unreadable or not
	 * present.  Just use the defaults.
	 *
	 */

	if ((fp = fopen(configfile, "r")) == NULL)
		return FALSE;

	while (fgets(varname, LINELEN, fp) != NULL) {

		/* If we don't get a whole line, dump the rest of the line */
		if (varname[strlen(varname)-1] != '\n')
			while (fgetc(fp) != '\n')
			;

		/* Remove trailing whitespace and newline */
		for (num = strlen(varname) - 1; (ssize_t) num >= 0 && isspace((int) varname[num]); num--)
		;
		varname[num+1] = 0;

		/* Drop everything past the comment character */
		for (num = 0; num <= strlen(varname)+1; num++) {
			if (varname[num] == COMMENT)
				varname[num] = 0;
		}

		/* Find end of variable name */
		for (num = 0; varname[num] != 0 && !isspace((int) varname[num]) && num < LINELEN; num++);

		for (num2 = num; isspace((int) varname[num2]) && num2 < LINELEN; num2++);

		/* Find the beginning of the value */
		strncpy(value, &varname[num2], LINELEN);
		varname[num] = 0; /* chop off value from the var name */

		/* varname now contains the variable name */


		/* First, boolean config stuff */
		if (strcmp(varname, "attrib_set") == 0) {
			f_setup.attribute_assignment = getbool(value);
		}
		else if (strcmp(varname, "attrib_test") == 0) {
			f_setup.attribute_testing = getbool(value);
		}

		else if (strcmp(varname, "ignore_fatal") == 0) {
			f_setup.ignore_errors = getbool(value);
		}

		else if (strcmp(varname, "color") == 0) {
			u_setup.disable_color = !getbool(value);
		}
		else if (strcmp(varname, "colour") == 0) {
			u_setup.disable_color = !getbool(value);
		}
		else if (strcmp(varname, "force_color") == 0) {
			u_setup.force_color = getbool(value);
		}
		else if (strcmp(varname, "emphasis_mode") == 0) {
			u_setup.emphasis_mode = getemph(value);
		}
		else if (strcmp(varname, "obj_move") == 0) {
			f_setup.object_movement = getbool(value);
		}
		else if (strcmp(varname, "obj_loc") == 0) {
			f_setup.object_locating = getbool(value);
		}
		else if (strcmp(varname, "piracy") == 0) {
			f_setup.piracy = getbool(value);
		}
		else if (strcmp(varname, "ascii") == 0) {
			u_setup.plain_ascii = getbool(value);
		}
		else if (strcmp(varname, "sound") == 0) {
			f_setup.sound = getbool(value);
		}
		else if (strcmp(varname, "tandy") == 0) {
			f_setup.tandy = getbool(value);
		}
		else if (strcmp(varname, "expand_abb") == 0) {
			f_setup.expand_abbreviations = getbool(value);
		}

		/* now for stringtype yet still numeric variables */
		else if (strcmp(varname, "background") == 0) {
			u_setup.background_color = getcolor(value);
		}
		else if (strcmp(varname, "foreground") == 0) {
			u_setup.foreground_color = getcolor(value);
		}
		else if (strcmp(varname, "context_lines") == 0) {
			f_setup.context_lines = atoi(value);
		}
		else if (strcmp(varname, "screen_height") == 0) {
			u_setup.screen_height = atoi(value);
		}
		else if (strcmp(varname, "left_margin") == 0) {
			f_setup.left_margin = atoi(value);
		}
		else if (strcmp(varname, "right_margin") == 0) {
			f_setup.right_margin = atoi(value);
		}
		else if (strcmp(varname, "randseed") == 0) {
			u_setup.random_seed = atoi(value);
		}
		else if (strcmp(varname, "script_width") == 0) {
			f_setup.script_cols = atoi(value);
		}
		else if (strcmp(varname, "undo_slots") == 0) {
			f_setup.undo_slots = atoi(value);
		}
		else if (strcmp(varname, "screen_width") == 0) {
			u_setup.screen_width = atoi(value);
		}
		/* default is set in main() by call to init_err() */
		else if (strcmp(varname, "errormode") == 0) {
			f_setup.err_report_mode = geterrmode(value);
		}

		/* now for really stringtype variable */

		else if (strcmp(varname, "zcode_path") == 0) {
			size_t sz = strlen(value) * sizeof(char) + 1;
			f_setup.zcode_path = malloc(sz);
			memcpy(f_setup.zcode_path, value, sz);
		} /* The big nasty if-else thingy is finished */
	} /* while */
	return TRUE;
} /* getconfig */


/* getemph
 *
 * Check a string for possible emphasis types:
 * 	italic, underline, caps, or none
 *
 */
static int getemph(char *value)
{
	int num;

	/* Be case-insensitive */
	for (num = 0; value[num] != 0; num++)
		value[num] = tolower((int) value[num]);

	if (strncmp(value, "italic", 6) == 0)
		return EMPHASIS_ITALIC;

	if (strncmp(value, "under", 5) == 0)
		return EMPHASIS_UNDERLINE;

	if (strncmp(value, "none", 2) == 0)
		return EMPHASIS_NONE;

	if (atoi(value) == 0)
		return EMPHASIS_NONE;

	return EMPHASIS_ITALIC;
} /* getemph */


/*
 * getbool
 *
 * Check a string for something that means "yes" and return TRUE.
 * Otherwise return FALSE.
 *
 */
static int getbool(char *value)
{
	int num;

	/* Be case-insensitive */
	for (num = 0; value[num] !=0; num++)
		value[num] = tolower((int) value[num]);

	if (strncmp(value, "y", 1) == 0)
		return TRUE;
	if (strcmp(value, "true") == 0)
		return TRUE;
	if (strcmp(value, "on") == 0)
		return TRUE;
	if (strcmp(value, "1") == 0)
		return TRUE;

	return FALSE;
} /* getbool */


/*
 * getcolor
 *
 * Figure out what color this string might indicate and returns an integer
 * corresponding to the color macros defined in frotz.h.
 *
 */
static int getcolor(char *value)
{
	int num;

	/* Be case-insensitive */
	for (num = 0; value[num] !=0; num++)
		value[num] = tolower((int) value[num]);

	if (strcmp(value, "black") == 0)
		return BLACK_COLOUR;
	if (strcmp(value, "red") == 0)
		return RED_COLOUR;
	if (strcmp(value, "green") == 0)
		return GREEN_COLOUR;
	if (strcmp(value, "blue") == 0)
		return BLUE_COLOUR;
	if (strcmp(value, "magenta") == 0)
		return MAGENTA_COLOUR;
	if (strcmp(value, "cyan") == 0)
		return CYAN_COLOUR;
	if (strcmp(value, "white") == 0)
		return WHITE_COLOUR;
	if (strcmp(value, "yellow") == 0)
		return YELLOW_COLOUR;

	if (strcmp(value, "purple") == 0)
		return MAGENTA_COLOUR;
	if (strcmp(value, "violet") == 0)
		return MAGENTA_COLOUR;
	if (strcmp(value, "aqua") == 0)
		return CYAN_COLOUR;

	/* If we can't tell what that string means,
	 * we tell caller to use the default.
	 */

	return -1;

} /* getcolor */


/*
 * geterrmode
 *
 * Parse for "never", "once", "always", or "fatal" and return a macro
 * defined in ux_frotz.h related to the error reporting mode.
 *
 */
static int geterrmode(char *value)
{
	int num;

        /* Be case-insensitive */
	for (num = 0; value[num] !=0; num++)
		value[num] = tolower((int) value[num]);

	if (strcmp(value, "never") == 0)
		return ERR_REPORT_NEVER;
	if (strcmp(value, "once") == 0)
		return ERR_REPORT_ONCE;
	if (strcmp(value, "always") == 0)
		return ERR_REPORT_ALWAYS;
	if (strcmp(value, "fatal") == 0)
		return ERR_REPORT_FATAL;

	return ERR_DEFAULT_REPORT_MODE;
} /* geterrmode */


/*
 * sigwinch_handler
 *
 * Called whenever Frotz receives a SIGWINCH signal to make curses
 * cleanly resize the window.  To be safe, just set a flag here.
 * It is checked and cleared in unix_read_char.
 *
 */
static void sigwinch_handler(int UNUSED(sig))
{
	terminal_resized = 1;
	signal(SIGWINCH, sigwinch_handler);
} /* sigwinch_handler */


/*
 * sigint_handler
 * Sometimes the screen will be left in a weird state if the following
 * is not done.
 *
 */
static void sigint_handler(int UNUSED(dummy))
{
	signal(SIGINT, sigint_handler);
	os_quit(EXIT_FAILURE);
} /* sigint_handler */


void os_init_setup(void)
{
	f_setup.interpreter_number = INTERP_DEC_20;

	u_setup.disable_color = 0;
	u_setup.force_color = 0;
	u_setup.foreground_color = -1;
	u_setup.background_color = -1;
	u_setup.emphasis_mode = EMPHASIS_DEFAULT;
	u_setup.screen_width = -1;
	u_setup.screen_height = -1;
	u_setup.random_seed = -1;
	u_setup.random_seed = -1;
	u_setup.current_text_style = 0;
			/* Since I can't use attr_get, which
			would make things easier, I need
			to use the same hack the MS-DOS port
			does...keep the current style in a
			global variable. */
	u_setup.plain_ascii = 0; /* true if user wants to disable Latin-1 */
	u_setup.curses_active = 0;      /* true if os_init_screen has run */
	/* u_setup.interpreter = INTERP_DEFAULT; */
	u_setup.current_color = 0;
	u_setup.color_enabled = FALSE;
	u_setup.mouse_enabled = FALSE;
} /* os_init_setup */


static void usage(void)
{
	printf("FROTZ V%s - Curses interface.  ", VERSION);
#ifndef NO_SOUND
		printf("Audio output enabled.\n");
#else
		printf("Audio output disabled.\n");
#endif
	puts(INFORMATION);
	puts(INFO2);
	return;
} /* usage */


static void print_version(void)
{
	printf("FROTZ V%s     Curses interface.  ", VERSION);
#ifndef NO_SOUND
	printf("Audio output enabled.");
#else
	printf("Audio output disabled.");
#endif
	printf("\nCommit date:    %s\n", GIT_DATE);
	printf("Git commit:     %s\n", GIT_HASH);
	printf("Notes:          %s\n", RELEASE_NOTES);
	printf("  Frotz was originally written by Stefan Jokisch.\n");
	printf("  It complies with standard 1.1 of the Z-Machine Standard.\n");
	printf("  It was ported to Unix by Galen Hazelwood.\n");
	printf("  It is distributed under the GNU General Public License version 2 or\n");
	printf("    (at your option) any later version.\n");
	printf("  This software is offered as-is with no warranty or liability.\n");
	printf("  The core and Unix port are maintained by David Griffith.\n");
	printf("  Frotz's homepage is https://661.org/proj/if/frotz/\n\n");
	return;
} /* print_version */
