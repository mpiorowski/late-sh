/* input.c - High level input functions
 *	Copyright (c) 1995-1997 Stefan Jokisch
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
 */

#include "frotz.h"

extern int save_undo(void);

extern zchar stream_read_key(zword, zword, bool);
extern zchar stream_read_input(int, zchar *, zword, zword, bool, bool);

extern void tokenise_line(zword, zword, zword, bool);
zword unicode_tolower(zword);
static bool truncate_question_mark(void);


/*
 * is_terminator
 *
 * Check if the given key is an input terminator.
 *
 */
bool is_terminator(zchar key)
{
	if (key == ZC_TIME_OUT)
		return TRUE;
	if (key == ZC_RETURN)
		return TRUE;
	if (key >= ZC_HKEY_MIN && key <= ZC_HKEY_MAX)
		return TRUE;

	if (z_header.terminating_keys != 0) {
		if (key >= ZC_ARROW_MIN && key <= ZC_MENU_CLICK) {
			zword addr = z_header.terminating_keys;
			zbyte c;
			do {
				LOW_BYTE(addr, c);
				if (c == 255 || key == translate_from_zscii(c))
					return TRUE;
				addr++;
			} while (c != 0);
		}
	}
	return FALSE;
} /* is_terminator */


/*
 * z_make_menu, add or remove a menu and branch if successful.
 *
 * 	zargs[0] = number of menu
 *	zargs[1] = table of menu entries or 0 to remove menu
 *
 */
void z_make_menu(void)
{
	/* This opcode was only used for the Macintosh version of Journey.
	 * It controls menus with numbers greater than 2 (menus 0, 1 and 2
	 * are system menus). Frotz doesn't implement menus yet.
	 */
	branch(FALSE);
} /* z_make_menu */


/*
 * read_yes_or_no
 *
 * Ask the user a question; return true if the answer is yes.
 *
 */
bool read_yes_or_no(const char *s)
{
	zchar key;

	print_string(s);
	print_string("? (y/n) >");

	key = stream_read_key(0, 0, FALSE);

	if (key == 'y' || key == 'Y') {
		print_string("y\n");
		return TRUE;
	} else {
		print_string("n\n");
		return FALSE;
	}
} /* read_yes_or_no */


/*
 * read_string
 *
 * Read a string from the current input stream.
 *
 */

void read_string (int max, zchar *buffer)
{
	zchar key;

 	buffer[0] = 0;

	do {
		key = stream_read_input(max, buffer, 0, 0, FALSE, FALSE);
	} while (key != ZC_RETURN);
} /* read_string */


/*
 * read_number
 *
 * Ask the user to type in a number and return it.
 *
 */
int read_number(void)
{
	zchar buffer[6];
	int value = 0;
	int i;

	read_string(5, buffer);

	for (i = 0; buffer[i] != 0; i++) {
		if (buffer[i] >= '0' && buffer[i] <= '9')
			value = 10 * value + buffer[i] - '0';
	}
	return value;
} /* read_number */


/*
 * z_read, read a line of input and (in V5+) store the terminating key.
 *
 *	zargs[0] = address of text buffer
 *	zargs[1] = address of token buffer
 *	zargs[2] = timeout in tenths of a second (optional)
 *	zargs[3] = packed address of routine to be called on timeout
 *
 */
void z_read(void)
{
	zchar buffer[INPUT_BUFFER_SIZE];
	zword addr;
	zchar key;
	zbyte max, size;
	zbyte c;
	int i;

	if (f_setup.err_report_repeat > 0) {
		runtime_error_repeat(f_setup.err_report_repeat);
		f_setup.err_report_repeat = 0;
	}

	/* Supply default arguments */
	if (zargc < 4)
		zargs[3] = 0;

	/* Get maximum input size */
	addr = zargs[0];
	LOW_BYTE (addr, max);
	if (z_header.version <= V4)
		max--;
	if (max >= INPUT_BUFFER_SIZE)
		max = INPUT_BUFFER_SIZE - 1;

	/* Get initial input size */
	if (z_header.version >= V5) {
		addr++;
		LOW_BYTE(addr, size);
	} else size = 0;

	/* Copy initial input to local buffer */
	for (i = 0; i < size; i++) {
		addr++;
		LOW_BYTE(addr, c);
		buffer[i] = translate_from_zscii(c);
	}
	buffer[i] = 0;

	/* Draw status line for V1 to V3 games */
	if (z_header.version <= V3)
		z_show_status();

	/* Read input from current input stream */
#ifdef LATE_ZORK
	/* Modified 2026-10-09: checkpoint decoded READ, not the SAVE branch. */
	door_input();
#endif
	key = stream_read_input (
		max, buffer,		/* buffer and size */
		zargs[2],		/* timeout value   */
		zargs[3],		/* timeout routine */
		TRUE,	        	/* enable hot keys */
		z_header.version == V6);	/* no script in V6 */

#ifdef LATE_ZORK
	door_input_done();
#endif

	if (key == ZC_BAD)
		return;

	/* Perform save_undo for V1 to V4 games */
	if (z_header.version <= V4)
		save_undo();

	/* Copy local buffer back to dynamic memory */
	for (i = 0; buffer[i] != 0; i++) {
		if (key == ZC_RETURN) {
			buffer[i] = unicode_tolower(buffer[i]);
		}
		if (truncate_question_mark() && buffer[i] == '?')
			buffer[i] = ' ';
		storeb((zword) (zargs[0] + ((z_header.version <= V4) ? 1 : 2) + i),
		    translate_to_zscii (buffer[i]));
	}

	/* Add null character (V1-V4) or write input length into 2nd byte */
	if (z_header.version <= V4)
		storeb((zword) (zargs[0] + 1 + i), 0);
	else
		storeb((zword) (zargs[0] + 1), i);

	/* Tokenise line if a token buffer is present */
	if (key == ZC_RETURN && zargs[1] != 0)
		tokenise_line (zargs[0], zargs[1], 0, FALSE);

	/* Store key */
	if (z_header.version >= V5)
		store(translate_to_zscii (key));
} /* z_read */


