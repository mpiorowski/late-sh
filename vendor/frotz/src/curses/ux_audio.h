/*
 * ux_audio.h
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
 * Audio related function prototypes specific to the Curses interface.
 *
 */

#ifndef CURSES_UX_AUDIO_H
#define CURSES_UX_AUDIO_H

void os_init_sound(void);                     /* startup system */
void os_beep(int);                            /* enqueue a beep sample */
void os_prepare_sample(int);                  /* put a sample into memory */
void os_start_sample(int, int, int, zword);   /* queue up a sample */
void os_stop_sample(int);                     /* terminate sample */
void os_finish_with_sample(int);              /* remove from memory */

#endif
