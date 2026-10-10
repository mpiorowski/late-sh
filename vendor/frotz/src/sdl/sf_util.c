/*
 * sf_util.c - SDL interface, startup functions
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

#include "sf_frotz.h"

#include <ctype.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

#include <libgen.h>

#include <SDL.h>
#include <zlib.h>

#ifdef __WIN32__
#include <io.h>
#endif

#ifdef UNIX
#include <unistd.h>
#include <getopt.h>
#endif

extern f_setup_t f_setup;

typedef void (*CLEANFUNC)();

typedef struct cfstruct cfrec;

static void print_version(void);

enum { USAGE_NORMAL, USAGE_EXTENDED };

struct cfstruct {
	CLEANFUNC func;
	cfrec *next;
	const char *name;
};

static cfrec *cflist = NULL;

static int getcolor(char *);


void sf_regcleanfunc(void *f, const char *p)
{
	cfrec *n = calloc(1, sizeof(cfrec));
	if (n) {
		if (!p)
			p = "";
		n->func = (CLEANFUNC) f;
		n->name = p;
		n->next = cflist;
		cflist = n;
	}
} /* sf_regcleanfunc */


void sf_cleanup_all(void)
{
	while (cflist) {
		cfrec *n = cflist->next;
		if (cflist->func)
			cflist->func();
		free(cflist);
		cflist = n;
	}
} /* sf_cleanup_all */


/*
 * os_reset_screen
 *
 * Reset the screen before the program ends.
 *
 */
