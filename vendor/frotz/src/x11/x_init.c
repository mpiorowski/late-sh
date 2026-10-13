/*
 * x_init.c
 *
 * X interface, initialisation
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
#include "x_info.h"
#include "x_blorb.h"

#include <stdarg.h>
#include <stdio.h>
#include <stdlib.h>
#include <strings.h>
#include <time.h>
#include <libgen.h>
#include <X11/Intrinsic.h>

x_setup_t x_setup;

/* Variables to save from os_process_arguments for use in os_init_screen */
static int saved_argc;
static char **saved_argv;
static char *user_bg, *user_fg;
static int user_random_seed = -1;
char *x_class;
char *x_name;


static void print_c_string(const char *s)
{
	int i;

	for (i = 0; i <= strlen(s); i++)
		os_display_char(translate_to_zscii(s[i]));
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

	style = os_get_text_style();
	os_beep(BEEP_HIGH);
	os_set_text_style(BOLDFACE_STYLE);
	print_c_string("Warning: ");
	os_set_text_style(NORMAL_STYLE);
	print_c_string(errorstring);
	new_line();
	os_set_text_style(style);
	return;
}


/*
 * os_fatal
 *
 * Display error message and exit program.
 *
 */
void os_fatal(const char *s, ...)
{
	va_list m;
	char errorstring[81];

	os_beep(BEEP_HIGH);
	fprintf(stderr, "\nFatal error: ");
	va_start(m, s);
	vfprintf(stderr, s, m);
	vsnprintf(errorstring, sizeof(char) * 80, s, m);
	va_end(m);
	fprintf(stderr, "\n\n");

	if (x_setup.x_active) {
		os_set_text_style(BOLDFACE_STYLE);
		print_c_string("Fatal error: ");
		os_set_text_style(NORMAL_STYLE);
		print_c_string(errorstring);
		new_line();
	}

	if (f_setup.ignore_errors) {
		os_display_string((zchar *) "Continuing anyway...");
		new_line();
		new_line();
	}

	if (x_setup.x_active)
		os_reset_screen();

	os_quit(EXIT_FAILURE);
} /* os_fatal */


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
static void parse_boolean(char *value, void *parsed)
{
	int *parsed_var = (int *)parsed;

	if (!strcasecmp(value, "on") || !strcasecmp(value, "true") ||
	    !strcasecmp(value, "yes"))
		*parsed_var = 1;
	else if (!strcasecmp(value, "off") || !strcasecmp(value, "false") ||
		 !strcasecmp(value, "no"))
		*parsed_var = 0;
	else
		fprintf(stderr, "Warning: invalid boolean resource `%s'\n",
			value);
}


static void parse_int(char *value, void *parsed)
{
	int *parsed_var = (int *)parsed;
	char *parse_end;

	int result = (int)strtol(value, &parse_end, 10);
	if (*parse_end || !(*value))
		fprintf(stderr, "Warning: invalid integer resource `%s'\n",
			value);
	else
		*parsed_var = result;
}


static void parse_zstrict(char *value, void *parsed)
{
	int *parsed_var = (int *)parsed;

	if (!strcasecmp(value, "on") || !strcasecmp(value, "true") ||
	    !strcasecmp(value, "yes") || !strcasecmp(value, "always"))
		*parsed_var = ERR_REPORT_ALWAYS;
	else if (!strcasecmp(value, "off") || !strcasecmp(value, "false") ||
		 !strcasecmp(value, "no") || !strcasecmp(value, "never"))
		*parsed_var = ERR_REPORT_NEVER;
	else if (!strcasecmp(value, "once"))
		*parsed_var = ERR_REPORT_ONCE;
	else if (!strcasecmp(value, "fatal"))
		*parsed_var = ERR_REPORT_FATAL;
	else {
		char *parse_end;
		int result = (int)strtol(value, &parse_end, 10);

		if (*parse_end || !(*value) || result < ERR_REPORT_NEVER ||
		    result > ERR_REPORT_FATAL)
			fprintf(stderr,
				"Warning: invalid zstrict level resource `%s'\n",
				value);
		else
			*parsed_var = result;
	}
}


