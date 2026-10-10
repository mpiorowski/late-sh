/* main.c - Frotz V2.40 main function
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

/*
 * This is an interpreter for Infocom V1 to V6 games. It also supports
 * the recently defined V7 and V8 games.
 *
 */

#include <stdlib.h>
#include "frotz.h"

#ifndef MSDOS_16BIT
#define cdecl
#endif

extern void interpret (void);
extern void init_memory (void);
extern void init_undo (void);
extern void reset_screen (void);
extern void reset_memory (void);

bool need_newline_at_exit = FALSE;

/* Story file name, id number and size */
char *story_name = 0;
enum story story_id = UNKNOWN;
long story_size = 0;

/* Setup data */
extern f_setup_t f_setup;

/* Story file header data */
extern z_header_t z_header;

/* Stack data */
zword stack[STACK_SIZE];
zword *sp = 0;
zword *fp = 0;
zword frame_count = 0;

/* IO streams */
bool ostream_screen = TRUE;
bool ostream_script = FALSE;
bool ostream_memory = FALSE;
bool ostream_record = FALSE;
bool istream_replay = FALSE;
bool message = FALSE;

/* Current window and mouse data */
int cwin = 0;
int mwin = 0;
int mouse_y = 0;
int mouse_x = 0;

/* Window attributes */
bool enable_wrapping = FALSE;
bool enable_scripting = FALSE;
bool enable_scrolling = FALSE;
bool enable_buffering = FALSE;

int option_sound = 1;
char *option_zcode_path;

/* Size of memory to reserve (in bytes) */
long reserve_mem = 0;

#ifdef TOPS20
/* Strange little TOPS-20 accomodation */
bool spurious_getchar = FALSE;
#endif

/*
 * z_piracy, branch if the story file is a legal copy.
 *
 *	no zargs used
 *
 */
void z_piracy(void)
{
	branch (!f_setup.piracy);
} /* z_piracy */


/*
 * main
 *
 * Prepare and run the game.
 *
 */
#ifdef TOPS20
int main (int argc, char *argv[])
#else
int cdecl main(int argc, char *argv[])
#endif
{
	init_header();
	init_setup();
	os_init_setup();
	os_process_arguments(argc, argv);
	init_buffer();
	init_err();
	init_memory();
#ifdef LATE_ZORK
	/* Modified 2026-10-09: validate explicit door startup before curses. */
	door_init();
#endif
	init_process();
	init_sound();
	os_init_screen();
	init_undo();
	z_restart();
#ifdef LATE_ZORK
	door_start();
#endif
	interpret();
	reset_screen();
	reset_memory();
	os_reset_screen();
	os_quit(EXIT_SUCCESS);
	return 0;
} /* main */