void os_reset_screen(void)
{
	sf_flushdisplay();

	const char *hit = sf_msgstring(IDS_HIT_KEY_EXIT);
	os_set_font(TEXT_FONT);
	os_set_text_style(0);
	screen_new_line();

	while (*hit)
		os_display_char((*hit++));
	sf_read_key(0, true, false, false);

	sf_cleanup_all();
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


int user_background = -1;
int user_foreground = -1;
int user_emphasis = -1;
int user_bold_typing = -1;
int user_reverse_bg = -1;
int user_reverse_fg = -1;
int user_screen_height = -1;
int user_screen_width = -1;
int user_font = 1;
int m_random_seed = -1;
int m_fullscreen = -1;
int m_reqW = 0, m_reqH = 0;
int m_vga_fonts = 0;
extern char m_names_format;
static char user_names_format = 0;
extern char *m_reslist_file;
extern int option_scrollback_buffer;
extern bool option_disable_color;

static char *info_header =
    "FROTZ V%s - SDL graphics and audio interface.\n"
    "An interpreter for all Infocom and other Z-Machine games.\n\n"
    "Syntax: sfrotz [options] story-file [blorb file]\n";

static char *info[] = {
	"-a   watch attribute setting",
	"-A   watch attribute testing",
	"-b <colourname> background colour",
	"-c # context lines",
	"-C <file> load this configuration file",
	"-d   disable color",
	"-f <colorname> foreground colour",
	"-F   fullscreen mode",
	"-H # screen height",
	"-i   ignore runtime errors",
	"-I # interpreter number",
	"-l # left margin",
	"-L <file> load this save file",
	"-o   watch object movement",
	"-O   watch object locating",
	"-P   alter piracy opcode",
	"-q   quiet (disable sound)",
	"-r # right margin",
	"-s # random number seed value",
	"-S # transcript width",
	"-t   set Tandy bit",
	"-u # slots for multiple undo",
	"-W # screen width",
	"-x   expand abbreviations g/x/z",
	"-X   show extended options",
	"-v   show version information",
	"-Z # error checking (see below)",
	NULL
};

static char *info_footer =
    "\nError checking: 0 none, 1 first only (default), 2 all, 3 exit after any error.";

static char *extended_header = "\nExtended Options\n";

static char *extended_options[] = {
	"-@ <file> use resources in <file>",
	"-%        use local resources",
	"-F        run in fullscreen mode.",
	"-m #      set timer interrupt cycle to # milliseconds",
	"-N <mode> add date or number to save filename",
	"-T        use in-game requests for filenames",
	"-V        force VGA fonts",
	NULL
};

static char *footer =
    "More options and information are in the manual page.  Type \"man sfrotz\".\n";


#define WIDCOL 42
static void usage(int type)
{
	char **p;
	int i = 0, len = 0;

	printf(info_header, VERSION);

	if (type == USAGE_NORMAL)
		p = info;
	else {
		p = extended_options;
		puts(extended_header);
	}

	while (*p) {
		if (i && type == USAGE_NORMAL) {
			while (len > 0) {
				fputc(' ', stdout);
				len--;
			}
			puts(*p);
		} else {
			fputs("  ", stdout);
			fputs(*p, stdout);
			if (type == USAGE_NORMAL)
				len = WIDCOL - strlen(*p) - 2;
			else
				fputs("\n", stdout);
		}
		i = 1 - i;
		p++;
	}
	if (i)
		fputc('\n', stdout);

	if (type == USAGE_NORMAL)
		puts(info_footer);
	puts(footer);

} /* usage */


/*
 * parse_options
 *
 * Parse program options and set global flags accordingly.
 *
 */

static const char *progname = NULL;

extern char *optarg;
extern int optind;

extern int m_timerinterval;

double m_xscale = 0;
double m_yscale = 0;

static int xscale_flag = 0;
static int yscale_flag = 0;

static char *options = "@:%aAb:B:c:C:df:FH:iI:l:L:m:N:oOPqr:s:S:tTu:vVW:xXZ:";
static struct option long_options[] = {
	{"xscale", required_argument, &xscale_flag, 1},
	{"yscale", required_argument, &yscale_flag, 1},
	{0, 0, 0, 0}
};

static int limit(int v, int m, int M)
{
	if (v < m)
		return m;
	if (v > M)
		return M;
	return v;
} /* limit */


static void parse_options(int argc, char **argv)
{
	int c;

	do {
		int num = 0, copt = 0;;
		int option_index = 0;

		c = getopt_long(argc, argv, options, long_options, &option_index);

		if (optarg != NULL) {
			num = atoi(optarg);
			copt = optarg[0];
		}

		if (c == 0) {
			if (xscale_flag)
				m_xscale = atof(optarg);
			if (yscale_flag)
				m_yscale = atof(optarg);
		}
		if (c == '%')
			m_localfiles = true;
		if (c == 'a')
			f_setup.attribute_assignment = 1;
		if (c == 'A')
			f_setup.attribute_testing = 1;
		if (c == 'b')
			user_background = getcolor(optarg);
		if (c == 'B')
			option_scrollback_buffer = num;
		if (c == 'c')
			f_setup.context_lines = num;
		if (c == 'C') {
			/* originally allocated in os_init_setup() */
			free(f_setup.config_file);
			f_setup.config_file = strdup(optarg);
		}
		if (c == 'd')
			option_disable_color = 1;
		if (c == 'm')
			m_timerinterval = limit(num, 10, 1000000);
		if (c == 'N')
			user_names_format = copt;
		if (c == '@')
			m_reslist_file = optarg;
		if (c == 'f')
			user_foreground = getcolor(optarg);
		if (c == 'F')
			m_fullscreen = 1;
		if (c == 'H')
			user_screen_height = num;
		if (c == 'i')
			f_setup.ignore_errors = 1;
		if (c == 'I') {
			f_setup.interpreter_number = num;
			z_header.interpreter_number = num;
		}
		if (c == 'l')
			f_setup.left_margin = num;
		if (c == 'L') {
			f_setup.restore_mode = TRUE;
			f_setup.tmp_save_name = strdup(optarg);
		}
		if (c == 'q')
			m_no_sound = 1;
		if (c == 'o')
			f_setup.object_movement = 1;
		if (c == 'O')
			f_setup.object_locating = 1;
		if (c == 'P')
			f_setup.piracy = 1;
		if (c == 'r')
			f_setup.right_margin = num;
		if (c == 's')
			m_random_seed = num;
		if (c == 'S')
			f_setup.script_cols = num;
		if (c == 't')
			f_setup.tandy = 1;
		if (c == 'T')
			sf_osdialog = NULL;
		if (c == 'u')
			f_setup.undo_slots = num;
		if (c == 'v')
			print_version();
		if (c == 'V')
			m_vga_fonts = 1;
		if (c == 'W')
			user_screen_width = num;
		if (c == 'x')
			f_setup.expand_abbreviations = 1;
		if (c == 'X') {
			usage(USAGE_EXTENDED);
			os_quit(EXIT_SUCCESS);
		}
		if (c == 'Z')
			if (num >= ERR_REPORT_NEVER && num <= ERR_REPORT_FATAL)
				f_setup.err_report_mode = num;
		if (c == '?') {
			usage(USAGE_NORMAL);
			os_quit(EXIT_FAILURE);
		}
	} while (c != EOF && c != '?');
} /* parse_options */


static void print_version(void)
{
	printf("FROTZ V%s     SDL interface.\n", VERSION);
	printf("Commit date:    %s\n", GIT_DATE);
	printf("Git commit:     %s\n", GIT_HASH);
	printf("Notes:          %s\n", RELEASE_NOTES);
	printf("  Frotz was originally written by Stefan Jokisch.\n");
	printf("  It complies with standard 1.1 of the Z-Machine Standard.\n");
	printf("  It was ported to Unix by Galen Hazelwood.\n");
	printf("  It is distributed under the GNU General Public License version 2 or\n");
	printf("    (at your option) any later version.\n");
	printf("  This software is offered as-is with no warranty or liability.\n");
	printf("  The core and SDL port are maintained by David Griffith.\n");
	printf("  Frotz's homepage is https://661.org/proj/if/frotz/\n\n");
	os_quit(EXIT_SUCCESS);
} /* print_version */


/**
 * Like dirname except well defined.
 * Does not modify path.  Always returns a new string (caller must free).
 */
static char *new_dirname(const char *path)
{
	char *p = strdup(path), *p2 = strdup(dirname(p));
	free(p);
	return p2;
} /* new_dirname */


/**
 * Like basename except well defined.
 * Does not modify path.  Always returns a new string (caller must free).
 */
static char *new_basename(const char *path)
{
	char *p = strdup(path), *p2 = strdup(basename(p));
	free(p);
	return p2;
} /* new_basename */


/*
 * os_process_arguments
 *
 * Handle command line switches.
 * Some variables may be set to activate special features of Frotz.
 *
 */
void os_process_arguments(int argc, char *argv[])
{
	char *p;

	optarg = NULL;

	sf_installhandlers();

	/* Parse command line first because we might be told to load a
	 * different config file.  Then read from that config file.
	 * Finally we parse the command line again because options
	 * specified there must override the config file.
	 */
	parse_options(argc, argv);
	sf_readsettings();
	parse_options(argc, argv);

	if (argv[optind] == NULL) {
		usage(USAGE_NORMAL);
		os_quit(EXIT_SUCCESS);
	}
	f_setup.story_file = strdup(argv[optind]);

	if (argv[optind + 1] != NULL)
		f_setup.blorb_file = argv[optind + 1];

	/* Strip path and extension off the story file name */
	f_setup.story_name = new_basename(f_setup.story_file);

	/* Now strip off the extension. */
	p = strrchr(f_setup.story_name, '.');
	if ((p != NULL) &&
	    ((strcmp(p, EXT_BLORB2) == 0) ||
	     (strcmp(p, EXT_BLORB3) == 0) || (strcmp(p, EXT_BLORB4) == 0))) {
		/*  blorb_ext = strdup(p); */
	} else
		/*  blorb_ext = strdup(EXT_BLORB); */

		/* Get rid of extensions with 1 to 6 character extensions. */
		/* This will take care of an extension like ".zblorb". */
		/* More than that, there might be something weird going on */
		/* which is not our concern. */
	if (p != NULL) {
		if (strlen(p) >= 2 && strlen(p) <= 7) {
			*p = '\0';	/* extension removed */
		}
	}
	f_setup.story_path = new_dirname(argv[optind]);

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
	} else {	/* Set our auto load save as the name_save */
		f_setup.save_name = malloc((strlen(f_setup.tmp_save_name) + strlen(EXT_SAVE) + 1) * sizeof(char));
		memcpy(f_setup.save_name, f_setup.tmp_save_name, (strlen(f_setup.story_name) + strlen(EXT_SAVE)) * sizeof(char));
		free(f_setup.tmp_save_name);
	}

	f_setup.aux_name = malloc((strlen(f_setup.story_name) + strlen(EXT_AUX) + 1) * sizeof(char));
	memcpy(f_setup.aux_name, f_setup.story_name, (strlen(f_setup.story_name) + strlen(EXT_AUX)) * sizeof(char));
	strncat(f_setup.aux_name, EXT_AUX, strlen(EXT_AUX) + 1);

	/* Save the executable file name */
	progname = argv[0];

	if (user_screen_width > 0)
		AcWidth = user_screen_width;
	if (user_screen_height > 0)
		AcHeight = user_screen_height;

	if (user_names_format)
		m_names_format = user_names_format;

	if (user_background != -1)
		m_defaultBack = sf_GetColour(user_background);
	if (user_foreground != -1)
		m_defaultFore = sf_GetColour(user_foreground);

	sf_initfonts();
} /* os_process_arguments */