static void parse_string(char *value, void *parsed)
{
	*((char **)parsed) = value;
}


void os_process_arguments(int argc, char *argv[])
{
	static bool show_version;
	static XrmOptionDescRec options[] = {
		{"-aa", ".watchAttrAssign", XrmoptionNoArg, (void *)"true"},
		{"+aa", ".watchAttrAssign", XrmoptionNoArg, (void *)"false"},
		{"-at", ".watchAttrTest", XrmoptionNoArg, (void *)"true"},
		{"+at", ".watchAttrTest", XrmoptionNoArg, (void *)"false"},
		{"-c", ".contextLines", XrmoptionSepArg, (void *)NULL},
		{"-ol", ".watchObjLocating", XrmoptionNoArg, (void *)"true"},
		{"+ol", ".watchObjLocating", XrmoptionNoArg, (void *)"false"},
		{"-om", ".watchObjMovement", XrmoptionNoArg, (void *)"true"},
		{"+om", ".watchObjMovement", XrmoptionNoArg, (void *)"false"},
		{"-lm", ".leftMargin", XrmoptionSepArg, (void *)NULL},
		{"-rm", ".rightMargin", XrmoptionSepArg, (void *)NULL},
		{"-e", ".ignoreErrors", XrmoptionNoArg, (void *)"true"},
		{"+e", ".ignoreErrors", XrmoptionNoArg, (void *)"false"},
		{"-p", ".piracy", XrmoptionNoArg, (void *)"true"},
		{"+p", ".piracy", XrmoptionNoArg, (void *)"false"},
		{"-R", ".restricted", XrmoptionSepArg, (void *)NULL},
		{"-t", ".tandy", XrmoptionNoArg, (void *)"true"},
		{"+t", ".tandy", XrmoptionNoArg, (void *)"false"},
		{"-u", ".undoSlots", XrmoptionSepArg, (void *)NULL},
		{"-v", ".showVersion", XrmoptionNoArg, (void *)"true"},
		{"-x", ".expandAbbrevs", XrmoptionNoArg, (void *)"true"},
		{"+x", ".expandAbbrevs", XrmoptionNoArg, (void *)"false"},
		{"-sc", ".scriptColumns", XrmoptionSepArg, (void *)NULL},
		{"-rs", ".randomSeed", XrmoptionSepArg, (void *)NULL},
		{"-zs", ".zStrict", XrmoptionSepArg, (void *)NULL},
		/* I can never remember whether it's zstrict or strictz */
		{"-sz", ".zStrict", XrmoptionSepArg, (void *)NULL},
		{"-fn-b", ".fontB", XrmoptionSepArg, (void *)NULL},
		{"-fn-i", ".fontI", XrmoptionSepArg, (void *)NULL},
		{"-fn-bi", ".fontBI", XrmoptionSepArg, (void *)NULL},
		{"-fn-f", ".fontF", XrmoptionSepArg, (void *)NULL},
		{"-fn-fb", ".fontFB", XrmoptionSepArg, (void *)NULL},
		{"-fn-fi", ".fontFI", XrmoptionSepArg, (void *)NULL},
		{"-fn-fbi", ".fontFBI", XrmoptionSepArg, (void *)NULL},
		{"-fn-z", ".fontZ", XrmoptionSepArg, (void *)NULL},
		{"-L", ".loadthis", XrmoptionSepArg, (void *)NULL}
	};
	static struct {
		char *class;
		char *name;
		void (*parser)(char *, void *);
		void *ptr;
	} vars[] = {
		{".WatchAttribute", ".watchAttrAssign", parse_boolean,
		 &f_setup.attribute_assignment},
		{".WatchAttribute", ".watchAttrTest", parse_boolean,
		 &f_setup.attribute_testing},
		{".ContextLines", ".contextLines", parse_int,
		 &f_setup.context_lines},
		{".WatchObject", ".watchObjLocating", parse_boolean,
		 &f_setup.object_locating},
		{".WatchObject", ".watchObjMovement", parse_boolean,
		 &f_setup.object_movement},
		{".Margin", ".leftMargin", parse_int,
		 &f_setup.left_margin},
		{".Margin", ".rightMargin", parse_int,
		 &f_setup.right_margin},
		{".IgnoreErrors", ".ignoreErrors", parse_boolean,
		 &f_setup.ignore_errors},
		{".Piracy", ".piracy", parse_boolean,
		 &f_setup.piracy},
		{".Restricted", ".restricted", parse_string,
		 &f_setup.restricted_path},
		{".Tandy", ".tandy", parse_boolean,
		 &f_setup.tandy},
		{".UndoSlots", ".undoSlots", parse_int,
		 &f_setup.undo_slots},
		{".ShowVersion", ".showVersion", parse_boolean,
		 &show_version},
		{".ExpandAbbrevs", ".expandAbbrevs", parse_boolean,
		 &f_setup.expand_abbreviations},
		{".ScriptColumns", ".scriptColumns", parse_int,
		 &f_setup.script_cols},
		{".RandomSeed", ".randomSeed", parse_int,
		 &x_setup.random_seed},
		{".ZStrict", ".zStrict", parse_zstrict,
		 &f_setup.err_report_mode},
		{".Background", ".background", parse_string,
		 &user_bg},
		{".Foreground", ".foreground", parse_string,
		 &user_fg},
		{".Font", ".font", parse_string,
		 &font_names[0]},
		{".Font", ".fontB", parse_string,
		 &font_names[1]},
		{".Font", ".fontI", parse_string,
		 &font_names[2]},
		{".Font", ".fontBI", parse_string,
		 &font_names[3]},
		{".Font", ".fontF", parse_string,
		 &font_names[4]},
		{".Font", ".fontFB", parse_string,
		 &font_names[5]},
		{".Font", ".fontFI", parse_string,
		 &font_names[6]},
		{".Font", ".fontFBI", parse_string,
		 &font_names[7]},
		{".Font", ".fontZ", parse_string,
		 &font_names[8]},
		{".LoadThis", ".loadthis", parse_string,
		 &f_setup.tmp_save_name}
	};
	XtAppContext app_context;
	char *str_type_return;
	char *class_buf, *name_buf, *class_append, *name_append;
	char *p;
	int i;
	XrmValue value;
	bool  bad_option;

	bad_option = FALSE;

	saved_argv = malloc(sizeof(char *) * argc);
	memcpy(saved_argv, argv, sizeof(char *) * argc);
	saved_argc = argc;
	app_context = XtCreateApplicationContext();
	dpy = XtOpenDisplay(app_context, NULL, NULL, "XFrotz",
			    options, XtNumber(options), &argc, argv);
	if (dpy == NULL)
		os_fatal("Could not open display.");

	XtGetApplicationNameAndClass(dpy, &x_name, &x_class);

	class_buf = malloc(strlen(x_class) + 20);
	strcpy(class_buf, x_class);
	class_append = strchr(class_buf, 0);
	name_buf = malloc(strlen(x_name) + 20);
	strcpy(name_buf, x_name);
	name_append = strchr(name_buf, 0);

	for (i = 0; i < XtNumber(vars); i++) {
		strcpy(class_append, vars[i].class);
		strcpy(name_append, vars[i].name);
		if (XrmGetResource(XtDatabase(dpy), name_buf, class_buf,
				   &str_type_return, &value))
			vars[i].parser((char *)value.addr, vars[i].ptr);
	}

	if (show_version) {
		printf("FROTZ V%s - X11 interface.\n", VERSION);
		printf("Commit date:    %s\n", GIT_DATE);
		printf("Git commit:     %s\n", GIT_HASH);
		printf("Notes:          %s\n", RELEASE_NOTES);
		printf("  Frotz was originally written by Stefan Jokisch.\n");
		printf("  It complies with standard 1.1 of the Z-Machine Standard.\n");
		printf("  The X11 interface code was done by Daniel Schepler,\n");
		printf("  It is distributed under the GNU General Public License version 2 or\n");
		printf("    (at your option) any later version.\n");
		printf("  This software is offered as-is with no warranty or liability.\n");
		printf("  The core and X11 port are maintained by David Griffith.\n");
		printf("  Frotz's homepage is https://661.org/proj/if/frotz.\n\n");
		os_quit(EXIT_SUCCESS);
	}

	if (argv[1] != NULL && argv[1][0] == '-') {
		bad_option = TRUE;
		printf("%s: illegal option '%s'\n", argv[0], argv[1]);
	}

	if (argc < 2 || bad_option) {
		printf("FROTZ V%s - X11 interface.\n", VERSION);
		puts(INFORMATION);
		puts(INFO2);
		os_quit(EXIT_SUCCESS);
	}

	if (f_setup.tmp_save_name != NULL)
		f_setup.restore_mode = 1;

	f_setup.story_file = strdup(argv[1]);
	f_setup.story_name = strdup(basename(argv[1]));

	if (argc > 2)
		f_setup.blorb_file = strdup(argv[2]);

	/* Now strip off the extension */
	p = strrchr(f_setup.story_name, '.');
	if (p != NULL)
		*p = '\0';	/* extension removed */

	/* Create nice default file names */
	f_setup.script_name =
	    malloc((strlen(f_setup.story_name) +
		    strlen(EXT_SCRIPT)) * sizeof(char) + 1);
	memcpy(f_setup.script_name, f_setup.story_name,
	       (strlen(f_setup.story_name) +
		strlen(EXT_SCRIPT)) * sizeof(char));
	strncat(f_setup.script_name, EXT_SCRIPT, strlen(EXT_SCRIPT) + 1);

	f_setup.command_name =
	    malloc((strlen(f_setup.story_name) +
		    strlen(EXT_COMMAND)) * sizeof(char) + 1);
	memcpy(f_setup.command_name, f_setup.story_name,
	       (strlen(f_setup.story_name) +
		strlen(EXT_COMMAND)) * sizeof(char));
	strncat(f_setup.command_name, EXT_COMMAND, strlen(EXT_COMMAND) + 1);

	if (!f_setup.restore_mode) {
		f_setup.save_name =
		    malloc((strlen(f_setup.story_name) +
			    strlen(EXT_SAVE)) * sizeof(char) + 1);
		memcpy(f_setup.save_name, f_setup.story_name,
		       (strlen(f_setup.story_name) +
			strlen(EXT_SAVE)) * sizeof(char));
		strncat(f_setup.save_name, EXT_SAVE, strlen(EXT_SAVE) + 1);
	} else {		/*Set our auto load save as the name_save */
		f_setup.save_name =
		    malloc((strlen(f_setup.tmp_save_name) +
			    strlen(EXT_SAVE)) * sizeof(char) + 1);
		memcpy(f_setup.save_name, f_setup.tmp_save_name,
		       (strlen(f_setup.tmp_save_name) +
			strlen(EXT_SAVE)) * sizeof(char));
		/* No need to free f_setup.tmp_save_name */
	}

	f_setup.aux_name =
	    malloc((strlen(f_setup.story_name) +
		    strlen(EXT_AUX)) * sizeof(char) + 1);
	memcpy(f_setup.aux_name, f_setup.story_name,
	       (strlen(f_setup.story_name) + strlen(EXT_AUX)) * sizeof(char));
	strncat(f_setup.aux_name, EXT_AUX, strlen(EXT_AUX) + 1);

} /* os_process_arguments */


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
 *
 * Finally, set reserve_mem to the amount of memory (in bytes) that
 * should not be used for multiple undo and reserved for later use.
 *
 * (Unix has a non brain-damaged memory model which dosen't require such
 *  ugly hacks, neener neener neener. --GH :)
 *
 */
