/*
 * dosinput.c - DOS interface, input functions
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

#include <bios.h>

#ifdef __WATCOMC__
#include <io.h>
#endif

#include <string.h>
#include <stdio.h>
#include "frotz.h"
#include "dosfrotz.h"

#ifndef HISTORY_BUFSIZE
#define HISTORY_BUFSIZE 500
#endif

extern bool is_terminator (zchar);

extern bool read_yes_or_no (const char *);
extern void read_string (int, zchar *);

extern int completion (const zchar *, zchar *);

extern bool at_keybrd;

static long limit = 0;

static struct {
	zchar buffer[HISTORY_BUFSIZE];
	int latest;
	int current;
	int prefix_len;
} history;

static struct {
	zchar *buffer;
	int pos;
	int length;
	int max_length;
	int width;
	int max_width;
} input;

static bool overwrite = FALSE;

volatile int end_of_sound_flag;


/*
 * swap_colours
 *
 * This is a small helper function for switch_cursor. It swaps the
 * current background and foreground colours.
 *
 */
static void swap_colours(void)
{
    byte temp;

    temp = text_fg;
    text_fg = text_bg;
    text_bg = temp;
} /* swap_colours */


/*
 * switch_cursor
 *
 * Turn cursor on/off. If there is mouse support then turn the mouse
 * pointer on/off as well. The cursor should not be moved and the
 * contents of the screen should not be changed while the cursor is
 * visible (because of the primitive cursor emulation we use here).
 *
 */
static void switch_cursor(bool cursor)
{
	if (display <= _TEXT_) {
		/* Use hardware cursor in text mode */
#ifdef __WATCOMC__
		word shape;
		if (display == _MONO_)
			shape = overwrite ? 0x080f : 0x0a0b;
		else
			shape = overwrite ? 0x0408 : 0x0506;

		if (!cursor)
			shape = 0xffff;

		bios_video_ah_bh_dh_dl(2, 0, cursor_y, cursor_x);
		bios_video_ah_cx(1, shape);
#else
		if (display == _MONO_)
			_CX = overwrite ? 0x080f : 0x0a0b;
		else
			_CX = overwrite ? 0x0408 : 0x0506;

		if (!cursor)
			_CX = 0xffff;

		asm mov ah,2
		asm mov bh,0
		asm mov dh,byte ptr cursor_y
		asm mov dl,byte ptr cursor_x
		asm int 0x10
		asm mov ah,1
		asm int 0x10
#endif
	} else {
		int saved_x = cursor_x;

		if (cursor)
			swap_colours();

		if (input.pos < input.length)
			os_display_char(input.buffer[input.pos]);
		else
			os_display_char(' ');

		if (cursor)
			swap_colours();

		cursor_x = saved_x;
	}
} /* switch_cursor */


/*
 * get_current_time
 *
 * Return the current system time in 1/10 seconds.
 *
 */
static long get_current_time(void)
{
	long time;

	/* Get the current time of day measured in
	 * 65536 / 1,193,180 = 0.054925493
	 * seconds. Multiply this value with
	 * 959 / 1746 = 0.54925544
	 * to get the current time in 0.1 seconds.
	 */
#ifdef __WATCOMC__
	time = bios_time_ah(0);
#else
	asm mov ah,0
	asm int 0x1a
	asm mov word ptr time,dx
	asm mov word ptr time + 2,cx
#endif
	return time * 959 / 1746;
} /* get_current_time */


/*
 * set_timer
 *
 * Set a time limit of timeout/10 seconds if timeout is not zero;
 * otherwise clear the time limit.
 *
 */
static void set_timer(int timeout)
{
	limit = (timeout != 0) ? get_current_time () + timeout : 0;
} /* set_timer */


/*
 * time_limit_hit
 *
 * Return true if a previously set time limit has been exceeded.
 *
 */
static bool out_of_time(void)
{
	if (limit != 0) {
		long now = get_current_time ();

	if (now < 1L * 3600 * 10 && limit > 23L * 3600 * 10)
		now += 24L * 3600 * 10;

		return now >= limit;
	}
	return FALSE;
} /* out_of_time */