#ifdef WIN32
#include <windows.h>
#else
#include <time.h>
#include <sys/time.h>
#endif

void sf_sleep(int msecs)
{
	SDL_Delay(msecs);
} /* sf_sleep */


#ifdef WIN32
zlong sf_ticks(void)
{
	return (GetTickCount());
}
#else
zlong sf_ticks(void)
{
	struct timeval now;
	static struct timeval start;
	static int started = 0;
	zlong ticks;
	now.tv_sec = now.tv_usec = 0;
	gettimeofday(&now, NULL);
	if (!started) {
		started = 1;
		start = now;
	}
	ticks =
	    (now.tv_sec - start.tv_sec) * 1000 + (now.tv_usec -
						  start.tv_usec) / 1000;
	return ticks;
} /* sf_ticks */
#endif


static char *getextension(int flag)
{
	char *ext = EXT_AUX;

	if (flag == FILE_SAVE || flag == FILE_RESTORE)
		ext = EXT_SAVE;
	else if (flag == FILE_SCRIPT)
		ext = EXT_SCRIPT;
	else if (flag == FILE_RECORD || flag == FILE_PLAYBACK)
		ext = EXT_COMMAND;

	return ext;
} /* getextension */


static bool newfile(int flag)
{
	if (flag == FILE_SAVE || flag == FILE_SAVE_AUX || flag == FILE_RECORD)
		return true;
	return false;
} /* newfile */