Display *dpy;
Window main_window = 0;
const XFontStruct *current_font_info;
GC normal_gc, reversed_gc, bw_gc, cursor_gc, current_gc;
Pixmap bgpm;

void os_init_screen(void)
{
	XSetWindowAttributes window_attrs;
	XSizeHints *size_hints;
	XWMHints *wm_hints;
	XClassHint *class_hint;
	const char *story_basename;
	char *window_title;
	int font_width, font_height;
	XGCValues gc_setup;

	/* First, configuration parameters get set up */
	if (z_header.version == V3 && f_setup.tandy != 0)
		z_header.config |= CONFIG_TANDY;
	if (z_header.version == V3)
		z_header.config |= CONFIG_SPLITSCREEN | CONFIG_PROPORTIONAL;
	if (z_header.version >= V4)
		z_header.config |=
		    CONFIG_BOLDFACE | CONFIG_EMPHASIS | CONFIG_FIXED;
	if (z_header.version >= V5) {
		z_header.flags |=
		    GRAPHICS_FLAG | UNDO_FLAG | MOUSE_FLAG | COLOUR_FLAG;
#ifdef NO_SOUND
		z_header.flags &= ~SOUND_FLAG;
#else
		z_header.flags |= SOUND_FLAG;
#endif
	}

	if (z_header.version >= V5 && (z_header.flags & UNDO_FLAG)
	    && f_setup.undo_slots == 0)
		z_header.flags &= ~UNDO_FLAG;

	z_header.interpreter_number = INTERP_DEC_20;
	z_header.interpreter_version = 'F';

	x_init_colour(user_bg, user_fg);

	window_attrs.background_pixel = def_bg_pixel;
	window_attrs.backing_store = Always /* NotUseful */ ;
	window_attrs.save_under = FALSE;
	window_attrs.event_mask = ExposureMask | KeyPressMask | ButtonPressMask | StructureNotifyMask;;
	window_attrs.override_redirect = FALSE;
	main_window = XCreateWindow(dpy, DefaultRootWindow(dpy),
				    0, 0, X_WIDTH, X_HEIGHT, 0, CopyFromParent,
				    InputOutput, CopyFromParent,
				    CWBackPixel | CWBackingStore |
				    CWOverrideRedirect | CWSaveUnder |
				    CWEventMask, &window_attrs);

	size_hints = XAllocSizeHints();
	size_hints->min_width = size_hints->max_width = size_hints->base_width =
	    X_WIDTH;
	size_hints->min_height = size_hints->max_height =
	    size_hints->base_height = X_HEIGHT;
	size_hints->flags = PMinSize | PMaxSize | PBaseSize;

	wm_hints = XAllocWMHints();
	wm_hints->input = TRUE;
	wm_hints->initial_state = NormalState;
	wm_hints->flags = InputHint | StateHint;

	class_hint = XAllocClassHint();
	class_hint->res_name = x_name;
	class_hint->res_class = x_class;

	/* printf("%s\n\n", f_setup.story_name); */

	story_basename = strrchr(f_setup.story_name, '/');
	if (story_basename == NULL)
		story_basename = f_setup.story_name;
	else
		story_basename++;
	window_title = malloc(strlen(story_basename) + 14);
	sprintf(window_title, "XFrotz (V%d): %s", z_header.version,
		story_basename);

	XmbSetWMProperties(dpy, main_window, window_title, story_basename,
			   saved_argv, saved_argc, size_hints, wm_hints,
			   class_hint);

	XMapWindow(dpy, main_window);
	free(window_title);

	current_font_info = get_font(TEXT_FONT, 0);
	if (!current_font_info)
		os_fatal("Could not open default font");

	z_header.screen_width = X_WIDTH;
	z_header.screen_height = X_HEIGHT;
	os_font_data(FIXED_WIDTH_FONT, &font_height, &font_width);
	z_header.font_height = font_height;
	z_header.font_width = font_width;
	z_header.screen_cols = X_WIDTH / z_header.font_width;
	z_header.screen_rows = X_HEIGHT / z_header.font_height;

	fg_pixel = def_fg_pixel;
	bg_pixel = def_bg_pixel;

	gc_setup.function = GXcopy;
	gc_setup.foreground = fg_pixel;
	gc_setup.background = bg_pixel;
	gc_setup.fill_style = FillSolid;
	gc_setup.font = current_font_info->fid;
	normal_gc = XCreateGC(dpy, main_window,
			      GCFunction | GCForeground | GCBackground |
			      GCFillStyle | GCFont, &gc_setup);
	gc_setup.foreground = bg_pixel;
	gc_setup.background = fg_pixel;
	reversed_gc = XCreateGC(dpy, main_window,
				GCFunction | GCForeground | GCBackground |
				GCFillStyle | GCFont, &gc_setup);
	gc_setup.foreground = ~0UL;
	gc_setup.background = 0UL;
	bw_gc = XCreateGC(dpy, main_window,
			  GCFunction | GCForeground | GCBackground |
			  GCFillStyle | GCFont, &gc_setup);
	gc_setup.background = 0UL;
	gc_setup.foreground = bg_pixel ^ fg_pixel;
	gc_setup.function = GXxor;
	cursor_gc = XCreateGC(dpy, main_window,
			      GCFunction | GCForeground | GCBackground |
			      GCFillStyle, &gc_setup);

	/* Let os_fatal know the graphics contexts have been initialized */
	x_setup.x_active = 1;

	for(;;) {
		XEvent e;
		XNextEvent(dpy, &e);
		if (e.type == MapNotify)
			break;
	}
	bgpm = XCreatePixmap(dpy, main_window, X_WIDTH, X_HEIGHT, DefaultDepth(dpy,DefaultScreen(dpy)));
	XSetWindowBackgroundPixmap(dpy, main_window, bgpm);

	/* Must be after screen dimensions are computed.  */
	if (z_header.version >= V6) {
		if (x_init_pictures()) {
			z_header.config |= CONFIG_PICTURES;
			z_header.flags &= ~MENU_FLAG;
		} else
			z_header.flags &= ~GRAPHICS_FLAG;
	}
} /* os_init_screen */


