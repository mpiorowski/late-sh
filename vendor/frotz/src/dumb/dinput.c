/*
 * dinput.c - Dumb interface, input functions
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

#include <string.h>
#include <assert.h>

#include "dfrotz.h"

extern f_setup_t f_setup;

static char runtime_usage[] =
	"DUMB-FROTZ runtime help:\n"
	"  General Commands:\n"
	"    \\help    Show this message.\n"
	"    \\set     Show the current values of runtime settings.\n"
	"    \\s       Show the current contents of the whole screen.\n"
	"    \\d       Discard the part of the input before the cursor.\n"
	"    \\wN      Advance clock N/10 seconds, possibly causing the current\n"
	"                and subsequent inputs to timeout.\n"
	"    \\w       Advance clock by the amount of real time since this input\n"
	"                started (times the current speed factor).\n"
	"    \\t       Advance clock just enough to timeout the current input\n"
	"  Reverse-Video Display Method Settings:\n"
	"    \\rn   none    \\rc   CAPS    \\rd   doublestrike    \\ru   underline\n"
	"    \\rbC  show rv blanks as char C (orthogonal to above modes)\n"
	"  Output Compression Settings:\n"
	"    \\cn      none: show whole screen before every input.\n"
	"    \\cm      max: show only lines that have new nonblank characters.\n"
	"    \\cs      spans: like max, but emit a blank line between each span of\n"
	"                screen lines shown.\n"
	"    \\chN     Hide top N lines (orthogonal to above modes).\n"
	"  Misc Settings:\n"
	"    \\sfX     Set speed factor to X.  (0 = never timeout automatically).\n"
	"    \\mp      Toggle use of MORE prompts\n"
	"    \\ln      Toggle display of line numbers.\n"
	"    \\lt      Toggle display of the line type identification chars.\n"
	"    \\vb      Toggle visual bell.\n"
	"    \\pb      Toggle display of picture outline boxes.\n"
	"    (Toggle commands can be followed by a 1 or 0 to set value ON or OFF.)\n"
	"  Character Escapes:\n"
	"    \\\\  backslash    \\#  backspace    \\[  escape    \\_  return\n"
	"    \\< \\> \\^ \\.  cursor motion        \\1 ..\\0  f1..f10\n"
	"    \\D ..\\X   Standard Frotz hotkeys.  Use \\H (help) to see the list.\n"
	"  Line Type Identification Characters:\n"
	"    Input lines:\n"
	"      untimed  timed\n"
	"      >        T      A regular line-oriented input\n"
	"      )        t      A single-character input\n"
	"      }        D      A line input with some input before the cursor.\n"
	"                         (Use \\d to discard it.)\n"
	"    Output lines:\n"
	"      ]     Output line that contains the cursor.\n"
	"      .     A blank line emitted as part of span compression.\n"
	"            (blank) Any other output line.\n"
;

static float speed = 1;

enum input_type {
	INPUT_CHAR,
	INPUT_LINE,
	INPUT_LINE_CONTINUED,
};


/* get a character.  Exit with no fuss on EOF.  */
static int xgetchar(void)
{
	int c = getchar();
	if (c == EOF) {
		if (feof(stdin)) {
			fprintf(stderr, "\nEOT\n");
			os_quit(EXIT_SUCCESS);
		}
#ifdef TOPS20
		/* On TOPS-20 only, the very first getchar() may return EOF,
		 * even thought feof(stdin) is false.  No idea why, but...
		 */
		if (!spurious_getchar) {
			spurious_getchar = TRUE;
			return xgetchar();
		} else {
			os_fatal(strerror(errno));
		}
#else
		os_fatal(strerror(errno));
#endif
	}
	return c;
} /* xgetchar */


/* Read one line, including the newline, into s.  Safely avoids buffer
 * overruns (but that's kind of pointless because there are several
 * other places where I'm not so careful).  */
static void dumb_getline(char *s)
{
	int c;
	char *p = s;
	while (p < s + INPUT_BUFFER_SIZE - 1) {
		if ((*p++ = xgetchar()) == '\n') {
			*p = '\0';
			return;
		}
	}
	p[-1] = '\n';
	p[0] = '\0';
	while ((c = xgetchar()) != '\n')
		;
	printf("Line too long, truncated to %s\n", s - INPUT_BUFFER_SIZE);
} /* dumb_getline */