/*
 * get_key
 *
 * Read a keypress or a mouse click. Returns...
 *
 *	ZC_TIME_OUT = time limit exceeded,
 *	ZC_BACKSPACE = the backspace key,
 *	ZC_RETURN = the return key,
 *	ZC_HKEY_MIN...ZC_HKEY_MAX = a hot key,
 *	ZC_ESCAPE = the escape key,
 *	ZC_ASCII_MIN...ZC_ASCII_MAX = ASCII character,
 *	ZC_ARROW_MIN...ZC_ARROW_MAX = an arrow key,
 *	ZC_FKEY_MIN...ZC_FKEY_MAX = a function key,
 *	ZC_NUMPAD_MIN...ZC_NUMPAD_MAX = a number pad key,
 *	ZC_SINGLE_CLICK = single mouse click,
 *	ZC_DOUBLE_CLICK = double mouse click,
 *	ZC_LATIN1_MIN+1...ZC_LATIN1_MAX = ISO Latin-1 character,
 *	SPECIAL_KEY_MIN...SPECIAL_KEY_MAX = a special editing key.
 *
 */
static int get_key(bool cursor)
{
	static byte arrow_key_map[] = {0x48, 0x50, 0x4b, 0x4d};
	static byte special_key_map[] = {0x47, 0x4f, 0x73, 0x74, 0x53,
					0x52, 0x49, 0x51, 0x0f};
	static byte hot_key_map[] = {0x13, 0x19, 0x1f, 0x16,
				     0x31, 0x2d, 0x20, 0x23};
	static byte function_key_map[] = {0x3B, 0x3C, 0x3D, 0x3E, 0x3F, 0x40,
	                                  0x41, 0x42, 0x43, 0x44, 0x85, 0x86};

	int key;

	/* Loop until a key was pressed */
	if (cursor)
		switch_cursor(TRUE);

#ifdef __WATCOMC__
	if (z_header.flags & MOUSE_FLAG)
		dos_mouse_ax(1);
#else
	if (z_header.flags & MOUSE_FLAG) {
		asm mov ax,1
		asm int 0x33
	}
#endif
	do {

#ifndef NO_SOUND
		if (end_of_sound_flag)
			end_of_sound();
#endif

		if (_bios_keybrd(at_keybrd ? _NKEYBRD_READY : _KEYBRD_READY)) {
			word code = _bios_keybrd(at_keybrd ? _NKEYBRD_READ : _KEYBRD_READ);
			byte code0 = code;
			byte code1 = code >> 8;

			if (code0 != 0 && code0 != 9 && code0 != 0xe0) {
				key = code0 - '0' + ZC_NUMPAD_MIN;
				if (key >= ZC_NUMPAD_MIN && key <= ZC_NUMPAD_MAX && code1 >= 0x10)
					goto exit_loop;

				for (key = ZC_LATIN1_MIN + 1; key <= ZC_LATIN1_MAX; key++) {
					if (code0 == latin1_to_ibm[key - ZC_LATIN1_MIN])
						goto exit_loop;
				}
				key = code0;

				if (key == ZC_BACKSPACE)
					goto exit_loop;
				if (key == ZC_RETURN)
					goto exit_loop;
				if (key == ZC_ESCAPE)
					goto exit_loop;
				if (key >= ZC_ASCII_MIN && key <= ZC_ASCII_MAX)
					goto exit_loop;
			} else {
				for (key = ZC_ARROW_MIN; key <= ZC_ARROW_MAX; key++) {
					if (code1 == arrow_key_map[key - ZC_ARROW_MIN])
						goto exit_loop;
				}

				for (key = ZC_FKEY_MIN; key <= ZC_FKEY_MAX; key++) {
					if (code1 == function_key_map[key - ZC_FKEY_MIN])
						goto exit_loop;
				}

				for (key = ZC_HKEY_MIN; key <= ZC_HKEY_MAX; key++) {
					if (code1 == hot_key_map[key - ZC_HKEY_MIN])
						goto exit_loop;
				}

				for (key = SPECIAL_KEY_MIN; key <= SPECIAL_KEY_MAX; key++) {
					if (code1 == special_key_map[key - SPECIAL_KEY_MIN])
						goto exit_loop;
				}
			}
		} else {
			int clicks = read_mouse ();

			if (clicks == 1) {
				key = ZC_SINGLE_CLICK;
				goto exit_loop;
			}
			if (clicks == 2) {
				key = ZC_DOUBLE_CLICK;
				goto exit_loop;
			}
		}

		key = ZC_TIME_OUT;
	} while (!out_of_time());

exit_loop:

#ifdef __WATCOMC__
	if (z_header.flags & MOUSE_FLAG)
		dos_mouse_ax(2);
#else
	if (z_header.flags & MOUSE_FLAG) {
		asm mov ax,2
		asm int 0x33
	}
#endif
	if (cursor)
		switch_cursor(FALSE);

	return key;
} /* get_key */