static char buf[FILENAME_MAX];

static const char *getnumbername(const char * UNUSED(def), char *ext)
{
	int len, number = 0;
	strcpy(buf, f_setup.story_name);
	len = strlen(buf);
	for (;;) {
		sprintf(buf + len, "%03d%s", number++, ext);
		if (access(buf, F_OK))
			break;
	}
	return buf;
} /* getnumbername */


static const char *getdatename(const char * UNUSED (def), char *ext)
{
	int len;

	time_t t;
	struct tm *tm;
	time(&t);
	tm = localtime(&t);

	strcpy(buf, f_setup.story_name);
	len = strlen(buf);
	sprintf(buf + len, "%04d%02d%02d%02d%02d%s",
		tm->tm_year + 1900, tm->tm_mon + 1, tm->tm_mday,
		tm->tm_hour, tm->tm_min, ext);
	return buf;
} /* getdatename */


static int ingame_read_file_name(char *file_name, const char *default_name,
				 int flag);
static int dialog_read_file_name(char *file_name, const char *default_name,
				 int flag);

/*
 * os_read_file_name
 *
 * Return the name of a file. Flag can be one of:
 *
 *    FILE_SAVE      - Save game file
 *    FILE_RESTORE   - Restore game file
 *    FILE_SCRIPT    - Transcript file
 *    FILE_RECORD    - Command file for recording
 *    FILE_PLAYBACK  - Command file for playback
 *    FILE_SAVE_AUX  - Save auxiliary ("preferred settings") file
 *    FILE_LOAD_AUX  - Load auxiliary ("preferred settings") file
 *    FILE_NO_PROMPT - Return file without prompting the user
 *
 * The length of the file name is limited by MAX_FILE_NAME. Ideally
 * an interpreter should open a file requester to ask for the file
 * name. If it is unable to do that then this function should call
 * print_string and read_string to ask for a file name.
 *
 * Return value is NULL is there was a problem
 */