/* Translate in place all the escape characters in s.  */
static void translate_special_chars(char *s)
{
	char *src = s, *dest = s;
	while (*src)
		switch(*src++) {
		default: *dest++ = src[-1]; break;
		case '\n': *dest++ = ZC_RETURN; break;
		case '\\':
			switch (*src++) {
			case '\n': *dest++ = ZC_RETURN; break;
			case '\\': *dest++ = '\\'; break;
			case '?': *dest++ = ZC_BACKSPACE; break;
			case '[': *dest++ = ZC_ESCAPE; break;
			case '_': *dest++ = ZC_RETURN; break;
			case '^': *dest++ = ZC_ARROW_UP; break;
			case '.': *dest++ = ZC_ARROW_DOWN; break;
			case '<': *dest++ = ZC_ARROW_LEFT; break;
			case '>': *dest++ = ZC_ARROW_RIGHT; break;
			case 'R': *dest++ = ZC_HKEY_RECORD; break;
			case 'P': *dest++ = ZC_HKEY_PLAYBACK; break;
			case 'S': *dest++ = ZC_HKEY_SEED; break;
			case 'U': *dest++ = ZC_HKEY_UNDO; break;
			case 'N': *dest++ = ZC_HKEY_RESTART; break;
			case 'X': *dest++ = ZC_HKEY_QUIT; break;
			case 'D': *dest++ = ZC_HKEY_DEBUG; break;
			case 'H': *dest++ = ZC_HKEY_HELP; break;
			case '1': *dest++ = ZC_FKEY_F1; break;
			case '2': *dest++ = ZC_FKEY_F2; break;
			case '3': *dest++ = ZC_FKEY_F3; break;
			case '4': *dest++ = ZC_FKEY_F4; break;
			case '5': *dest++ = ZC_FKEY_F5; break;
			case '6': *dest++ = ZC_FKEY_F6; break;
			case '7': *dest++ = ZC_FKEY_F7; break;
			case '8': *dest++ = ZC_FKEY_F8; break;
			case '9': *dest++ = ZC_FKEY_F9; break;
#ifdef TOPS20
			case '0': *dest++ = (char) (ZC_FKEY_F10 & 0xff); break;
#else
			case '0': *dest++ = ZC_FKEY_F10; break;
#endif
			default:
				fprintf(stderr, "DUMB-FROTZ: unknown escape char: %c\n", src[-1]);
				fprintf(stderr, "Enter \\help to see the list\n");
			}
		}
	*dest = '\0';
} /* translate_special_chars */


/* The time in tenths of seconds that the user is ahead of z time.  */
static int time_ahead = 0;

/* Called from os_read_key and os_read_line if they have input from
 * a previous call to dumb_read_line.
 * Returns TRUE if we should timeout rather than use the read-ahead.
 * (because the user is further ahead than the timeout).  */
static bool check_timeout(int timeout)
{
	if ((timeout == 0) || (timeout > time_ahead))
		time_ahead = 0;
	else
		time_ahead -= timeout;
	return time_ahead != 0;
} /* check_timeout */


/* If val is '0' or '1', set *var accordingly, otherwise toggle it.  */
static void toggle(bool *var, char val)
{
	*var = val == '1' || (val != '0' && !*var);
} /* toggle */


/* Handle input-related user settings and call dumb_output_handle_setting.  */
bool dumb_handle_setting(const char *setting, bool show_cursor, bool startup)
{
	if (!strncmp(setting, "sf", 2)) {
		speed = atof(&setting[2]);
		printf("Speed Factor %g\n", speed);
	} else if (!strncmp(setting, "mp", 2)) {
		toggle(&do_more_prompts, setting[2]);
		printf("More prompts %s\n", do_more_prompts ? "ON" : "OFF");
	} else {
		if (!strcmp(setting, "set")) {
			printf("Speed Factor %g\n", speed);
			printf("More Prompts %s\n",
				do_more_prompts ? "ON" : "OFF");
		}
		return dumb_output_handle_setting(setting, show_cursor, startup);
	}
	return TRUE;
} /* dumb_handle_setting */


/* Read a line, processing commands (lines that start with a backslash
 * (that isn't the start of a special character)), and write the
 * first non-command to s.
 * Return true if timed-out.  */
