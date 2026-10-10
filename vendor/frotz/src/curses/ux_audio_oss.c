/*
 * ux_audio_oss.c - Sound support using the OSS drivers
 *
 * This code is mostly verbatim from the file x_sample.c in Daniel
 * Schepler's xfrotz-2.32.1.
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

/* #include <bits/sigaction.h> */

#include "ux_defines.h"

#ifdef USE_NCURSES_H
#include <ncurses.h>
#else
#include <curses.h>
#endif

#include "ux_frotz.h"

#ifdef OSS_SOUND	/* don't compile this if not using OSS */

#include <stdio.h>
#include <stdlib.h>
#include <errno.h>
#include <fcntl.h>
#include <unistd.h>
#include <string.h>
#include <sys/types.h>
#include <sys/ioctl.h>
#include <sys/wait.h>

#include <sys/soundcard.h>

extern void end_of_sound(void);

/* Buffer used to store sample data */
static char *sound_buffer = NULL;
static int sound_length;
static int sample_rate;
static int current_num;

/* Implementation of the separate process which plays the sounds.
   The signals used to communicate with the process are:
     SIGINT - complete current repeat of the sound, then quit
     SIGTERM - stop sound immediately
*/

static pid_t child_pid;

/* Number of repeats left */
static int num_repeats;

/* File handles for mixer and PCM devices */
static int mixer_fd, dsp_fd;

static int old_volume;

static void sigterm_handler(int signal) {
	ioctl(dsp_fd, SNDCTL_DSP_RESET, 0);
	if (mixer_fd >= 0)
		ioctl(mixer_fd, SOUND_MIXER_WRITE_VOLUME, &old_volume);
	_exit(EXIT_SUCCESS);
} /* sigterm_handler */


static void oss_sigint_handler(int signal) {
	num_repeats = 1;
} /* oss_sigint_handler */


static void play_sound(int volume, int repeats) {
	struct sigaction sa;
	int format = AFMT_U8;
	int channels = 1;

	dsp_fd = open(SOUND_DEV, O_WRONLY);
	if (dsp_fd < 0) {
		perror(SOUND_DEV);
		_exit(EXIT_FAILURE);
	}


	/* This section of code fixes the nasty problem of samples
	 * being played with pops and scratches when used with a
	 * non-Linux system implementing OSS sound.
	 */
	if (ioctl(dsp_fd, SNDCTL_DSP_SETFMT, &format) == -1) {
		perror(SOUND_DEV);
		exit(EXIT_FAILURE);
	}
	if (format != AFMT_U8) {
		fprintf(stderr, "bad sound format\n");
		exit(EXIT_FAILURE);
	}
	if (ioctl(dsp_fd, SNDCTL_DSP_CHANNELS, &channels) == -1) {
		perror(SOUND_DEV);
		exit(EXIT_FAILURE);
	}
	if (channels != 1) {
		fprintf(stderr, "bad channels\n");
		exit(EXIT_FAILURE);
	}
	/* End sound fix from Torbjorn Andersson (no dot thingies) */

	ioctl(dsp_fd, SNDCTL_DSP_SPEED, &sample_rate);

	if (volume != 255) {
		mixer_fd = open("/dev/mixer", O_RDWR);
		if (mixer_fd < 0)
			perror("/dev/mixer");
		else {
			int new_vol;
			ioctl(mixer_fd, SOUND_MIXER_READ_VOLUME, &old_volume);
			new_vol = volume * 100 / 8;
			new_vol = (new_vol << 8) | new_vol;
			ioctl(mixer_fd, SOUND_MIXER_WRITE_VOLUME, &new_vol);
		}
	} else
		mixer_fd = -1;

	sa.sa_handler = sigterm_handler;
	sigemptyset(&sa.sa_mask);
	sigaddset(&sa.sa_mask, SIGINT);
	sigaddset(&sa.sa_mask, SIGTERM);
	sa.sa_flags = 0;
	sigaction(SIGTERM, &sa, NULL);
	sa.sa_handler = oss_sigint_handler;
	sigaction(SIGINT, &sa, NULL);

	for (num_repeats = repeats; num_repeats > 0;
		num_repeats < 255 ? num_repeats-- : 0) {
		char *curr_pos = sound_buffer;
		int len_left = sound_length;
		int write_result;

		while (len_left > 0) {
			write_result = write(dsp_fd, curr_pos, len_left);
			if (write_result <= 0) {
				perror(SOUND_DEV);
				goto finish;
			}
			curr_pos += write_result;
			len_left -= write_result;
		}
	}

finish:
	ioctl(dsp_fd, SNDCTL_DSP_SYNC, 0);
	if (mixer_fd >= 0)
		ioctl(mixer_fd, SOUND_MIXER_WRITE_VOLUME, &old_volume);
	_exit(EXIT_SUCCESS);
} /* play_sound */


/*
 * os_beep
 *
 * Play a beep sound. Ideally, the sound should be high- (number == 1)
 * or low-pitched (number == 2).
 *
 */
void os_beep (int number)
{
	/* This should later be expanded to support high and low beeps. */
	beep();

} /* os_beep */


/*
 * os_prepare_sample
 *
 * Load the sample from the disk.
 *
 */