char *os_read_file_name(const char *default_name, int flag)
{
	int st;
	const char *initname = default_name;
	char file_name[FILENAME_MAX + 1];
	char *ext;

	if (newfile(flag)) {
		char *flag_ext = getextension(flag);
		if (m_names_format == 'd')
			initname = getdatename(initname, flag_ext);
		else if (m_names_format == 'n')
			initname = getnumbername(initname, flag_ext);
	}

	/* If we're restoring a game before the interpreter starts,
	 * and our filename is already provided with the -L flag,
	 * just go ahead silently.
	 */
	if (flag == FILE_NO_PROMPT) {
		const char *ext = strrchr(default_name, '.');
		if (strncmp(ext, EXT_AUX, 4)) {
			os_warn("Blocked unprompted access of %s. Should only be %s files.", default_name, EXT_AUX);
			return NULL;
		} else
			strncpy(file_name, default_name, FILENAME_MAX);
	} else if (f_setup.restore_mode) {
		strncpy(file_name, f_setup.save_name, FILENAME_MAX);
	} else {
		st = dialog_read_file_name(file_name, initname, flag);
		if (st == SF_NOTIMP)
			st = ingame_read_file_name(file_name, initname, flag);

		if (!st)
			return NULL;

		/* Add appropriate filename extensions if not already there. */
		ext = strrchr(file_name, '.');
		if (flag == FILE_SAVE || flag == FILE_RESTORE) {
			if (ext == NULL ||  (strncmp(ext, EXT_SAVE, EXT_LENGTH) != 0) ) {
				strncat(file_name, EXT_SAVE, EXT_LENGTH);
			}
		} else if (flag == FILE_SCRIPT) {
			if (ext == NULL || strncmp(ext, EXT_SCRIPT, EXT_LENGTH) != 0) {
				strncat(file_name, EXT_SCRIPT, EXT_LENGTH);
			}
		} else if (flag == FILE_SAVE_AUX || flag == FILE_LOAD_AUX) {
			if (ext == NULL || strncmp(ext, EXT_AUX, EXT_LENGTH) != 0) {
				strncat(file_name, EXT_AUX, EXT_LENGTH);
			}
		} else if (flag == FILE_RECORD || flag == FILE_PLAYBACK) {
			if (ext == NULL || strncmp(ext, EXT_COMMAND, EXT_LENGTH) != 0) {
				strncat(file_name, EXT_COMMAND, EXT_LENGTH);
			}
		}
	}
	return strdup(file_name);
} /* os_read_file_name */


static int ingame_read_file_name(char *file_name, const char *default_name,
				 int flag)
{
	char *extension;
	FILE *fp;
	bool terminal;
	bool result;

	bool saved_replay = istream_replay;
	bool saved_record = ostream_record;

	/* Turn off playback and recording temporarily */
	istream_replay = FALSE;
	ostream_record = FALSE;

	/* Select appropriate extension */
	extension = getextension(flag);

	/* Input file name (reserve four bytes for a file name extension) */
	print_string("Enter file name (\"");
	print_string(extension);
	print_string("\" will be added).\nDefault is \"");
	print_string(default_name);
	print_string("\": ");

#ifdef USE_UTF8
	{
		zchar z_name[FILENAME_MAX + 1];
		zchar *zp;
		int i = 0;
		read_string(FILENAME_MAX - EXT_LENGTH - 4, z_name);
		zp = z_name;
		while (*zp) {
			if (*zp <= 0x7f) {
				if (i > FILENAME_MAX - EXT_LENGTH - 4)
					break;
				file_name[i++] = *zp;
			} else if (*zp > 0x7ff) {
				if (i > FILENAME_MAX - EXT_LENGTH - 6)
					break;
				file_name[i++] = 0xe0 | ((*zp >> 12) & 0xf);
				file_name[i++] = 0x80 | ((*zp >> 6) & 0x3f);
				file_name[i++] = 0x80 | (*zp & 0x3f);
			} else {
				if (i > FILENAME_MAX - EXT_LENGTH - 5)
					break;
				file_name[i++] = 0xc0 | ((*zp >> 6) & 0x1f);
				file_name[i++] = 0x80 | (*zp & 0x3f);
			}
			zp++;
		}
		file_name[i] = 0;
	}
#else
	read_string(MAX_FILE_NAME - EXT_LENGTH - 4, (zchar *) file_name);
#endif

	/* Use the default name if nothing was typed */
	if (file_name[0] == 0)
		strcpy(file_name, default_name);
	if (strchr(file_name, '.') == NULL)
		strcat(file_name, extension);

	/* Make sure it is safe to use this file name */
	result = TRUE;

	/* OK if the file is opened for reading */
	if (!newfile(flag))
		goto finished;

	/* OK if the file does not exist */
	if ((fp = fopen(file_name, "rb")) == NULL)
		goto finished;

	/* OK if this is a pseudo-file (like PRN, CON, NUL) */
	terminal = isatty(fileno(fp));

	fclose(fp);

	if (terminal)
		goto finished;

	/* OK if user wants to overwrite */
	result = read_yes_or_no("Overwrite existing file");

 finished:

	/* Restore state of playback and recording */
	istream_replay = saved_replay;
	ostream_record = saved_record;

	return result;
} /* ingame_read_file_name */


