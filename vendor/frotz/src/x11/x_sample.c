/*
 * x_sample.c
 *
 * X interface, sound support
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

void os_init_sound(void)
{
}


/*
 * os_beep
 *
 * Play a beep sound. Ideally, the sound should be high- (number == 1)
 * or low-pitched (number == 2).
 *
 */
void os_beep(int number)
{
	XKeyboardState old_values;
	XKeyboardControl new_values;

	XGetKeyboardControl(dpy, &old_values);
	new_values.bell_pitch = (number == 2 ? 400 : 800);
	new_values.bell_duration = 100;
	XChangeKeyboardControl(dpy, KBBellPitch | KBBellDuration, &new_values);
	XBell(dpy, 100);
	new_values.bell_pitch = old_values.bell_pitch;
	new_values.bell_duration = old_values.bell_duration;
	XChangeKeyboardControl(dpy, KBBellPitch | KBBellDuration, &new_values);
} /* os_beep */


#ifdef NO_SOUND
/*
 * os_prepare_sample
 *
 * Load the sample from the disk.
 *
 */
void os_prepare_sample(int UNUSED (number))
{

	/* Not implemented */

} /* os_prepare_sample */


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
void os_start_sample(int UNUSED (number), int UNUSED (volume), int UNUSED (repeats), zword UNUSED (eos))
{

	/* Not implemented */

} /* os_start_sample */


/*
 * os_stop_sample
 *
 * Turn off the current sample.
 *
 */
void os_stop_sample(int UNUSED (number))
{

	/* Not implemented */

} /* os_stop_sample */


/*
 * os_finish_with_sample
 *
 * Remove the current sample from memory (if any).
 *
 */
void os_finish_with_sample(int UNUSED (number))
{

	/* Not implemented */

} /* os_finish_with_sample */


/*
 * os_wait_sample
 *
 * Stop repeating the current sample and wait until it finishes.
 *
 */
void os_wait_sample(void)
{

	/* Not implemented */

} /* os_wait_sample */

#endif /* NO_SOUND */

#ifdef OSS_SOUND

#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <errno.h>
#include <fcntl.h>
#include <unistd.h>
#include <sys/soundcard.h>
#include <sys/types.h>
#include <sys/ioctl.h>
#include <sys/wait.h>

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


static void sigterm_handler(int signal)
{
	ioctl(dsp_fd, SNDCTL_DSP_RESET, 0);
	if (mixer_fd >= 0)
		ioctl(mixer_fd, SOUND_MIXER_WRITE_VOLUME, &old_volume);
	_exit(0);
}


static void sigint_handler(int signal)
{
	num_repeats = 1;
}