static bool dumb_read_line(char *s, char *prompt, bool show_cursor,
			   int timeout, enum input_type type,
			   zchar *continued_line_chars)
{
	time_t start_time;

	start_time = time(0);

	if (timeout) {
		if (time_ahead >= timeout) {
			time_ahead -= timeout;
			return TRUE;
		}
		timeout -= time_ahead;
		start_time = time(0);
	}
	time_ahead = 0;

	dumb_show_screen(show_cursor);
	for (;;) {
		char *command;
		if (prompt)
			fputs(prompt, stdout);
		else
			dumb_show_prompt(show_cursor,
				(timeout ? "tTD" : ")>}")[type]);

		/* Prompt only shows up after user input if we don't flush stdout */
		fflush(stdout);
		dumb_getline(s);
		if ((s[0] != '\\') || ((s[1] != '\0') && !islower(s[1]))) {
			/* Is not a command line.  */
			translate_special_chars(s);
			if (timeout) {
				int elapsed = (time(0) - start_time) * 10 * speed;
				if (elapsed > timeout) {
					time_ahead = elapsed - timeout;
					return TRUE;
				}
			}
			return FALSE;
		}
		/* Commands.  */

		/* Remove the \ and the terminating newline.  */
		command = s + 1;
		command[strlen(command) - 1] = '\0';

		if (!strcmp(command, "t")) {
			if (timeout) {
				time_ahead = 0;
				s[0] = '\0';
				return TRUE;
			}
		} else if (*command == 'w') {
			if (timeout) {
				int elapsed = atoi(&command[1]);
				time_t now = time(0);
				if (elapsed == 0)
					elapsed = (now - start_time) * 10 * speed;
				if (elapsed >= timeout) {
					time_ahead = elapsed - timeout;
					s[0] = '\0';
					return TRUE;
				}
				timeout -= elapsed;
				start_time = now;
			}
		} else if (!strcmp(command, "d")) {
			if (type != INPUT_LINE_CONTINUED)
				fprintf(stderr, "DUMB-FROTZ: No input to discard\n");
			else {
				dumb_discard_old_input(strlen((char*) continued_line_chars));
				continued_line_chars[0] = '\0';
				type = INPUT_LINE;
			}
		} else if (!strcmp(command, "help")) {
			if (!do_more_prompts)
				fputs(runtime_usage, stdout);
			else {
				char *current_page, *next_page;
				current_page = next_page = runtime_usage;
				for (;;) {
					int i;
					for (i = 0; (i < z_header.screen_rows - 2) && *next_page; i++)
						next_page = strchr(next_page, '\n') + 1;
					/* next_page - current_page is width */
					printf("%.*s", (int) (next_page - current_page), current_page);
					current_page = next_page;
					if (!*current_page)
						break;
					printf("HELP: Type <return> for more, or q <return> to stop: ");
					fflush(stdout);
					dumb_getline(s);
					if (!strcmp(s, "q\n"))
						break;
				}
			}
		} else if (!strcmp(command, "s")) {
			dumb_dump_screen();
    		} else if (!dumb_handle_setting(command, show_cursor, FALSE)) {
			fprintf(stderr, "DUMB-FROTZ: unknown command: %s\n", s);
			fprintf(stderr, "Enter \\help to see the list of commands\n");
		}
	}
} /* dumb_read_line */


/* Read a line that is not part of z-machine input (more prompts and
 * filename requests).  */
static void dumb_read_misc_line(char *s, char *prompt)
{
	dumb_read_line(s, prompt, 0, 0, 0, 0);
	/* Remove terminating newline */
	s[strlen(s) - 1] = '\0';
} /* dumb_read_misc_line */


/* For allowing the user to input in a single line keys to be returned
 * for several consecutive calls to read_char, with no screen update
 * in between.  Useful for traversing menus.  */
static char read_key_buffer[INPUT_BUFFER_SIZE];

/* Similar.  Useful for using function key abbreviations.  */
static char read_line_buffer[INPUT_BUFFER_SIZE];

#ifdef USE_UTF8
/* Convert UTF-8 encoded char starting at in[idx] to zchar (UCS-2) if
 * representable in 16 bits or '?' otherwise and return index to next
 * char of input array. */
