/* sound.c - Sound effect function
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

#ifdef DJGPP
#include "djfrotz.h"
#endif

#define EFFECT_PREPARE 1
#define EFFECT_PLAY 2
#define EFFECT_STOP 3
#define EFFECT_FINISH_WITH 4

extern int direct_call(zword);

#ifndef NO_SOUND

static zword routine = 0;

static int next_sample = 0;
static int next_volume = 0;

static bool locked = FALSE;
static bool playing = FALSE;


/*
 * init_sound
 *
 * Initialize sound variables.
 *
 */
void init_sound(void)
{
	/* These games use sound effects numbers 1 and 2, which are defined
	 * as high and low "bleeps" respectively, yet they do not set
	 * SOUND_FLAG or OLD_SOUND_FLAG.  Note that OLD_SOUND_FLAG uses the
	 * same bit as does UNDO_FLAG, but the former is only for V3 games
	 * and the latter for V5+.
	 *
	 * Currently os_beep() just rings the terminal, so f_setup.bleep
	 * isn't yet being used.
	 */
	switch (story_id) {
	case TRINITY:
	case BUREAUCRACY:
	case AMFV:
		f_setup.bleep = TRUE;
		break;
	default:
		break;
	}

	if ((z_header.flags & SOUND_FLAG)
	    || (z_header.version == V3 && (z_header.flags & OLD_SOUND_FLAG))
	    || (f_setup.bleep)
	    ) {
		if (!f_setup.quiet) {
			f_setup.sound = TRUE;
			locked = FALSE;
			playing = FALSE;
			os_init_sound();
		}
	} else
		f_setup.sound = FALSE;
} /* init_sound */


/*
 * start_sample
 *
 * Call the IO interface to play a sample.
 *
 */
static void start_sample(int number, int volume, int repeats, zword eos)
{
	static zbyte lh_repeats[] = {
		0x00, 0x00, 0x00, 0x01, 0xff,
		0x00, 0x01, 0x01, 0x01, 0x01,
		0xff, 0x01, 0x01, 0xff, 0x00,
		0xff, 0xff, 0xff, 0xff, 0xff
	};

	if (story_id == LURKING_HORROR)
		repeats = lh_repeats[number];

	os_start_sample(number, volume, repeats, eos);
	routine = eos;
	playing = TRUE;
} /* start_sample */


/*
 * start_next_sample
 *
 * Play a sample that has been delayed until the previous sound effect has
 * finished.  This is necessary for two samples in The Lurking Horror that
 * immediately follow other samples.
 *
 */
static void start_next_sample(void)
{
	if (next_sample != 0)
		start_sample(next_sample, next_volume, 0, 0);

	next_sample = 0;
	next_volume = 0;
} /* start_next_sample */


/*
 * end_of_sound
 *
 * Call the Z-code routine which was given as the last parameter of
 * a sound_effect call. This function may be called from a hardware
 * interrupt (which requires extremely careful programming).
 *
 */
void end_of_sound(void)
{
#if defined(DJGPP) && defined(NO_SOUND)
	end_of_sound_flag = 0;
#endif

	playing = FALSE;

	if (!locked) {

		if (story_id == LURKING_HORROR)
			start_next_sample();

		direct_call(routine);
	}

} /* end_of_sound */


/*
 * z_sound_effect, load / play / stop / discard a sound effect.
 *
 *   	zargs[0] = number of bleep (1 or 2) or sample
 *	zargs[1] = operation to perform (samples only)
 *	zargs[2] = repeats and volume (play sample only)
 *	zargs[3] = end-of-sound routine (play sample only, optional)
 *
 * Note: Volumes range from 1 to 8, volume 255 is the default volume.
 *	 Repeats are stored in the high byte, 255 is infinite loop.
 *
 */
void z_sound_effect(void)
{
	zword number = zargs[0];
	zword effect = zargs[1];
	zword volume = zargs[2];

	/* By default play sound 1 at volume 8 */
	if (zargc < 1)
		number = 1;
	if (zargc < 2)
		effect = EFFECT_PLAY;
	if (zargc < 3)
		volume = 8;

	if (number == 1 || number == 2) {
		os_beep(number);
		return;
	}

	if (!f_setup.sound)
		runtime_error(ERR_PLAY_SOUND);

	if (f_setup.blorb_file == NULL) {
		runtime_error(ERR_SOUND_MISSING);
		return;
	}

	if (number >= 3 || number == 0) {
		locked = TRUE;
		if (story_id == LURKING_HORROR && (number == 9 || number == 16)) {
			if (effect == EFFECT_PLAY) {
				next_sample = number;
				next_volume = volume;
				locked = FALSE;
				if (!playing)
					start_next_sample();
			} else
				locked = FALSE;
			return;
		}
		playing = FALSE;
		switch (effect) {
		case EFFECT_PREPARE:
			os_prepare_sample(number);
			break;
		case EFFECT_PLAY:
			start_sample(number, lo(volume), hi(volume),
				(zargc == 4) ? zargs[3] : 0);
			break;
		case EFFECT_STOP:
			os_stop_sample(number);
			break;
		case EFFECT_FINISH_WITH:
			os_finish_with_sample(number);
			break;
		}
		locked = FALSE;
	}
} /* z_sound_effect */

#else /* NO_SOUND */

void init_sound(void) { /* nothing here */ }
void end_of_sound(void) { /* nothing here */ }

/* Processes bleeps and nothing more. */
void z_sound_effect(void)
{
	zword number = zargs[0];

	if (number == 1 || number == 2)
		os_beep(number);
	return;
} /* z_sound_effect */
#endif