/*
 * os_reset_screen
 *
 * Reset the screen before the program stops.
 *
 */
void os_reset_screen(void)
{
	os_set_text_style(0);
	print_string("[Hit any key to exit.]");
	flush_buffer();
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
void os_restart_game(int stage)
{
	/* Show Beyond Zork's title screen */
	if ((stage == RESTART_END) && (story_id == BEYOND_ZORK)) {
		int w, h;
		if (os_picture_data(1, &h, &w)) {
			os_draw_picture(1, 1, 1);
			os_read_key(0, 0);
		}
	}
}


/*
 * os_random_seed
 *
 * Return an appropriate random seed value in the range from 0 to
 * 32767, possibly by using the current system time.
 *
 */
int os_random_seed(void)
{
	if (user_random_seed == -1)
		return time(0) & 0x7fff;
	else
		return user_random_seed;
} /* os_random_seed */


void os_init_setup(void)
{
	memset(&x_setup, 0, sizeof(x_setup));
	return;
}


/*
 * os_load_story
 *
 * This is different from os_path_open() because we need to see if the
 * story file is actually a chunk inside a blorb file.  Right now we're
 * looking only at the exact path we're given on the command line.
 *
 */

FILE *os_load_story(void)
{
	FILE *fp;

#ifndef NO_BLORB
        switch (x_blorb_init(f_setup.story_file)) {
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

        fp = fopen(f_setup.story_file, "rb");

        /* Is this a Blorb file containing Zcode? */
        if (f_setup.exec_in_blorb)
                fseek(fp, blorb_res.data.startpos, SEEK_SET);
#else

	fp = fopen(f_setup.story_file, "rb");
#endif
	return fp;
}


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
}


int os_storyfile_tell(FILE * fp)
{
#ifndef NO_BLORB
        /* Is this a Blorb file containing Zcode? */
        if (f_setup.exec_in_blorb)
                return ftell(fp) - blorb_res.data.startpos;
        else
                return ftell(fp);
#else
        return ftell(fp);
#endif
}
