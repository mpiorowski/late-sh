/*
 * sf_sound.c - SDL interface, audio functions
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

#include "sf_frotz.h"

int m_no_sound = 0;
int m_frequency = 44100;

#ifndef NO_SOUND

#include <stdarg.h>
#include <unistd.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <errno.h>

#include "../blorb/blorblow.h"

#include <SDL.h>
#include <SDL_thread.h>
#include <SDL_mixer.h>

/*************
 * AUDIO!!!
 *
 */
enum { SFX_TYPE, MOD_TYPE };

typedef struct EFFECT {
	void (*destroy)(struct EFFECT *);
	int number;
	int type;
	int active;
	int voice;
	Mix_Music *mod;
	Mix_Chunk *sam;
	int repeats;
	int volume;
	int ended;
	zword eos;
	zlong endtime;
} EFFECT;

/* no effects cache */
static EFFECT *e_sfx = NULL;
static EFFECT *e_mod = NULL;

static int SFaudiorunning = 0;
volatile int end_of_sound_flag = 0;

static int audio_rate, audio_channels;

/* set this to any of 512,1024,2048,4096
 * the higher it is, the more FPS shown and CPU needed
 */
static int audio_buffers = 512;
static Uint16 audio_format;
static int bits;

static void finishaudio(void)
{
	if (!SFaudiorunning)
		return;
	os_stop_sample(0);
	SFaudiorunning = 0;

	/* flush cache */
	if (e_sfx)
		e_sfx->destroy(e_sfx);
	if (e_mod)
		e_mod->destroy(e_mod);

	e_sfx = e_mod = NULL;

	Mix_CloseAudio();
} /* finishaudio */


static void music_finished(void);
static void channel_finished(int channel);

/*
 * This interface is broken.  The function gets called regardless of
 * whether sound is desired and has no means for indicating whether sound is
 * available.  See sf_initsound for the real init.
 */
void os_init_sound(void)
{
} /* os_init_sound */


int sf_initsound(void)
{
	if (SFaudiorunning)
		return 1;
	if (m_no_sound)
		return 0;
	SFaudiorunning = 1;

	/* initialize sdl mixer, open up the audio device */
	if (Mix_OpenAudio(m_frequency, MIX_DEFAULT_FORMAT, 2, audio_buffers) <
	    0) {
		SFaudiorunning = 0;
		return 0;
	}

	Mix_QuerySpec(&audio_rate, &audio_format, &audio_channels);
	bits = audio_format & 0xFF;

	/* hook for end-of-music */
	Mix_HookMusicFinished(music_finished);
	/* hook for end-of-sfx */
	Mix_ChannelFinished(channel_finished);

	Mix_AllocateChannels(1);
	CLEANREG(finishaudio);
	return 1;
} /* sf_initsound */


static void baredestroy(EFFECT * r)
{
	if (r) {
		if (r->mod)
			Mix_FreeMusic(r->mod);
		if (r->sam)
			Mix_FreeChunk(r->sam);
		free(r);
	}
} /* baredestroy */


static EFFECT *new_effect(int type, int num)
{
	EFFECT *reader = (EFFECT *) calloc(1, sizeof(EFFECT));
	if (reader) {
		reader->type = type;
		reader->number = num;
		reader->destroy = baredestroy;
	}
	return (EFFECT *) reader;
} /* new_effect */


/* According to specs, this is only called when music ends "naturally",
 * which I take for "not halted programmatically"
 */
static void music_finished(void)
{
	if (!e_mod)
		return;
	if (!e_mod->active)
		return;
	e_mod->active = 0;
	e_mod->ended = 1;
} /* music_finished */


/* This may be called also via a Mix_Haltetc. */
static void channel_finished(int channel)
{
	if (channel != 0)
		return;
	if (!e_sfx)
		return;
	if (!e_sfx->active)
		return;
	e_sfx->active = 0;
	e_sfx->ended = 1;	/* stopsample will take care of this... */
} /* channel_finished */


static void stopsample(void)
{
	if (!e_sfx)
		return;
	if (!e_sfx->ended)
		return;
	e_sfx->active = 0;
	Mix_HaltChannel(0);
	e_sfx->ended = 0;
} /* stopsample */


static void stopmodule(void)
{
	if (!e_mod)
		return;
	e_mod->active = 0;
	Mix_HaltMusic();
	e_mod->ended = 0;
} /* stopmodule */


static void startsample(void)
{
	if (!e_sfx)
		return;
	Mix_PlayChannel(0, e_sfx->sam, e_sfx->repeats);
	Mix_Volume(0, e_sfx->volume);
	e_sfx->active = 1;
} /* startsample */


static void startmodule(void)
{
	if (!e_mod)
		return;
	Mix_PlayMusic(e_mod->mod, e_mod->repeats);
	Mix_VolumeMusic(e_mod->volume);
	e_mod->active = 1;
} /* startmodule */


static EFFECT *getaiff(FILE * f, size_t pos, int UNUSED (len), int num)
{
	EFFECT *res = NULL;

	res = new_effect(SFX_TYPE, num);
	if (!res)
		return NULL;
	if (fseek(f, pos, SEEK_SET)
	    || !(res->sam = Mix_LoadWAV_RW(SDL_RWFromFP(f, false), 1))) {
		os_warn("Read error on audio data: %s", strerror(errno));
		res->destroy(res);
		return NULL;
	}
	return res;
} /* getaiff */