static void play_sound(int volume, int repeats)
{
	struct sigaction sa;

	dsp_fd = open("/dev/dsp", O_WRONLY);
	if (dsp_fd < 0) {
		perror("/dev/dsp");
		_exit(1);
	}
	ioctl(dsp_fd, SNDCTL_DSP_SPEED, &sample_rate);

	if (volume != 255) {
		mixer_fd = open("/dev/mixer", O_RDWR);
		if (mixer_fd < 0)
			perror("/dev/mixer");
		else {
			int new_vol;
			ioctl(mixer_fd, SOUND_MIXER_READ_VOLUME, &old_volume);
			new_vol = volume * 100 / 8;
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
	sa.sa_handler = sigint_handler;
	sigaction(SIGINT, &sa, NULL);

	for (num_repeats = repeats; num_repeats > 0;
	     num_repeats < 255 ? num_repeats-- : 0) {
		char *curr_pos = sound_buffer;
		int len_left = sound_length;
		int write_result;

		while (len_left > 0) {
			write_result = write(dsp_fd, curr_pos, len_left);
			if (write_result <= 0) {
				perror("write on /dev/dsp");
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
	_exit(0);
}


/*
 * os_prepare_sample
 *
 * Load the sample from the disk.
 *
 */
void os_prepare_sample(int number)
{
	FILE *samples;
	char *filename;
	const char *basename, *dotpos;
	int namelen;

	if (sound_buffer != NULL && current_num == number)
		return;

	free(sound_buffer);
	sound_buffer = NULL;
	filename = malloc(strlen(story_name) + 10);
	if (!filename)
		return;
	basename = strrchr(story_name, '/');
	if (basename)
		basename++;
	else
		basename = story_name;
	dotpos = strrchr(basename, '.');
	namelen = (dotpos ? dotpos - basename : strlen(basename));
	if (namelen > 6)
		namelen = 6;
	sprintf(filename, "%.*ssound/%.*s%02d.snd",
		basename - story_name, story_name, namelen, basename, number);

	samples = fopen(filename, "r");
	if (samples == NULL) {
		perror(filename);
		return;
	}

	fgetc(samples);
	fgetc(samples);
	fgetc(samples);
	fgetc(samples);
	sample_rate = fgetc(samples) << 8;
	sample_rate |= fgetc(samples);
	fgetc(samples);
	fgetc(samples);
	sound_length = fgetc(samples) << 8;
	sound_length |= fgetc(samples);

	sound_buffer = malloc(sound_length);
	if (!sound_buffer) {
		perror("malloc");
		return;
	}

	if (sound_length < 0 ||
	    fread(sound_buffer, 1, sound_length, samples) < sound_length) {
		if (feof(samples))
			fprintf(stderr, "%s: premature EOF\n", filename);
		else {
			errno = ferror(samples);
			perror(filename);
		}
		free(sound_buffer);
		sound_buffer = NULL;
	}

	current_num = number;
} /* os_prepare_sample */


static void sigchld_handler(int signal)
{
	int status;
	struct sigaction sa;

	waitpid(child_pid, &status, WNOHANG);
	child_pid = 0;
	sa.sa_handler = SIG_IGN;
	sigemptyset(&sa.sa_mask);
	sa.sa_flags = 0;
	sigaction(SIGCHLD, &sa, NULL);
	end_of_sound();
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
void os_start_sample(int number, int volume, int repeats)
{
	sigset_t sigchld_mask;
	struct sigaction sa;

	os_prepare_sample(number);
	if (!sound_buffer)
		return;
	os_stop_sample();

	sigemptyset(&sigchld_mask);
	sigaddset(&sigchld_mask, SIGCHLD);
	sigprocmask(SIG_BLOCK, &sigchld_mask, NULL);

	child_pid = fork();

	if (child_pid < 0) {	/* error in fork */
		perror("fork");
		return;
	} else if (child_pid == 0) {	/* child */
		sigprocmask(SIG_UNBLOCK, &sigchld_mask, NULL);
		play_sound(volume, repeats);
	} else {		/* parent */
		sa.sa_handler = sigchld_handler;
		sigemptyset(&sa.sa_mask);
		sa.sa_flags = 0;
		sigaction(SIGCHLD, &sa, NULL);

		sigprocmask(SIG_UNBLOCK, &sigchld_mask, NULL);
	}
} /* os_start_sample */


/* Send the specified signal to the player program, then wait for
   it to exit. */
static void stop_player(int signal)
{
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
}


/*
 * os_stop_sample
 *
 * Turn off the current sample.
 *
 */
void os_stop_sample(void)
{
	stop_player(SIGTERM);
} /* os_stop_sample */


/*
 * os_finish_with_sample
 *
 * Remove the current sample from memory (if any).
 *
 */
void os_finish_with_sample(void)
{
	free(sound_buffer);
	sound_buffer = NULL;
} /* os_finish_with_sample */


/*
 * os_wait_sample
 *
 * Stop repeating the current sample and wait until it finishes.
 *
 */
void os_wait_sample(void)
{
	stop_player(SIGINT);
} /* os_wait_sample */

#endif /* OSS_SOUND */