static int utf8_to_zchar(zchar *out, char *in, int idx)
{
	zchar ch;
	int i;
	if ((in[idx] & 0x80) == 0) {
		ch = in[idx++];
	} else if ((in[idx] & 0xe0) == 0xc0) {
		ch = in[idx++] & 0x1f;
		if ((in[idx] & 0xc0) != 0x80)
			goto error;
		ch = (ch << 6) | (in[idx++] & 0x3f);
	} else if ((in[idx] & 0xf0) == 0xe0) {
		ch = in[idx++] & 0xf;
		for (i = 0; i < 2; i++) {
			if ((in[idx] & 0xc0) != 0x80)
				goto error;
			ch = (ch << 6) | (in[idx++] & 0x3f);
		}
	} else {
		/* Consume all subsequent continuation bytes. */
		while ((in[++idx] & 0xc0) == 0x80)
			;
error:
		ch = '?';
	}
	*out = ch;
	return idx;
} /* utf8_to_zchar */
#endif


zchar os_read_key (int timeout, bool show_cursor)
{
	zchar c;
	int timed_out;
	int idx = 1;

	/* Discard any keys read for line input.  */
	read_line_buffer[0] = '\0';

	if (read_key_buffer[0] == '\0') {
		timed_out = dumb_read_line(read_key_buffer, NULL,
			show_cursor, timeout, INPUT_CHAR, NULL);
		/* An empty input line is reported as a single CR.
		 * If there's anything else in the line, we report
		 * only the line's contents and not the terminating CR.  */
		if (strlen(read_key_buffer) > 1)
			read_key_buffer[strlen(read_key_buffer) - 1] = '\0';
	} else
		timed_out = check_timeout(timeout);

	if (timed_out)
		return ZC_TIME_OUT;

#ifndef USE_UTF8
	c = read_key_buffer[0];
#else
	idx = utf8_to_zchar(&c, read_key_buffer, 0);
#endif
	memmove(read_key_buffer, read_key_buffer + idx,
		strlen(read_key_buffer) - idx + 1);

	/* TODO: error messages for invalid special chars.  */

	return c;
} /* os_read_key */


zchar os_read_line (int UNUSED (max), zchar *buf, int timeout, int UNUSED(width), int continued)
{
	char *p;
	int terminator;
	static bool timed_out_last_time;
	int timed_out;
#ifdef USE_UTF8
	int i, j, len;
#endif

	/* Discard any keys read for single key input.  */
	read_key_buffer[0] = '\0';

	/* After timing out, discard any further input unless
	 * we're continuing.  */
	if (timed_out_last_time && !continued)
		read_line_buffer[0] = '\0';

	if (read_line_buffer[0] == '\0') {
		timed_out = dumb_read_line(read_line_buffer, NULL, TRUE,
			timeout, buf[0] ? INPUT_LINE_CONTINUED : INPUT_LINE,
			buf);
	} else
		timed_out = check_timeout(timeout);

	if (timed_out) {
		timed_out_last_time = TRUE;
		return ZC_TIME_OUT;
	}

	/* find the terminating character.  */
	for (p = read_line_buffer;; p++) {
		if (is_terminator(*p)) {
			terminator = *p;
			*p++ = '\0';
			break;
		}
	}

	/* TODO: Truncate to width and max.  */

	/* copy to screen */
	dumb_display_user_input(read_line_buffer);

	/* copy to the buffer and save the rest for next time.  */
#ifndef USE_UTF8
	strncat((char*) buf, (char *) read_line_buffer,
		INPUT_BUFFER_SIZE - strlen(read_line_buffer) - 1);
#else
	for (len = 0;; len++) {
		if (!buf[len])
			break;
	}
	j = 0;
	for (i = len; i < INPUT_BUFFER_SIZE - 2; i++) {
		if (!read_line_buffer[j])
			break;
		j = utf8_to_zchar(&buf[i], read_line_buffer, j);
	}
	buf[i] = 0;
#endif
	p = read_line_buffer + strlen(read_line_buffer) + 1;
	memmove(read_line_buffer, p, strlen(p) + 1);

	/* If there was just a newline after the terminating character,
	 * don't save it.  */
	if ((read_line_buffer[0] == '\r') && (read_line_buffer[1] == '\0'))
		read_line_buffer[0] = '\0';

	timed_out_last_time = FALSE;
	return terminator;
} /* os_read_line */


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
 * Return value is NULL if there was a problem.
 */
