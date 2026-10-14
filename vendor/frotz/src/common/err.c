/* err.c - Runtime error reporting functions
 *	Written by Jim Dunleavy <jim.dunleavy@erha.ie>
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

/* Define stuff for stricter Z-code error checking, for the generic
   Unix/DOS/etc terminal-window interface. Feel free to change the way
   player prefs are specified, or replace report_zstrict_error()
   completely if you want to change the way errors are reported. */

/* int err_report_mode = ERR_DEFAULT_REPORT_MODE; */

static int error_count[ERR_NUM_ERRORS];
static int error_repeat_count[ERR_NUM_ERRORS];

static char *err_messages[] = {
	"Text buffer overflow",
	"Store out of dynamic memory",
	"Division by zero",
	"Illegal object",
	"Illegal attribute",
	"No such property",
	"Stack overflow",
	"Call to illegal address",
	"Call to non-routine",
	"Stack underflow",
	"Illegal opcode",
	"Bad stack frame",
	"Jump to illegal address",
	"Can't save while in interrupt",
	"Nesting stream #3 too deep",
	"Illegal window",
	"Illegal window property",
	"Print at illegal address",
	"@jin called with object 0",
	"@get_child called with object 0",
	"@get_parent called with object 0",
	"@get_sibling called with object 0",
	"@get_prop_addr called with object 0",
	"@get_prop called with object 0",
	"@put_prop called with object 0",
	"@clear_attr called with object 0",
	"@set_attr called with object 0",
	"@test_attr called with object 0",
	"@move_object called moving object 0",
	"@move_object called moving into object 0",
	"@remove_object called with object 0",
	"@get_next_prop called with object 0",
	"@play_sound called without SOUND_FLAG or OLD_SOUND_FLAG set",
	"@play_sound called but sound resources are missing",
	"@draw_picture called but picture resources are missing"
};

static void print_long (unsigned long value, int base);


/*
 * init_err
 *
 * Initialise error reporting.
 *
 */
void init_err(void)
{
	int i;

	/* Initialize the counters. */
	for (i = 0; i < ERR_NUM_ERRORS; i++) {
		error_count[i] = 0;
		error_repeat_count[i] = 0;
	}

} /* init_err */


/*
 * runtime_error
 *
 * An error has occurred. Ignore it, pass it to os_fatal or report
 * it according to err_report_mode.
 *
 * errnum : Numeric code for error (1 to ERR_NUM_ERRORS)
 *
 */
void _runtime_error(int errnum, bool repeat)
{
	int wasfirst;

	if (errnum <= 0 || errnum > ERR_NUM_ERRORS)
		return;

	wasfirst = (error_count[errnum - 1] == 0);
	error_count[errnum - 1]++;

	if (repeat && error_repeat_count[errnum - 1] == 0) {
		error_repeat_count[errnum - 1]++;
		wasfirst = TRUE;
	}


	if (wasfirst && (f_setup.err_report_mode == ERR_REPORT_FATAL || errnum <= ERR_MAX_FATAL)) {
		flush_buffer ();
		os_fatal (err_messages[errnum - 1]);
	} else if ((f_setup.err_report_mode == ERR_REPORT_ALWAYS)
		|| (f_setup.err_report_mode == ERR_REPORT_ONCE && wasfirst)) {

		long pc;

		GET_PC (pc);
		print_string ("Warning: ");
		print_string (err_messages[errnum - 1]);
		print_string (" (PC = ");
		print_long (pc, 16);
		print_char (')');
	}

	if (wasfirst) {
		if (f_setup.err_report_mode == ERR_REPORT_ONCE)
			print_string (" (will ignore further occurrences)");
		else {
			print_string (" (occurrence ");
			print_long (error_count[errnum - 1], 10);
			print_char (')');
		}
		new_line ();
	}
} /* report_error */


/*
 * print_long
 *
 * Print an unsigned 32bit number in decimal or hex.
 *
 */
static void print_long (unsigned long value, int base)
{
	unsigned long i;
	char c;

	for (i = (base == 10 ? 1000000000 : 0x10000000); i != 0; i /= base) {
		if (value >= i || i == 1) {
			c = (value / i) % base;
			print_char (c + (c <= 9 ? '0' : 'a' - 10));
		}
	}
} /* print_long */