/*
 * cursor_left
 *
 * Move the cursor one character to the left.
 *
 */
static void cursor_left(void)
{
	if (input.pos > 0)
		cursor_x -= os_char_width(input.buffer[--input.pos]);
} /* cursor_left */


/*
 * cursor_right
 *
 * Move the cursor one character to the right.
 *
 */
static void cursor_right(void)
{
	if (input.pos < input.length)
		cursor_x += os_char_width(input.buffer[input.pos++]);
} /* cursor_right */


/*
 * first_char
 *
 * Move the cursor to the beginning of the input line.
 *
 */
static void first_char(void)
{
	while (input.pos > 0)
		cursor_left();
} /* first_char */


/*
 * last_char
 *
 * Move the cursor to the end of the input line.
 *
 */
static void last_char(void)
{
	while (input.pos < input.length)
		cursor_right();
} /* last_char */


/*
 * prev_word
 *
 * Move the cursor to the start of the previous word.
 *
 */
static void prev_word(void)
{
	do {
		cursor_left();
		if (input.pos == 0)
			return;
	} while (input.buffer[input.pos] == ' ' || input.buffer[input.pos - 1] != ' ');
} /* prev_word */


/*
 * next_word
 *
 * Move the cursor to the start of the next word.
 *
 */
static void next_word(void)
{
	do {
		cursor_right();
		if (input.pos == input.length)
			return;
	} while (input.buffer[input.pos] == ' ' || input.buffer[input.pos - 1] != ' ');
} /* next_word */


/*
 * input_move
 *
 * Helper function to move parts of the input buffer:
 *
 *    newc != 0, oldc == 0: INSERT
 *    newc != 0, oldc != 0: OVERWRITE
 *    newc == 0, oldc != 0: DELETE
 *    newc == 0, oldc == 0: NO OPERATION
 *
 */
#define H(x) (x ? 1 : 0)
static void input_move(zchar newc, zchar oldc)
{
	int newwidth = (newc != 0) ? os_char_width(newc) : 0;
	int oldwidth = (oldc != 0) ? os_char_width(oldc) : 0;

	zchar *p = input.buffer + input.pos;

	int saved_x = cursor_x;

	int updated_width = input.width + newwidth - oldwidth;
	int updated_length = input.length + H (newc) - H (oldc);

	if (updated_width > input.max_width)
		return;
	if (updated_length > input.max_length)
		return;

	input.width = updated_width;
	input.length = updated_length;

	if (oldc != 0 && newc == 0)
		memmove(p, p + 1, updated_length - input.pos + 1);
	if (newc != 0 && oldc == 0)
		memmove(p + 1, p, updated_length - input.pos);

	if (newc != 0)
		*p = newc;

	os_display_string(p);

	switch_scrn_attr(TRUE);

	if (oldwidth > newwidth) {
		os_erase_area (
		    cursor_y + 1,
		    cursor_x + 1,
		    cursor_y + z_header.font_height,
		    cursor_x + oldwidth - newwidth,
		    -1);
	}

	switch_scrn_attr(FALSE);

	cursor_x = saved_x;

	if (newc != 0)
		cursor_right();
} /* input_move */
#undef H