static int dialog_read_file_name(char *file_name, const char *default_name,
				 int flag)
{
	int filter = 0;
	int title = 0, st;
	char *res;

	sf_flushdisplay();

	switch (flag) {
	case FILE_SAVE:
		filter = IDS_SAVE_FILTER;
		title = IDS_SAVE_TITLE;
		break;
	case FILE_RESTORE:
		filter = IDS_SAVE_FILTER;
		title = IDS_RESTORE_TITLE;
		break;
	case FILE_SCRIPT:
		filter = IDS_SCRIPT_FILTER;
		title = IDS_SCRIPT_TITLE;
		break;
	case FILE_RECORD:
		filter = IDS_RECORD_FILTER;
		title = IDS_RECORD_TITLE;
		break;
	case FILE_PLAYBACK:
		filter = IDS_RECORD_FILTER;
		title = IDS_PLAYBACK_TITLE;
		break;
	case FILE_SAVE_AUX:
		filter = IDS_AUX_FILTER;
		title = IDS_SAVE_AUX_TITLE;
		break;
	case FILE_LOAD_AUX:
		filter = IDS_AUX_FILTER;
		title = IDS_LOAD_AUX_TITLE;
		break;
	default:
		return 0;
	}

	st = sf_user_fdialog(!newfile(flag), default_name, sf_msgstring(filter),
			     sf_msgstring(title), &res);
	if (st == SF_NOTIMP)
		return st;
	if (st == 0) {
		strncpy(file_name, res, FILENAME_MAX);
		file_name[FILENAME_MAX] = 0;
		return 1;
	}
	return 0;
} /* dialog_read_file_name */

static char *rc = NULL;

void sf_FinishProfile(void)
{
	if (!rc)
		return;
	free(rc);
	rc = NULL;
} /* sf_FinishProfile */


void sf_InitProfile(const char *fn)
{
	FILE *f;
	int size;
	char *s, *d;
	char *my_fn;
	char *homedir;
	int len;

	if (!fn)
		return;

	homedir = strdup(getenv(HOMEDIR));
	len = ((strlen(homedir) + strlen(fn) + 1) * sizeof(char)) + 1;
	my_fn = malloc(len);
	snprintf(my_fn, len, "%s/%s", homedir, fn);

	f = fopen(fn, "rb");
	if (!f) {
		f = fopen(my_fn, "rb");
		if (!f)
			return;
	}
	fseek(f, 0, SEEK_END);
	size = ftell(f);
	if (!size) {
		fclose(f);
		return;
	}
	rc = malloc(size + 1);
	if (!rc) {
		fclose(f);
		return;
	}
	fseek(f, 0, 0);
	fread(rc, 1, size, f);
	fclose(f);
	rc[size] = 0;

	s = d = rc;

	while (*s) {
		if (*s == '#') {
			while ((*s) && (*s != '\n'))
				s++;
			if (!*s)
				break;
		} else
			*d++ = *s++;
	}
	*d = 0;

	CLEANREG(sf_FinishProfile);
} /* sf_InitProfile */


static char *findsect(const char *sect)
{
	int ns = strlen(sect);
	char *r = rc;
	while (r) {
		r = strchr(r, '[');
		if (!r)
			return NULL;
		r++;
		if (strncmp(r, sect, ns))
			continue;
		return (r + ns);
	}
	return NULL;
} /* findsect */


static char *findid(const char *sect, const char *id)
{
	int nid = strlen(id);
	char *r, *sav, *rq, *fnd = NULL;
	r = findsect(sect);
	if (!r)
		return NULL;
	sav = strchr(r, '[');
	if (sav)
		*sav = 0;
	while (r) {
		r = strstr(r, id);
		if (!r)
			break;
		rq = r + nid;
		if ((*(zbyte *) (r - 1) <= ' ')
		    && ((*rq == ' ') || (*rq == '='))) {
			while (*rq)
				if (*rq++ == '=')
					break;
			if (*rq) {
				fnd = rq;
				break;
			}
		}
		r = rq;
	}
	if (sav)
		*sav = '[';
	return fnd;
} /* findid */