char *os_read_file_name (const char *default_name, int flag)
{
	static char file_name[FILENAME_MAX + 1];
	char prompt[INPUT_BUFFER_SIZE];

	char fullpath[INPUT_BUFFER_SIZE];
	char *buf;

	FILE *fp;
	char *tempname;
	char path_separator[2];
	int i;
	char *ext;
	bool freebuf = FALSE;

	path_separator[0] = PATH_SEPARATOR;
	path_separator[1] = 0;

	/* If we're restoring a game before the interpreter starts,
 	 * our filename is already provided.  Just go ahead silently.
	 */
	if (f_setup.restore_mode) {
		/* Caller always strdups */
		return (char*)default_name;
	} else if (flag == FILE_NO_PROMPT) {
		ext = strrchr(default_name, '.');
		if (strncmp(ext, EXT_AUX, 4)) {
			os_warn("Blocked unprompted access of %s. Should only be %s files.", default_name, EXT_AUX);
			free(ext);
			return NULL;
		}
		free(ext);
		if (f_setup.restricted_path == NULL)
			buf = strndup(default_name, MAX_FILE_NAME - EXT_LENGTH);
		else
			buf = basename((char *)default_name);
	} else {
		if (f_setup.restricted_path != NULL) {
			sprintf(prompt, "Please enter a filename [%s]: ", basename((char *)default_name));
		} else
			sprintf(prompt, "Please enter a filename [%s]: ", default_name);

		dumb_read_misc_line(fullpath, prompt);

		/* If using default filename... */
		freebuf = TRUE;
		if (strlen(fullpath) == 0)
			buf = strndup(default_name, MAX_FILE_NAME);
		else /* Using supplied filename... */
			buf = strndup(fullpath, MAX_FILE_NAME);

		if (strlen(buf) > MAX_FILE_NAME) {
			printf("Filename too long\n");
			free(buf);
			return NULL;
		}
	}

	assert(buf);
	strncpy(file_name, buf, FILENAME_MAX);
	if (freebuf) free(buf);

	/* Check if we're restricted to one directory. */
	if (f_setup.restricted_path != NULL) {
		for (i = strlen(file_name); i > 0; i--) {
			if (file_name[i] == PATH_SEPARATOR) {
				i++;
				break;
			}
		}
		tempname = strdup(file_name + i);
		strncpy(file_name, f_setup.restricted_path, FILENAME_MAX);

		/* Make sure the final character is the path separator. */
		if (file_name[strlen(file_name)-1] != PATH_SEPARATOR) {
			strncat(file_name, path_separator, FILENAME_MAX - strlen(file_name) - 2);
		}
		strncat(file_name, tempname, strlen(file_name) - strlen(tempname) - 1);
	}

	/* Add appropriate filename extensions if not already there. */
	ext = strrchr(file_name, '.');
	if (flag == FILE_SAVE || flag == FILE_RESTORE) {
		if (ext == NULL || strncmp(ext, EXT_SAVE, EXT_LENGTH) != 0) {
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

	/* Warn if overwriting a file.  */
	if ((flag == FILE_SAVE || flag == FILE_SAVE_AUX || flag == FILE_RECORD)
		&& ((fp = fopen(file_name, "rb")) != NULL)) {
		fclose (fp);
		dumb_read_misc_line(fullpath, "Overwrite existing file? ");
		if (tolower(fullpath[0]) != 'y')
			return NULL;
	}
	return file_name;
} /* os_read_file_name */


void os_more_prompt (void)
{
	if (do_more_prompts) {
		char buf[INPUT_BUFFER_SIZE];
		dumb_read_misc_line(buf, "***MORE***");
	} else
		dumb_elide_more_prompt();
} /* os_more_prompt */


void dumb_init_input(void)
{
	if ((z_header.version >= V4) && (speed != 0))
		z_header.config |= CONFIG_TIMEDINPUT;

	if (z_header.version >= V5)
		z_header.flags &= ~(MOUSE_FLAG | MENU_FLAG);
} /* dumb_init_input */


zword os_read_mouse(void)
{
	/* NOT IMPLEMENTED */
	return 0;
} /* os_read_mouse */

void os_tick(void)
{
	/* Nothing here yet */
} /* os_tick */