/*
 * delete_char
 *
 * Delete the character below the cursor.
 *
 */
static void delete_char(void)
{
	input_move (0, input.buffer[input.pos]);
} /* delete_char */


/*
 * delete_left
 *
 * Delete the character to the left of the cursor.
 *
 */
static void delete_left(void)
{
	if (input.pos > 0) {
		cursor_left();
		delete_char();
	}
} /* delete_left */


/*
 * truncate_line
 *
 * Truncate the input line to n characters.
 *
 */
static void truncate_line(int n)
{
	last_char();
	while (input.length > n)
		delete_left();
} /* truncate_line */


/*
 * insert_char
 *
 * Insert a character into the input buffer.
 *
 */
static void insert_char(zchar newc)
{
	zchar oldc = 0;

	if (overwrite)
		oldc = input.buffer[input.pos];

	input_move(newc, oldc);
} /* insert_char */


/*
 * insert_string
 *
 * Add a string of characters to the input line.
 *
 */
static void insert_string(const zchar *s)
{
	while (*s != 0) {
		if (input.length + 1 > input.max_length)
			break;
		if (input.width + os_char_width (*s) > input.max_width)
			break;
		insert_char (*s++);
	}
} /* insert_string */


/*
 * tabulator_key
 *
 * Complete the word at the end of the input line, if possible.
 *
 */
static void tabulator_key(void)
{
	int status;

	if (input.pos == input.length) {
		zchar extension[10];

		status = completion(input.buffer, extension);
		insert_string(extension);
	} else
		status = 2;

	/* Beep if the completion was impossible or ambiguous */
	if (status != 0)
		os_beep(status);
} /* tabulator_key */


/*
 * store_input
 *
 * Copy the current input line to the history buffer.
 *
 */
static void store_input(void)
{
	if (input.length >= HISTORY_MIN_ENTRY) {

		const zchar *ptr = input.buffer;

		do {
			if (history.latest++ == HISTORY_BUFSIZE - 1)
				history.latest = 0;

			history.buffer[history.latest] = *ptr;

		} while (*ptr++ != 0);
	}
} /* store_input */


/*
 * fetch_entry
 *
 * Copy the current history entry to the input buffer and check if it
 * matches the prefix in the input buffer.
 *
 */
static bool fetch_entry(zchar *buf, int entry)
{
	int i = 0;
	zchar c;

	do {
		if (entry++ == HISTORY_BUFSIZE - 1)
			entry = 0;

		c = history.buffer[entry];

		if (i < history.prefix_len && input.buffer[i] != c)
			return FALSE;

		buf[i++] = c;
	} while (c != 0);

	return (i > history.prefix_len) && (i > 1);
} /* fetch_entry */


/*
 * get_prev_entry
 *
 * Copy the previous history entry to the input buffer.
 *
 */
static void get_prev_entry(void)
{
	zchar buf[INPUT_BUFFER_SIZE];

	int i = history.current;

	do {
		do {
			if (i-- == 0)
				i = HISTORY_BUFSIZE - 1;

			if (i == history.latest)
				return;

		} while (history.buffer[i] != 0);

	} while (!fetch_entry(buf, i));

	truncate_line(history.prefix_len);
	insert_string(buf + history.prefix_len);
	history.current = i;
} /* get_prev_entry */


/*
 * get_next_entry
 *
 * Copy the next history entry to the input buffer.
 *
 */
