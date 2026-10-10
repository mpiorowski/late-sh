/*
 * x_input.c
 *
 * X interface, input functions
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
#include <X11/Xutil.h>
#define XK_MISCELLANY
#include <X11/keysymdef.h>
#undef XK_MISCELLANY
#include <libgen.h>
#include <stdio.h>
#include <string.h>
#include <ctype.h>

extern bool is_terminator(zchar key);
extern void x_del_char(zchar c);
extern Pixmap bgpm;


/*
 * os_read_line
 *
 * Read a line of input from the keyboard into a buffer. The buffer
 * may already be primed with some text. In this case, the "initial"
 * text is already displayed on the screen. After the input action
 * is complete, the function returns with the terminating key value.
 * The length of the input should not exceed "max" characters plus
 * an extra 0 terminator.
 *
 * Terminating keys are the return key (13) and all function keys
 * (see the Specification of the Z-machine) which are accepted by
 * the is_terminator function. Mouse clicks behave like function
 * keys except that the mouse position is stored in global variables
 * "mouse_x" and "mouse_y" (top left coordinates are (1,1)).
 *
 * Furthermore, Frotz introduces some special terminating keys:
 *
 *     ZC_HKEY_KEY_PLAYBACK (Alt-P)
 *     ZC_HKEY_RECORD (Alt-R)
 *     ZC_HKEY_SEED (Alt-S)
 *     ZC_HKEY_UNDO (Alt-U)
 *     ZC_HKEY_RESTART (Alt-N, "new game")
 *     ZC_HKEY_QUIT (Alt-X, "exit game")
 *     ZC_HKEY_DEBUGGING (Alt-D)
 *     ZC_HKEY_HELP (Alt-H)
 *
 * If the timeout argument is not zero, the input gets interrupted
 * after timeout/10 seconds (and the return value is 0).
 *
 * The complete input line including the cursor must fit in "width"
 * screen units.
 *
 * The function may be called once again to continue after timeouts,
 * misplaced mouse clicks or hot keys. In this case the "continued"
 * flag will be set. This information can be useful if the interface
 * implements input line history.
 *
 * The screen is not scrolled after the return key was pressed. The
 * cursor is at the end of the input line when the function returns.
 *
 * Since Inform 2.2 the helper function "completion" can be called
 * to implement word completion (similar to tcsh under Unix).
 *
 */
zchar os_read_line(int max, zchar * buf, int timeout, int UNUSED (width), int UNUSED (continued))
{
	zchar c;
	int index;

	index = 0;
	while (buf[index] != 0)
		index++;
	for (;;) {
		c = os_read_key(timeout, TRUE);
		if (is_terminator(c))
			return c;
		if (c == ZC_BACKSPACE) {
			if (index == 0)
				os_beep(1);
			else {
				index--;
				x_del_char(buf[index]);
				buf[index] = 0;
			}
		} else if (index == max)
			os_beep(1);
		else {
			os_display_char(c);
			buf[index++] = c;
			buf[index] = 0;
		}
	}
} /* os_read_line */


/*
 * os_read_key
 *
 * Read a single character from the keyboard (or a mouse click) and
 * return it. Input aborts after timeout/10 seconds.
 *
 */
