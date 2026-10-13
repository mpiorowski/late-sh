/*
 * ux_audio_none.c - Unix interface, sound support
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

#include "ux_defines.h"

#ifdef USE_NCURSES_H
#include <ncurses.h>
#else
#include <curses.h>
#endif

#include "ux_frotz.h"
#ifdef NO_SOUND	/* don't compile this unless we're using no audio */


/*
 * os_init_sound
 *
 * Do any required setup for sound output.
 *
 */
void os_init_sound(void)
{
	/* Not implemented */
}


/*
 * os_beep
 *
 * Play a beep sound. Ideally, the sound should be high- (number == 1)
 * or low-pitched (number == 2).
 *
 */
void os_beep (int UNUSED(number))
{
	beep();
}


/*
 * os_prepare_sample
 *
 * Load the sample from the disk.
 *
 */
void os_prepare_sample (int UNUSED(number))
{

	/* Not implemented */

}


/*
 * os_start_sample
 *
 * Play the given sample at the given volume (ranging from 1 to 8 and
 * 255 meaning a default volume). The sound is played once or several
 * times in the background (255 meaning forever). In Z-code 3 the
 * repeats value is always 0 and the number of repeats is taken from
 * the sound file itself. The end_of_sound function is called as soon
 * as the sound finishes.
 *
 */

void os_start_sample (int UNUSED(number), int UNUSED(volume), int UNUSED(repeats), zword UNUSED(eos))
{

	/* Not implemented */

}


/*
 * os_stop_sample
 *
 * Turn off the current sample.
 *
 */
void os_stop_sample (int UNUSED(number))
{

    /* Not implemented */

}


/*
 * os_finish_with_sample
 *
 * Remove the current sample from memory (if any).
 *
 */
void os_finish_with_sample (int UNUSED(number))
{

    /* Not implemented */

}

/*
 * os_wait_sample
 *
 * Stop repeating the current sample and wait until it finishes.
 *
 */

void os_wait_sample (void)
{

    /* Not implemented */

}

#endif /* NO_SOUND */