static void get_next_entry(void)
{
	zchar buf[INPUT_BUFFER_SIZE];

	int i = history.current;

	truncate_line(history.prefix_len);

	do {
		do {
			if (i == history.latest)
				return;

			if (i++ == HISTORY_BUFSIZE - 1)
				i = 0;
		} while (history.buffer[i] != 0);
		if (i == history.latest)
			goto no_further;
	} while (!fetch_entry (buf, i));

	insert_string(buf + history.prefix_len);

no_further:
	history.current = i;
} /* get_next_entry */


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
 *     ZC_HKEY_PLAYBACK (Alt-P)
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
#define new_history_search() \
    { history.prefix_len = input.pos; history.current = history.latest; }

zchar os_read_line(int max, zchar *buf, int timeout, int width, int continued)
{
	int key = continued ? 9999 : 0;

	input.buffer = buf;
	input.pos = strlen((char *) buf);
	input.length = strlen((char *) buf);
	input.max_length = max;
	input.width = os_string_width(buf);
	input.max_width = width - os_char_width(' ');

	/* Calculate time limit */
	set_timer (timeout);

	/* Loop until a terminator is found */
	do {
		if (key != 9999)
			new_history_search();

		/* Get next key from mouse or keyboard */
		key = get_key (TRUE);

		if (key < ZC_ASCII_MIN || key > ZC_ASCII_MAX && key < ZC_LATIN1_MIN || key > ZC_LATIN1_MAX) {
			/* Ignore time-outs if the cursor is not at end of the line */
			if (key == ZC_TIME_OUT && input.pos < input.length)
				key = 9999;

			/* Backspace, return and escape keys */
			if (key == ZC_BACKSPACE)
				delete_left();
			if (key == ZC_RETURN)
				store_input();
			if (key == ZC_ESCAPE)
				truncate_line(0);

			/* Editing keys */
			if (cwin == 0) {
				if (key == ZC_ARROW_UP)
					get_prev_entry();
				if (key == ZC_ARROW_DOWN)
					get_next_entry();
				if (key == ZC_ARROW_LEFT)
					cursor_left();
				if (key == ZC_ARROW_RIGHT)
					cursor_right();
				if (key >= ZC_ARROW_MIN && key <= ZC_ARROW_MAX)
					key = 9999;
				if (key == SPECIAL_KEY_HOME)
					first_char();
				if (key == SPECIAL_KEY_END)
					last_char();
				if (key == SPECIAL_KEY_WORD_LEFT)
					prev_word();
				if (key == SPECIAL_KEY_WORD_RIGHT)
					next_word();
				if (key == SPECIAL_KEY_DELETE)
					delete_char();
				if (key == SPECIAL_KEY_INSERT)
					overwrite = !overwrite;
				if (key == SPECIAL_KEY_TAB)
					tabulator_key();
			}
			if (key == SPECIAL_KEY_PAGE_UP)
				key = ZC_ARROW_UP;
			if (key == SPECIAL_KEY_PAGE_DOWN)
				key = ZC_ARROW_DOWN;
		} else
			insert_char(key);
	} while (key > 0xff || !is_terminator(key));

	last_char();

	overwrite = FALSE;

	/* Return terminating key */
	return key;
} /* os_read_line */

#ifdef __WATCOMC__
#undef new_history_search
#else
#undef new_history_search()
#endif


/*
 * os_read_key
 *
 * Read a single character from the keyboard (or a mouse click) and
 * return it. Input aborts after timeout/10 seconds.
 *
 */
