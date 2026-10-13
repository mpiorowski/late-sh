/* hotkey.c - Hot key functions
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

extern int restore_undo (void);

extern int read_number (void);

extern bool read_yes_or_no (const char *);

extern void replay_open (void);
extern void replay_close (void);
extern void record_open (void);
extern void record_close (void);

extern void seed_random (int);

/*
 * hot_key_debugging
 *
 * ...allows user to toggle cheating options on/off.
 *
 */
static bool hot_key_debugging(void)
{
	print_string ("Debugging options\n");
	f_setup.attribute_assignment = read_yes_or_no("Watch attribute assignment");
	f_setup.attribute_testing = read_yes_or_no("Watch attribute testing");
	f_setup.object_movement = read_yes_or_no("Watch object movement");
	f_setup.object_locating = read_yes_or_no("Watch object locating");
	return FALSE;
} /* hot_key_debugging */


/*
 * hot_key_help
 *
 * ...displays a list of all hot keys.
 *
 */
static bool hot_key_help(void)
{
	print_string ("Help\n");
	print_string (
		"\n"
		"Alt-D  debugging options\n"
		"Alt-H  help\n"
		"Alt-N  new game\n"
		"Alt-P  playback on\n"
		"Alt-R  recording on/off\n"
		"Alt-S  seed random numbers\n"
		"Alt-U  undo one turn\n"
		"Alt-X  exit game\n");
	return FALSE;
} /* hot_key_help */


/*
 * hot_key_playback
 *
 * ...allows user to turn playback on.
 *
 */
static bool hot_key_playback(void)
{
	print_string ("Playback on\n");
	if (!istream_replay)
		replay_open ();
	return FALSE;
} /* hot_key_playback */


/*
 * hot_key_recording
 *
 * ...allows user to turn recording on/off.
 *
 */
static bool hot_key_recording(void)
{
	if (istream_replay) {
		print_string("Playback off\n");
		replay_close();
	} else if (ostream_record) {
		print_string("Recording off\n");
		record_close();
	} else {
		print_string("Recording on\n");
		record_open();
	}
	return FALSE;
} /* hot_key_recording */


/*
 * hot_key_seed
 *
 * ...allows user to seed the random number seed.
 *
 */
static bool hot_key_seed(void)
{
	print_string("Seed random numbers\n");
	print_string("Enter seed value (or return to randomize): ");
    	seed_random(read_number ());
	return FALSE;
} /* hot_key_seed */


/*
 * hot_key_undo
 *
 * ...allows user to undo the previous turn.
 *
 */
static bool hot_key_undo(void)
{
	print_string("Undo one turn\n");
	if (restore_undo()) {
		if (z_header.version >= V5) {		/* for V5+ games we must */
			store(2);		/* store 2 (for success) */
			return TRUE;		/* and abort the input   */
		}

		if (z_header.version <= V3) {		/* for V3- games we must */
			z_show_status();	/* draw the status line  */
			return FALSE;		/* and continue input    */
		}
	} else
		print_string("No more undo information available.\n");

	return FALSE;
} /* hot_key_undo */


/*
 * hot_key_restart
 *
 * ...allows user to start a new game.
 *
 */
static bool hot_key_restart(void)
{
	print_string("New game\n");
	if (read_yes_or_no("Do you wish to restart")) {
		z_restart();
		return TRUE;
	} else
		return FALSE;
} /* hot_key_restart */


/*
 * hot_key_quit
 *
 * ...allows user to exit the game.
 *
 */
static bool hot_key_quit(void)
{
	print_string("Exit game\n");
	if (read_yes_or_no("Do you wish to quit")) {
		z_quit();
		return TRUE;
	} else
		return FALSE;
} /* hot_key_quit */


/*
 * handle_hot_key
 *
 * Perform the action associated with a so-called hot key. Return
 * true to abort the current input action.
 *
 */
bool handle_hot_key(zchar key)
{
#ifdef LATE_ZORK
	/* Modified 2026-10-09: no alternate file/undo/restart paths in door mode. */
	if (door_enabled()) return FALSE;
#endif
	if (cwin == 0) {
		bool aborting;
		aborting = FALSE;

		print_string ("\nHot key -- ");

		switch (key) {
		case ZC_HKEY_RECORD: aborting = hot_key_recording(); break;
		case ZC_HKEY_PLAYBACK: aborting = hot_key_playback(); break;
		case ZC_HKEY_SEED: aborting = hot_key_seed(); break;
		case ZC_HKEY_UNDO: aborting = hot_key_undo(); break;
		case ZC_HKEY_RESTART: aborting = hot_key_restart(); break;
		case ZC_HKEY_QUIT: aborting = hot_key_quit(); break;
		case ZC_HKEY_DEBUG: aborting = hot_key_debugging(); break;
		case ZC_HKEY_HELP: aborting = hot_key_help(); break;
		}

		if (aborting)
			return TRUE;

		print_string ("\nContinue input...\n");
	}
	return FALSE;
} /* handle_hot_key */