zchar os_read_key(int UNUSED (timeout), bool cursor)
{
	XEvent ev;
	char result[2];
	int size;
	KeySym symbol;
	int text_height;
	static Time lastclicktime=0;

	text_height = current_font_info->ascent + current_font_info->descent;
	if (cursor)
		XFillRectangle(dpy, main_window, cursor_gc,
			       curr_x, curr_y, 2, text_height);
	XCopyArea(dpy, main_window, bgpm, current_gc, 0, 0, X_WIDTH, X_HEIGHT, 0, 0);
	for (;;) {
		XNextEvent(dpy, &ev);
		if (ev.type == KeyPress) {
			XKeyEvent *key_ev = (XKeyEvent *) & ev;

			if (cursor)
				XFillRectangle(dpy, main_window, cursor_gc,
					       curr_x, curr_y, 2, text_height);
			size = XLookupString(key_ev, result, sizeof result,
					     &symbol, NULL);
			if (size == 1) {
				if (key_ev->state & Mod1Mask)
					switch (result[0]) {
					case 'r':
						return ZC_HKEY_RECORD;
					case 'p':
						return ZC_HKEY_PLAYBACK;
					case 's':
						return ZC_HKEY_SEED;
					case 'u':
						return ZC_HKEY_UNDO;
					case 'n':
						return ZC_HKEY_RESTART;
					case 'x':
						return ZC_HKEY_QUIT;
					case 'd':
						return ZC_HKEY_DEBUG;
					case 'h':
						return ZC_HKEY_HELP;
					default:
						os_beep(1);
				} else
					return result[0] == 10 ? 13 : result[0];
			} else {
				switch (symbol) {
				case XK_Left:
				case XK_KP_Left:
					return ZC_ARROW_LEFT;
				case XK_Up:
				case XK_KP_Up:
					return ZC_ARROW_UP;
				case XK_Right:
				case XK_KP_Right:
					return ZC_ARROW_RIGHT;
				case XK_Down:
				case XK_KP_Down:
					return ZC_ARROW_DOWN;
				default:
					if (symbol >= XK_F1 && symbol <= XK_F12)
						return (ZC_FKEY_MIN - XK_F1) +
						    symbol;
				}
			}
			if (cursor)
				XFillRectangle(dpy, main_window, cursor_gc,
					       curr_x, curr_y, 2, text_height);
		} else if (ev.type == ButtonPress) {
			XButtonEvent *button_ev = (XButtonEvent *) & ev;

			if (cursor)
				XFillRectangle(dpy, main_window, cursor_gc,
					       curr_x, curr_y, 2, text_height);
			mouse_x = button_ev->x + 1;
			mouse_y = button_ev->y + 1;

			if (button_ev->time - lastclicktime < DCLICKTIME)
				return ZC_DOUBLE_CLICK;

			lastclicktime = button_ev->time;
			return ZC_SINGLE_CLICK;
		}
	}
} /* os_read_key */


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
 *    FILE_NO_PROMPT - Return file without prompting the use
 *
 * The length of the file name is limited by MAX_FILE_NAME. Ideally
 * an interpreter should open a file requester to ask for the file
 * name. If it is unable to do that then this function should call
 * print_string and read_string to ask for a file name.
 *
 * Return value is NULL if there was a problem.
 *
 */
extern void read_string(int, zchar *);

char *os_read_file_name(const char *default_name, int flag)
{
	FILE *fp;
	int saved_replay = istream_replay;
	int saved_record = ostream_record;
	char file_name[FILENAME_MAX + 1];
	zchar read_name[FILENAME_MAX + 1];
	int i;
	char *tempname;
	zchar answer[4];
	char path_separator[2];
	char *ext;

	path_separator[0] = PATH_SEPARATOR;
	path_separator[1] = 0;

	/* Turn off playback and recording temporarily */
	istream_replay = 0;
	ostream_record = 0;

	if (f_setup.restore_mode || flag == FILE_NO_PROMPT) {
		file_name[0] = 0;
	} else {
		print_string("Enter a file name.\nDefault is \"");
		print_string(default_name);
		print_string("\": ");

		read_string(FILENAME_MAX - EXT_LENGTH, read_name);
		for (i = 0; i <= FILENAME_MAX - EXT_LENGTH; i++) {
			file_name[i] = translate_from_zscii(read_name[i]);
			if (file_name[i] == 0) break;
		}
	}

	/* Use the default name if nothing was typed */
	if (file_name[0] == 0) {
		/* If FILE_NO_PROMPT, restrict to currect directory. */
		/* If FILE_NO_PROMPT and using restricted path, then */
		/*   nothing more needs to be done to restrict the   */
		/*   file access there. */
		if (flag == FILE_NO_PROMPT && f_setup.restricted_path == NULL) {
			tempname = basename((char *)default_name);
			strncpy(file_name, tempname, FILENAME_MAX);
		} else
			strcpy(file_name, default_name);
	}

	/* If we're restricted to one directory, strip any leading path left
	 * over from a previous call to os_read_file_name(), then prepend
	 * the prescribed path to the filename.  Hostile leading paths won't
	 * get this far.  Instead we return failure a few lines above here if
	 * someone tries it.
	 */
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
	} else if (flag == FILE_NO_PROMPT) {
		ext = strrchr(file_name, '.');
		if (strncmp(ext, EXT_AUX, 4)) {
			os_warn("Blocked unprompted access of %s. Should only be %s files.", file_name, EXT_AUX);
			return NULL;
		}
	}

	/* Warn if overwriting a file. */
	if ((flag == FILE_SAVE || flag == FILE_SAVE_AUX ||
	    flag == FILE_RECORD || flag == FILE_SCRIPT)
	    && ((fp = fopen(file_name, "rb")) != NULL)) {
		fclose (fp);
		print_string("Overwrite existing file? ");
		read_string(4, answer);
		if (tolower(answer[0] != 'y'))
		return NULL;
	}

	/* Restore state of playback and recording */
	istream_replay = saved_replay;
	ostream_record = saved_record;

	return strdup(file_name);
} /* os_read_file_name */


void os_tick(void)
{
}