static EFFECT *getmodule(FILE * f, size_t pos, int UNUSED (len), int num)
{
	EFFECT *res;
	zbyte h[2];

	res = new_effect(MOD_TYPE, num);
	if (!res)
		return NULL;

	fseek(f, pos, SEEK_SET);
	fread(h, 1, 2, f);
	fseek(f, pos, SEEK_SET);
	if (h[0] == 'P' && h[1] == 'K')	/* zipped module */
	{
		int size;
		void *data;
		int st = sf_pkread(f, pos, &data, &size);
		if (st) {
			res->destroy(res);
			return NULL;
		}
		res->mod = Mix_LoadMUS_RW(SDL_RWFromMem(data, size), true);
		free(data);
	} else {
		res->mod = Mix_LoadMUS_RW(SDL_RWFromFP(f, false), true);
	}
	if (!res->mod) {
		res->destroy(res);
		return NULL;
	}
	return res;
} /* getmodule */


static EFFECT *geteffect(int num)
{
	myresource res;
	EFFECT *result = NULL;
	unsigned int id;

	if ((e_sfx) && (e_sfx->number == num))
		return e_sfx;
	if ((e_mod) && (e_mod->number == num))
		return e_mod;

	if (sf_getresource(num, 0, bb_method_FilePos, &res) != bb_err_None)
		return NULL;

	/* Look for a recognized format */
	id = res.type;

	if (id == bb_ID_FORM) {
		result =
		    getaiff(res.file, res.bbres.data.startpos, res.bbres.length,
			    num);
	} else if (id == bb_ID_MOD || id == bb_ID_OGGV) {
		result =
		    getmodule(res.file, res.bbres.data.startpos,
			      res.bbres.length, num);
	}
	sf_freeresource(&res);

	return result;
} /* geteffect */


/* sound handling */

/*
 * os_beep
 *
 * Play a beep sound. Ideally, the sound should be high- (number == 1)
 * or low-pitched (number == 2).
 *
 */
void os_beep(int UNUSED (number))
{
	if (m_no_sound)
		return;
	printf("\a");
	fflush(stdout);
	if (!SFaudiorunning)
		return;
/*	theWnd->FlushDisplay();
	::MessageBeep(MB_ICONEXCLAMATION);*/
} /* os_beep */


/*
 * os_finish_with_sample
 *
 * Remove the current sample from memory (if any).
 *
 */
void os_finish_with_sample(int number)
{
	if (!SFaudiorunning)
		return;
	os_stop_sample(number);
} /* os_finish_with_sample */


/*
 * os_prepare_sample
 *
 * Load the given sample from the disk.
 *
 */
void os_prepare_sample(int UNUSED (number))
{
	if (!SFaudiorunning)
		return;
} /* os_prepare_sample */


/*
 * os_start_sample
 *
 * Play the given sample at the given volume (ranging from 1 to 8 and
 * 255 meaning a default volume). The sound is played once or several
 * times in the background (255 meaning forever). The end_of_sound
 * function is called as soon as the sound finishes, passing in the
 * eos argument.
 *
 */
void os_start_sample(int number, int volume, int repeats, zword eos)
{
	EFFECT *e;

	if (!SFaudiorunning)
		return;


	/* NOTE: geteffect may return an already loaded effect */
	e = geteffect(number);
	if (!e)
		return;
	if (e->type == SFX_TYPE)
		stopsample();
	else
		stopmodule();
	if (repeats < 1)
		repeats = 1;
	if (repeats == 255)
		repeats = -1;
	if (volume < 0)
		volume = 0;
	if (volume > 8)
		volume = 8;
	if (e->type == SFX_TYPE && repeats > 0)
		repeats--;
	e->repeats = repeats;
	e->volume = 32 * volume;
	e->eos = eos;
	e->ended = 0;
	if (e->type == SFX_TYPE) {
		if ((e_sfx) && (e_sfx != e))
			e_sfx->destroy(e_sfx);
		e_sfx = e;
		startsample();
	} else {
		if ((e_mod) && (e_mod != e))
			e_mod->destroy(e_mod);
		e_mod = e;
		startmodule();
	}
} /* os_start_sample */


/*
 * os_stop_sample
 *
 * Turn off the current sample.
 *
 */
void os_stop_sample(int number)
{
	if (!SFaudiorunning)
		return;
	if (number == 0) {
		stopsample();
		stopmodule();
		return;
	}
	if ((e_sfx) && (e_sfx->number == number))
		stopsample();
	if ((e_mod) && (e_mod->number == number))
		stopmodule();
} /* os_stop_sample */


void sf_checksound(void)
{
	if ((e_sfx) && (e_sfx->ended)) {
		e_sfx->ended = 0;
		if (z_header.version <= V4 || e_sfx->eos) {
			end_of_sound_flag = 1;
			end_of_sound();
		}
	}
	if ((e_mod) && (e_mod->ended)) {
		e_mod->ended = 0;
		if (z_header.version <= V4 || e_mod->eos) {
			end_of_sound_flag = 1;
			end_of_sound();
		}
	}
} /* sf_checksound */

/*************************************/

#else /* NO_SOUND */

/* Dummy functions */

void os_init_sound(void)
{
	/* Not implemented */
}

int sf_initsound(void)
{
	/* Not implemented */
	return 0;
}

void os_beep(int UNUSED (number))
{
	/* Not implemented */
}

void os_finish_with_sample(int number)
{
	/* Not implemented */
}

void os_prepare_sample(int UNUSED (number))
{
	/* Not implemented */
}

void os_start_sample(int number, int volume, int repeats, zword eos)
{
	/* Not implemented */
}

void os_stop_sample(int number)
{
	/* Not implemented */
}

void sf_checksound(void)
{
	/* Not implemented */
}

#endif /* NO_SOUND */

/*************************************/