zchar os_read_key(int timeout, bool cursor)
{
	int key;

	set_timer(timeout);

	do {
		key = get_key(cursor);
	} while (key > 0xff);

	return key;
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
 *    FILE_NO_PROMPT - Return file without prompting the user
 *
 * The length of the file name is limited by MAX_FILE_NAME. Ideally
 * an interpreter should open a file requester to ask for the file
 * name. If it is unable to do that then this function should call
 * print_string and read_string to ask for a file name.
 *
 */
char *os_read_file_name (const char *default_name, int flag)
{
	char *extension;
	FILE *fp;
	bool terminal;
	bool result;

	bool saved_replay = istream_replay;
	bool saved_record = ostream_record;

	char* basename;
	size_t prefix_len;
	char file_name[FILENAME_MAX + 1];
	char *ext;

	/* Turn off playback and recording temporarily */
	istream_replay = FALSE;
	ostream_record = FALSE;

	/* Select appropriate extension */
	extension = EXT_AUX;

	if (flag == FILE_SAVE || flag == FILE_RESTORE)
		extension = EXT_SAVE;
	if (flag == FILE_SCRIPT)
		extension = EXT_SCRIPT;
	if (flag == FILE_RECORD || flag == FILE_PLAYBACK)
		extension = EXT_COMMAND;

	file_name[0] = 0;
	if (flag != FILE_NO_PROMPT) {
		/* Input file name (reserve four bytes for a file name extension) */
		print_string("Enter file name (\"");
		print_string(extension);
		print_string("\" will be added).\nDefault is \"");
		print_string(default_name);
		print_string("\": ");
		read_string(MAX_FILE_NAME - 4, (zchar *) file_name);
	}

	/* Use the default name if nothing was typed */
	if (file_name[0] == 0)
		strcpy(file_name, default_name);

	/* Find last path separator in file name */
	for (basename = (file_name + strlen(file_name) - 1);
		 basename != file_name;
		 basename--) {
		if ((*basename == OS_PATH_SEPARATOR) ||
			(*basename == OS_ALT_PATH_SEPARATOR))
		{
			basename++;
			break;
		}
	}
	/* If there's no dot anywhere after the last path separator,
	   add the file extension */
	if (strchr(basename, '.') == NULL)
		strcat(basename, extension);

	/* Check if we're restricted to one directory. */
	if (f_setup.restricted_path && *f_setup.restricted_path) {
		prefix_len = strlen(f_setup.restricted_path);
		if ((f_setup.restricted_path[prefix_len - 1] != OS_PATH_SEPARATOR) &&
			(f_setup.restricted_path[prefix_len - 1] != OS_ALT_PATH_SEPARATOR))
		{
			prefix_len++;
		}
		/* Make sure we have space in the string for the user-input basename
		   plus the restricted path prefix we'll be inserting */
		if ((prefix_len + strlen(basename)) > FILENAME_MAX)
		{
			/* Gotta bail out... */
			os_warn("Pathname too long");
			return NULL;
		}
		/* Move basename to its new position */
		memmove(file_name + prefix_len, basename, (strlen(basename) + 1));
		/* Apply prefix and path separator */
		strcpy(file_name, f_setup.restricted_path);
		file_name[prefix_len - 1] = OS_PATH_SEPARATOR;
	}

	if (flag == FILE_NO_PROMPT) {
		ext = strrchr(file_name, '.');
		if (strncmp(ext, EXT_AUX, 4)) {
			os_warn("Blocked unprompted access of %s. Should only be %s files.", file_name, EXT_AUX);
			return NULL;
		}
	}
	/* Make sure it is safe to use this file name */
	result = TRUE;

	/* OK if the file is opened for reading */
	if (flag != FILE_SAVE && flag != FILE_SAVE_AUX && flag != FILE_RECORD)
		goto finished;

	/* OK if the file does not exist */
	if ((fp = fopen(file_name, "rb")) == NULL)
		goto finished;

	/* OK if this is a pseudo-file (like PRN, CON, NUL) */

#ifdef __WATCOMC__
	terminal = isatty(fileno(fp));
#else
	terminal = fp->flags & _F_TERM;
#endif
	fclose(fp);

	if (terminal)
		goto finished;

	/* OK if user wants to overwrite */
	result = read_yes_or_no ("Overwrite existing file");

finished:

	/* Restore state of playback and recording */
	istream_replay = saved_replay;
	ostream_record = saved_record;

	if (!result)
		return NULL;

	return strdup(file_name);
} /* os_read_file_name */


/*
 * Called regularly by the interpreter, at least every few instructions
 * (only when interpreting: e.g., not when waiting for input).
 */
void os_tick(void)
{
	/* do nothing */
} /* os_tick */