void os_prepare_sample (int number)
{
	FILE *samples;
	char *filename;
	const char *basename, *dotpos;
	int namelen;
	int read_length;

	if (sound_buffer != NULL && current_num == number)
		return;

	free(sound_buffer);
	sound_buffer = NULL;

	filename = malloc(strlen(f_setup.story_name) + 10);

	if (! filename)
		return;

	basename = strrchr(f_setup.story_name, '/');
	if (basename) basename++; else basename = f_setup.story_name;
	dotpos = strrchr(basename, '.');
	namelen = (dotpos ? dotpos - basename : strlen(basename));
	if (namelen > 6) namelen = 6;
	sprintf(filename, "%.*ssound/%.*s%02d.snd",
		basename - f_setup.story_name, f_setup.story_name,
		namelen, basename, number);


	samples = fopen(filename, "r");
	if (samples == NULL) {
		perror(filename);
		return;
	}

	fgetc(samples); fgetc(samples); fgetc(samples); fgetc(samples);
	sample_rate = fgetc(samples) << 8;
	sample_rate |= fgetc(samples);
	fgetc(samples); fgetc(samples);
	sound_length = fgetc(samples) << 8;
	sound_length |= fgetc(samples);
	sound_buffer = NULL;

	if (sound_length > 0) {
		sound_buffer = malloc(sound_length);
		if (!sound_buffer) {
			perror("malloc");
			return;
		}
		read_length = fread(sound_buffer, 1, sound_length, samples);
		if (read_length < sound_length) {
			if (feof(samples)) {
				/*
				 * One of the Sherlock samples trigger this for me, so let's make it
				 * a non-fatal error.
				 */
				sound_buffer = realloc(sound_buffer, read_length);
				if (! sound_buffer) {
					perror("realloc");
					return;
				}
				sound_length = read_length;
			} else {
				errno = ferror(samples);
				perror(filename);
				free(sound_buffer);
				sound_buffer = NULL;
			}
		}
	}
} /* os_prepare_sample */


static void sigchld_handler(int signal) {
	int status;
	struct sigaction sa;

	waitpid(child_pid, &status, WNOHANG);
	child_pid = 0;
	sa.sa_handler = SIG_IGN;
	sigemptyset(&sa.sa_mask);
	sa.sa_flags = 0;
	sigaction(SIGCHLD, &sa, NULL);
	end_of_sound();
} /* sigchld_handler */


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
void os_start_sample (int number, int volume, int repeats, zword eos)
{
	/* INCOMPLETE */

	sigset_t sigchld_mask;
	struct sigaction sa;

	os_prepare_sample(number);
	if (! sound_buffer)
		return;
	os_stop_sample(0);

	sigemptyset(&sigchld_mask);
	sigaddset(&sigchld_mask, SIGCHLD);
	sigprocmask(SIG_BLOCK, &sigchld_mask, NULL);

	child_pid = fork();

	if (child_pid < 0) {          /* error in fork */
		perror("fork");
		return;
	} else if (child_pid == 0) {    /* child */
		sigprocmask(SIG_UNBLOCK, &sigchld_mask, NULL);
		play_sound(volume, repeats);
	} else {                        /* parent */
		sa.sa_handler = sigchld_handler;
		sigemptyset(&sa.sa_mask);
		sa.sa_flags = 0;
		sigaction(SIGCHLD, &sa, NULL);

		sigprocmask(SIG_UNBLOCK, &sigchld_mask, NULL);
	}
} /* os_start_sample */


/* Send the specified signal to the player program, then wait for
   it to exit. */
static void stop_player(int signal) {
	sigset_t sigchld_mask;
	struct sigaction sa;
	int status;

	sigemptyset(&sigchld_mask);
	sigaddset(&sigchld_mask, SIGCHLD);
	sigprocmask(SIG_BLOCK, &sigchld_mask, NULL);

	if (child_pid == 0) {
		sigprocmask(SIG_UNBLOCK, &sigchld_mask, NULL);
		return;
	}
	kill(child_pid, signal);
	waitpid(child_pid, &status, 0);
	child_pid = 0;

	sa.sa_handler = SIG_IGN;
	sigemptyset(&sa.sa_mask);
	sa.sa_flags = 0;
	sigaction(SIGCHLD, &sa, NULL);

	sigprocmask(SIG_UNBLOCK, &sigchld_mask, NULL);
} /* stop_player */


/*
 * os_stop_sample
 *
 * Turn off the current sample.
 *
 */
void os_stop_sample (int number)
{
	/* INCOMPLETE */

	stop_player(SIGTERM);
} /* os_stop_sample */


/*
 * os_finish_with_sample
 *
 * Remove the current sample from memory (if any).
 *
 */
void os_finish_with_sample (int number)
{
	/* INCOMPLETE */

	free(sound_buffer);
	sound_buffer = NULL;
} /* os_finish_with_sample */


/*
 * os_wait_sample
 *
 * Stop repeating the current sample and wait until it finishes.
 *
 */
void os_wait_sample (void)
{
	stop_player(SIGINT);
} /* os_wait_sample */

#endif /* OSS_SOUND */