/*
 * z_read_char, read and store a key.
 *
 *	zargs[0] = input device (must be 1)
 *	zargs[1] = timeout in tenths of a second (optional)
 *	zargs[2] = packed address of routine to be called on timeout
 *
 */

void z_read_char(void)
{
	zchar key;


        if (f_setup.err_report_repeat > 0) {
		runtime_error_repeat(f_setup.err_report_repeat);
		f_setup.err_report_repeat = 0;
	}

	/* Supply default arguments */
	if (zargc < 2)
		zargs[1] = 0;
	if (zargc < 3)
		zargs[2] = 0;

	/* Read input from the current input stream */
	key = stream_read_key(
		zargs[1],	/* timeout value   */
		zargs[2],	/* timeout routine */
		TRUE);  	/* enable hot keys */

	if (key == ZC_BAD)
		return;

	/* Store key */
	/* For timeouts, make sure translate_to_zscii() won't try to convert
	 * 0x00.  We should instead return 0x00 as is.
	 * Thanks to Peter Seebach.
	 */
	if (key == 0)
		store(key);
	else
		store(translate_to_zscii (key));
} /* z_read_char */


/*
 * z_read_mouse, write the current mouse status into a table.
 *
 *	zargs[0] = address of table
 *
 */

void z_read_mouse(void)
{
	zword btn;

	btn = 1;

	/** I don't remember what was going on here */
	/* Read the mouse position and which buttons are down */
/*
	btn = os_read_mouse ();
	z_header.x_mouse_y = mouse_y;
	z_header.x_mouse_x = mouse_x;
*/
	storew((zword) (zargs[0] + 0), z_header.x_mouse_y);
	storew((zword) (zargs[0] + 2), z_header.x_mouse_x);
	storew((zword) (zargs[0] + 4), btn);	/* mouse button bits */
	storew((zword) (zargs[0] + 6), 0);	/* menu selection */

} /* z_read_mouse */


/*
 * truncate_question_mark
 *
 * check if this game is one that expects the interpreter to truncate a
 * trailing question mark from the input buffer.
 *
 * For some games, Infocom modified the interpreter to truncate trailing
 * question marks.  Presumably this was to make it easier to deal with
 * questions asked of the narrator or interpreter, such as "WHAT IS A
 * GRUE?".  This is a deviation from the Z-Machine Standard (written
 * after Infocom's demise).  Some interpreters written later incorrectly
 * always truncate trailing punctuation.  In the interest of making sure
 * the original Infocom games play exactly as they did with Infocom's
 * own interpreters, this function checks for those games that expect
 * the trailing question mark to be truncated.
 *
 */
static bool truncate_question_mark(void)
{
	if (story_id == ZORK1) return TRUE;
	if (story_id == ZORK2) return TRUE;
	if (story_id == ZORK3) return TRUE;
	if (story_id == MINIZORK) return TRUE;
	if (story_id == SAMPLER1) return TRUE;
	if (story_id == SAMPLER2) return TRUE;
	if (story_id == ENCHANTER) return TRUE;
	if (story_id == SORCERER) return TRUE;
	if (story_id == SPELLBREAKER) return TRUE;
	if (story_id == PLANETFALL) return TRUE;
	if (story_id == STATIONFALL) return TRUE;
	if (story_id == BALLYHOO) return TRUE;
	if (story_id == BORDER_ZONE) return TRUE;
	if (story_id == AMFV) return TRUE;
	if (story_id == HHGG) return TRUE;
	if (story_id == LGOP) return TRUE;
	if (story_id == SUSPECT) return TRUE;

	return FALSE;
} /* truncate_question_mark */