int sf_GetProfileInt(const char *sect, const char *id, int def)
{
	if (rc) {
		char *p = findid(sect, id);
		if (p)
			def = atoi(p);
	}
	return def;
} /* sf_GetProfileInt */


double sf_GetProfileDouble(const char *sect, const char *id, double def)
{
	if (rc) {
		char *p = findid(sect, id);
		if (p)
			def = atof(p);
	}
	return def;
} /* sf_GetProfileDouble */


char *sf_GetProfileString(const char *sect, const char *id, char *def)
{
	char *q = NULL, sav = 0;
	if (rc) {
		char *p = findid(sect, id);
		if (p) {
			int quoted = 0;
			for (; *p; p++) {
				if (*p == '\"') {
					quoted = 1;
					p++;
					break;
				}
				if ((zbyte) (*p) > ' ')
					break;
			}
			if (*p) {
				if (quoted) {
					q = strchr(p, '\"');
					*q = 0;
				}
				if (!q) {
					q = p;
					while (*q >= ' ')
						q++;
					sav = *q;
					*q = 0;
				}
			}
			def = p;
		}
	}
	if (def)
		def = strdup(def);
	if (sav)
		*q = sav;
	return def;
} /* sf_GetProfileString */


/*  A.  Local file header:
 *
 *         local file header signature   0  4 bytes  (0x04034b50)
 *         version needed to extract     4  2 bytes
 *         general purpose bit flag      6  2 bytes
 *         compression method            8  2 bytes
 *         last mod file time           10  2 bytes
 *         last mod file date           12  2 bytes
 *         crc-32                       14  4 bytes
 *         compressed size              18  4 bytes
 *         uncompressed size            22  4 bytes
 *         file name length             26  2 bytes
 *         extra field length           28  2 bytes
 *
 *         file name (variable size)
 *         extra field (variable size)
 */

#define plong( b) (((int)((b)[3]) << 24) + ((int)((b)[2]) << 16) +\
	((int)((b)[1]) << 8) + (int)((b)[0]))

#define pshort( b) (((int)((b)[1]) << 8) + (int)((b)[0]))


static unsigned myin(void * UNUSED (d), zbyte ** UNUSED (b))
{
	return 0;
} /* myin */


static int myout(void *udata, zbyte * b, unsigned n)
{
	memmove(udata, b, n);
	udata += n;
	return 0;
} /* myout */


static int myunzip(int csize, zbyte * cdata, zbyte * udata)
{
	zbyte window[32768];
	z_stream z;
	int st;

	memset(&z, 0, sizeof(z));

	st = inflateBackInit(&z, 15, window);
	if (st)
		return st;

	z.next_in = cdata;
	z.avail_in = csize;

	for (;;) {
		st = inflateBack(&z, myin, NULL, myout, udata);
		if (st == Z_STREAM_END)
			break;
		if (st)
			return st;
	}

	st = inflateBackEnd(&z);
	return st;
} /* myunzip */


int sf_pkread(FILE * f, int foffs, void **out, int *size)
{
	zbyte hd[30];
	zbyte *data, *cdata;
	int csize, usize, cmet, skip, st;

	fseek(f, foffs, SEEK_SET);
	fread(hd, 1, 30, f);
	cmet = pshort(hd + 8);
	if (cmet != 8)
		return -10;
	csize = plong(hd + 18);
	usize = plong(hd + 22);
	if (csize <= 0)
		return -11;
	if (usize <= 0)
		return -12;
	data = malloc(usize);
	if (!data)
		return -13;
	cdata = malloc(csize);
	if (!cdata) {
		free(data);
		return -14;
	}
	skip = pshort(hd + 26) + pshort(hd + 28);
	fseek(f, foffs + 30 + skip, SEEK_SET);
	fread(cdata, 1, csize, f);

	st = myunzip(csize, cdata, data);

	free(cdata);
	if (st) {
		free(data);
		return st;
	}
	*out = (void *)data;
	*size = usize;
	return st;
} /* sf_pkread */


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


/************************/

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <sys/stat.h>
#ifdef WIN32
#include <windows.h>
#else
#include <unistd.h>
#endif

#ifndef WIN32
#define _stat stat
#endif
