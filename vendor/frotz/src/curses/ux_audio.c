/*
 * ux_audio.c - Unix interface, sound support
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
 * This file and only this file is dual licensed under the MIT license.
 *
 * Copyright (c) 2019 Mark McCurry
 */

#define __UNIX_PORT_FILE

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <assert.h>
#include <unistd.h> /* pread */

#include "ux_defines.h"

#ifdef USE_NCURSES_H
#include <ncurses.h>
#else
#include <curses.h>
#endif

#include "ux_sema.h"
#include "ux_frotz.h"
#include "ux_blorb.h"
#include "ux_audio.h"

extern f_setup_t f_setup;
extern z_header_t z_header;

#ifndef NO_SOUND

ux_sem_t sound_done;	/* 1 if the sound is done */

#include <pthread.h>
#include <ao/ao.h>
#include <sndfile.h>
#include <samplerate.h>
#include <libmodplug/modplug.h>

/* Exports
 * void  os_init_sound(void);                     startup system
 * void  os_beep(int);                            enqueue a beep sample
 * void  os_prepare_sample(int);                  put a sample into memory
 * void  os_start_sample(int, int, int, zword);   queue up a sample
 * void  os_stop_sample(int);                     terminate sample
 * void  os_finish_with_sample(int);              remove from memory
 */

#define EVENT_START_STREAM  1
#define EVENT_STOP_STREAM   2

typedef struct {
	SRC_STATE *src_state;
	SRC_DATA   src_data;
	float	  *scratch;
	float	  *input;
	float	  *output;
} resampler_t;


typedef struct {
	bool active; /* If a voice is actively outputting sound*/
	int  src;    /* The source sound ID*/
	int  type;   /* The voice type 0, 1, 2..N*/
	int  pos;    /* The current position*/
	int  repid;  /* The current number of repetitions*/
} sound_state_t;


typedef struct {
	float  *samples;
	int     nsamples;
} sound_buffer_t;

typedef enum {
	FORM,
	OGGV,
	MOD
} sound_type_t;

typedef struct sound_stream {
	/* returns 1 if process can continue */
	int (*process)(struct sound_stream *self, float *outl, float *outr, unsigned samples);
	void (*cleanup)(struct sound_stream *self);
	sound_type_t sound_type;
	int id;
} sound_stream_t;

typedef struct {
	int (*process)(sound_stream_t *self, float *outl, float *outr, unsigned samples);
	void (*cleanup)(sound_stream_t *self);
	sound_type_t sound_type;
	int id;
} sound_stream_dummy_t;

typedef struct {
	uint8_t *data;
	size_t   len;
	size_t   pos;
} buf_t;

typedef struct {
	FILE    *file;
	size_t   base_offset;
	size_t   len;
	size_t   pos;
} file_reader_t;

typedef struct {
	int (*process)(sound_stream_t *self, float *outl, float *outr, unsigned samples);
	void (*cleanup)(sound_stream_t *self);
	sound_type_t sound_type;
	int id;

	resampler_t *rsmp;
	float  volume;
	float *floatbuffer;

	SNDFILE *sndfile;
	SF_INFO  sf_info;
	size_t length;     /* Number of samples (= smpsl.length == smpsr.length) */
	int    repeats;    /* Total times to play the sample 1..n */
	int    pos;

	file_reader_t freader;
} sound_stream_aiff_t;

typedef struct {
	int (*process)(sound_stream_t *self, float *outl, float *outr, unsigned samples);
	void (*cleanup)(sound_stream_t *self);
	sound_type_t sound_type;
	int id;

	char *filedata;
	short *shortbuffer;
	ModPlugFile *mod;
	ModPlug_Settings settings;
} sound_stream_mod_t;

typedef struct {
	uint8_t type;
	union {
		sound_stream_t *e;
		int            i;
	} data;
} sound_event_t;

#define NUM_VOICES 8

typedef struct {
	/* Audio driver parameters */
	size_t buffer_size;
	float  sample_rate;

	/* Output buffers */
	float *outl;
	float *outr;

	/* Sound parameters */
	sound_stream_t *streams[NUM_VOICES]; /* Active streams */
	sound_state_t   voices[NUM_VOICES];  /* Max concurrent sound effects/music */

	/* Event (one is process per frame of audio) */
	ux_sem_t ev_free;    /* 1 if an event can be submitted */
	ux_sem_t ev_pending; /* 1 if there's an event ready to be processed */
	sound_event_t event;
} sound_engine_t;

static sound_engine_t frotz_audio;
/* FILE *audio_log; */

/**********************************************************************
 *                         Utilities                                  *
 *                                                                    *
 * getfiledata          - get all bytes of a file after               *
 *                        the start point                             *
 * make_id              - Create BLORB identifier                     *
 * get_type             - Get OGG/FORM/AIFF type                      *
 * limit                - x -> a <= x <= b                            *
 *                                                                    *
 **********************************************************************/
static char *
getfiledata(FILE *fp, long *size)
{
	long offset = ftell(fp);
	fseek(fp, 0L, SEEK_END);
	(*size) = ftell(fp);
	fseek(fp, offset, SEEK_SET);
	char *data = (char*)malloc(*size);
	fread(data, *size, sizeof(char), fp);
	fseek(fp, offset, SEEK_SET);
	return(data);
} /* getfiledata */


static
int32_t make_id(uint8_t a, uint8_t b, uint8_t c, uint8_t d)
{
	return (a << 24) | (b << 16) | (c << 8) | d;
} /* make_id */


static int
get_type(int magic)
{
	/*fprintf(audio_log, "magic = %x\n", magic);*/
	if (magic == make_id('F','O','R','M'))
		return FORM;
	if (magic == make_id('M','O','D',' '))
		return MOD;
	if (magic == make_id('O','G','G','V'))
		return OGGV;
	return -1;
} /* get_type */


static float
limit(float mn, float mx, float v)
{
	if (v<mn) return mn;
	if (v>mx) return mx;
	return v;
} /* limit */


/**********************************************************************
 *                         Resampler                                  *
 *                                                                    *
 * Processes data input at one sampling rate and converts to another. *
 * Used on ogg and aiff streams.                                      *
 *                                                                    *
 * NOTE: Additional code may be needed to smoothly handle repeat loop *
 *       conditions                                                   *
 *                                                                    *
 * resampler_init    - Create resampler                               *
 * resampler_cleanup - Deallocate resampler resources
 * resampler_step    - Add data to resampler                          *
 * resampler_consume - Remove data from resampler                     *
 **********************************************************************/

static resampler_t*
resampler_init(int sample_rate_input)
{
	resampler_t *rsmp = (resampler_t*)calloc(sizeof(resampler_t), 1);
	int error;
	rsmp->src_state = src_new(SRC_SINC_FASTEST, 2, &error);
	rsmp->input   = (float*)calloc(frotz_audio.buffer_size, sizeof(float)*2);
	rsmp->output  = (float*)calloc(frotz_audio.buffer_size, sizeof(float)*2);
	rsmp->scratch = (float*)calloc(frotz_audio.buffer_size, sizeof(float)*2);
	rsmp->src_data.src_ratio     = frotz_audio.sample_rate*1.0f/sample_rate_input;
	rsmp->src_data.input_frames  = 0;
	rsmp->src_data.output_frames = frotz_audio.buffer_size;
	rsmp->src_data.data_in       = rsmp->input;
	rsmp->src_data.data_out      = rsmp->output;
	rsmp->src_data.end_of_input  = 0;

	return rsmp;
} /* resampler_init */


static void
resampler_cleanup(resampler_t *rsmp)
{
	src_delete(rsmp->src_state);
	free(rsmp->input);
	free(rsmp->output);
	free(rsmp->scratch);
} /* resampler_cleanup */


/*0 done running, 1 run again with more data*/
static int
resampler_step(resampler_t *rsmp, float *block)
{
	/*Always stereo*/
	const int channels = 2;
	const int smps     = frotz_audio.buffer_size;
	if (block) {
		assert(rsmp->src_data.input_frames == 0);
		memcpy(rsmp->input, block, channels*smps*sizeof(float));
		rsmp->src_data.data_in      = rsmp->input;
		rsmp->src_data.input_frames = smps;
	}
	int err = src_process(rsmp->src_state, &rsmp->src_data);
	assert(err == 0);

	int u_in = rsmp->src_data.input_frames_used;
	rsmp->src_data.data_in      += 2*u_in;
	rsmp->src_data.input_frames -= u_in;
	/*
	 * If input buffer is empty, reset data_in pointer just in case
	 * the output buffer is also full.
	 */
	if (rsmp->src_data.input_frames == 0)
		rsmp->src_data.data_in      = rsmp->input;
	int g_out = rsmp->src_data.output_frames_gen;
	rsmp->src_data.data_out      += 2*g_out;
	rsmp->src_data.output_frames -= g_out;

	if (rsmp->src_data.output_frames == 0)
		return 0;
	return 1;
} /* resampler_step */


static void
resampler_consume(resampler_t *rsmp)
{
	rsmp->src_data.data_out      = rsmp->output;
	rsmp->src_data.output_frames = frotz_audio.buffer_size;
} /* resampler_consume */


/**********************************************************************
 *                           MOD                                      *
 *                                                                    *
 * Processes MOD data via libmodplug                                  *
 *                                                                    *
 * process_mod - Generate MOD samples                                 *
 * cleanup_mod - Free MOD resources                                   *
 * load_mod    - Create MOD stream                                    *
 **********************************************************************/

static int
process_mod(sound_stream_t *self_, float *outl, float *outr, unsigned samples)
{
	sound_stream_mod_t *self = (sound_stream_mod_t*)self_;

	int n = ModPlug_Read(self->mod, self->shortbuffer, samples*4) / 4;
	const float scale = (1.0f/32768.0f);/*volfactor;*/
	int i;
	for (i=0; i<n; ++i) {
		outl[i] += scale*self->shortbuffer[i*2+0];
		outr[i] += scale*self->shortbuffer[i*2+1];
	}

	if (n <= 0)
		return 0;

    return 1;
} /* process_mod */


static void
cleanup_mod(sound_stream_t *s)
{
	sound_stream_mod_t *self = (sound_stream_mod_t*)s;
	ModPlug_Unload(self->mod);
	free(self->shortbuffer);
	free(self->filedata);
} /* cleanup_mod */


/*file, data start, id, volume*/
static sound_stream_t *
load_mod(FILE *fp, long startpos, int id, float volume)
{
	sound_stream_mod_t *stream = (sound_stream_mod_t*)calloc(sizeof(sound_stream_mod_t), 1);
	long size;
	long filestart = ftell(fp);
	fseek(fp, startpos, SEEK_SET);

	stream->id         = id;
	stream->sound_type = MOD;
	stream->process    = process_mod;
	stream->cleanup    = cleanup_mod;

	ModPlug_GetSettings(&stream->settings);

	/* Note: All "Basic Settings" must be set before ModPlug_Load. */
	stream->settings.mResamplingMode   = MODPLUG_RESAMPLE_FIR; /* RESAMP */
	stream->settings.mChannels         = 2;
	stream->settings.mBits             = 16;
	stream->settings.mFrequency        = frotz_audio.sample_rate;
	stream->settings.mStereoSeparation = 128;
	stream->settings.mMaxMixChannels   = 256;

	/* insert more setting changes here */
	ModPlug_SetSettings(&stream->settings);

	/* remember to free() filedata later */
	stream->filedata = getfiledata(fp, &size);

	stream->mod = ModPlug_Load(stream->filedata, size);
	fseek(fp, filestart, SEEK_SET);
	if (!stream->mod) {
		fprintf(stderr, "Unable to load MOD chunk.\n\r");
		return 0;
	}

	ModPlug_SetMasterVolume(stream->mod, volume * 256);/*powf(2.0f, 8.0f));*/

	stream->shortbuffer = (int16_t*)calloc(frotz_audio.buffer_size, sizeof(short) * 2);

	return (sound_stream_t*)stream;
} /* load_mod */


/**********************************************************************
 *                         AIFF/OGG                                   *
 *                                                                    *
 * Processes OGG/AIFF data via sndfile + resampler                    *
 *                                                                    *
 * process_aiff     - Create OGG/AIFF samples                         *
 * cleanup_aiff     - Free   OGG/AIFF resources                       *
 * mem_snd_read     - In memory read                                  *
 * mem_snd_seek     - In memory seek                                  *
 * mem_tell         - In memory tell                                  *
 * mem_get_filelen  - In memory filelen                               *
 * load_aiff        - Create OGG/AIFF stream                          *
 *                                                                    *
 **********************************************************************/

static int
process_aiff(sound_stream_t *self_, float *outl, float *outr, unsigned samples)
{
	sound_stream_aiff_t *self = (sound_stream_aiff_t*)self_;

	int needs_data = resampler_step(self->rsmp, 0);
	int i;
	unsigned remaining_samples = samples;

	while(needs_data) {
		int inf = sf_readf_float(self->sndfile,
			self->floatbuffer, remaining_samples);
		if (self->sf_info.channels == 1) {
			for (i=0; i<inf; ++i) {
				self->rsmp->scratch[2*i+0+2*(samples-remaining_samples)] =
					self->floatbuffer[i];
				self->rsmp->scratch[2*i+1+2*(samples-remaining_samples)] =
					self->floatbuffer[i];
			}
		} else if (self->sf_info.channels == 2) {
			for (i=0; i<inf; ++i) {
				self->rsmp->scratch[2*i+0+2*(samples-remaining_samples)] =
					self->floatbuffer[2*i+0];
				self->rsmp->scratch[2*i+1+2*(samples-remaining_samples)] =
					self->floatbuffer[2*i+1];
			}
		}
		/*
        	 * If the read function didn't fill the scratch buffer, see if
		 * there are more repeats and if so, continue filling the scratch
		 * buffer, a repeat value of 255 means repeat forever
		 */
		if (inf < (int)remaining_samples) {
			if (self->repeats<255)
				self->repeats--;
			if (self->repeats > 0){
				/*
				 * Repeating... Seek back to the beginning of the sound
				 * and allow the read to get enough samples to fill the
				 * scratch buffer, and continue with the while loop
				 */
				sf_seek(self->sndfile,0, SEEK_SET);
				if ( inf == (int)remaining_samples)
					remaining_samples = samples;
				else
					remaining_samples = remaining_samples - inf;
				continue;
			} else if (inf <= 0) {
				/*
				 * No repeats and no data left in the sound file,
				 * return 0 to tell the next level up that the sound
				 * is done
				 */
				return 0;
			} else {
				/*
				 * No repeats but there was data read, set things back
				 * up so that the maximum buffer size can be read, but
				 * fall through to make sure the data is resampled
				 */
				remaining_samples = samples;
			}
		}
        	needs_data = resampler_step(self->rsmp, self->rsmp->scratch);
    	}
	resampler_consume(self->rsmp);

	for (i=0; i<(int)samples; ++i) {
		outl[i] += self->rsmp->output[2*i+0]*self->volume;
		outr[i] += self->rsmp->output[2*i+1]*self->volume;
	}

	return 1;
} /* process_aiff */


static void
cleanup_aiff(sound_stream_t *s)
{
	sound_stream_aiff_t *self = (sound_stream_aiff_t*)s;

	/* Cleanup frame */
	resampler_cleanup(self->rsmp);
	free(self->rsmp);
	sf_close(self->sndfile);
	free(self->floatbuffer);
} /* cleanup_aiff */


static sf_count_t
mem_snd_read(void *ptr_, sf_count_t size, void* datasource)
{
	uint8_t *ptr = (uint8_t*)ptr_;
	file_reader_t *fr = (file_reader_t *)datasource;
	size_t to_read = size;
	size_t read_total = 0;
	ssize_t did_read = 0;
	while (to_read > 0) {
		did_read = pread(fileno(fr->file), ptr, size, fr->pos+fr->base_offset);
		if (did_read < 0)
			return did_read;
		else if (did_read == 0)
			return read_total;
		read_total += did_read;
		fr->pos    += did_read;
		ptr        += did_read;
		to_read    -= did_read;
	}
	return read_total;
} /* mem_snd_read */


static sf_count_t
mem_snd_seek(sf_count_t offset, int whence, void *datasource) {
	file_reader_t *fr = (file_reader_t *)datasource;
	int64_t pos = 0;
	if (whence == SEEK_SET)
		pos = offset;
	if (whence == SEEK_CUR)
		pos += offset;
	if (whence == SEEK_END)
		pos = fr->len-offset;
	if (pos >= (int64_t)fr->len)
		pos = fr->len-1;
	if (pos < 0)
		pos = 0;
	fr->pos = pos;

	return 0;
} /* mem_snd_seek */


static long
mem_tell(void *datasource) {
	file_reader_t *fr = (file_reader_t*)datasource;
	return fr->pos;
} /* mem_tell */


static sf_count_t
mem_get_filelen(void *datasource)
{
	file_reader_t *fr = (file_reader_t*)datasource;
	return fr->len;
} /* mem_get_filelen */


static sound_stream_t *
load_aiff(FILE *fp, long startpos, long length, int id, float volume, int repeats)
{
	sound_stream_aiff_t *aiff =
		(sound_stream_aiff_t*)calloc(sizeof(sound_stream_aiff_t), 1);
	aiff->sound_type = FORM;
	aiff->id         = id;
	aiff->process    = process_aiff;
	aiff->cleanup    = cleanup_aiff;
	aiff->repeats    = repeats;

	aiff->volume = volume;
	aiff->sf_info.format = 0;

	fseek(fp, startpos, SEEK_SET);
	aiff->freader.file        = fp;
	aiff->freader.pos         = 0;
	aiff->freader.len         = length;
	aiff->freader.base_offset = startpos;

	SF_VIRTUAL_IO mem_cb = {
		.seek        = mem_snd_seek,
		.read        = mem_snd_read,
		.tell        = (sf_vio_tell)mem_tell,
		.get_filelen = mem_get_filelen,
		.write       = NULL
	};

	aiff->sndfile = sf_open_virtual(&mem_cb, SFM_READ,
		&aiff->sf_info, &aiff->freader);
	aiff->rsmp = resampler_init(aiff->sf_info.samplerate);

	aiff->floatbuffer = (float*)malloc(frotz_audio.buffer_size *
		aiff->sf_info.channels * sizeof(float));

	return (sound_stream_t*) aiff;
} /* load_aiff */




/**********************************************************************
 *                       Sound Engine                                 *
 *                                                                    *
 * Processes OGG/AIFF data via sndfile + resampler                    *
 *                                                                    *
 * process_engine     - Create a frame of output                      *
 * audio_loop         - Stream audio to sound device                  *
 * sound_halt_aiff    - Stop all AIFF voices                          *
 * sound_halt_mod     - Stop all MOD voices                           *
 * sound_halt_ogg     - Stop all OGG voices                           *
 * sound_stop_id      - Proxy to stop an id                           *
 * sound_stop_id_real - Stop a given stream id                        *
 * sound_enqueue      - Proxy to start a stream obj                   *
 * sound_enqueue_real - Start a stream obj                            *
 * volume_factor      - Convert volume to scalar multiplier           *
 **********************************************************************/

static void
sound_enqueue_real(sound_engine_t *e, sound_stream_t *s);
static void
sound_stop_id_real(sound_engine_t *e, int id);

static void
process_engine(sound_engine_t *e)
{
	int i;
	/* Handle event */
	if (ux_sem_trywait(&e->ev_pending) == 0) {
		if (e->event.type == EVENT_START_STREAM)
			sound_enqueue_real(e,e->event.data.e);
		else if (e->event.type == EVENT_STOP_STREAM)
			sound_stop_id_real(e,e->event.data.i);
		ux_sem_post(&e->ev_free);
	}

	/* Start out with an empty buffer */
	memset(e->outl, 0, sizeof(float)*e->buffer_size);
	memset(e->outr, 0, sizeof(float)*e->buffer_size);

	for (i=0; i<8; ++i) {
		sound_state_t *state = &e->voices[i];

		/* Only process active voices */
		if (!state->active)
			continue;

		sound_stream_t *sound = e->streams[i];

		if (sound) {
			int ret = sound->process(sound, e->outl, e->outr, e->buffer_size);
			if (ret == 0) {
				/* fprintf(audio_log, "stream #%d is complete\n", i); */
				state->active = false;
				sound->cleanup(sound);
				free(sound);
				e->streams[i] = NULL;
				ux_sem_post(&sound_done);
			}
		}
	}
} /* process_engine */


static void*
audio_loop(void*v)
{
	(void) v;
	size_t outsize = frotz_audio.buffer_size*2*sizeof(int16_t);
	int16_t *buf  = (int16_t*)calloc(outsize,1);
	int i;
	ao_device *device;
	ao_sample_format format;
	ao_initialize();
	int default_driver = ao_default_driver_id();

	memset(&format, 0, sizeof(ao_sample_format));

	format.byte_format = AO_FMT_NATIVE;
	format.bits = 16;
	format.channels = 2;
	format.rate = 48000.0f;
	device = ao_open_live(default_driver, &format, NULL);

	while (1) {
		process_engine(&frotz_audio);

		const float mul = (32768.0f);
		for (i=0; i<(int)frotz_audio.buffer_size; ++i) {
			buf[2*i+0] = limit(-32764,32767,mul*0.8*frotz_audio.outl[i]);
			buf[2*i+1] = limit(-32764,32767,mul*0.8*frotz_audio.outr[i]);
		}
		ao_play(device, (char*)buf, outsize);
	}
	return 0;
} /* audio_loop */


static void
sound_halt_aiff(void)
{
	int i;
	for (i=0; i<NUM_VOICES; ++i) {
		if (frotz_audio.streams[i] && frotz_audio.streams[i]->sound_type == FORM) {
			/* fprintf(audio_log, "killing aiff stream #%d\n", i); */
			sound_stream_t *s = frotz_audio.streams[i];
			frotz_audio.streams[i] = 0;
			s->cleanup(s);
			free(s);
		}
	}
} /* sound_halt_aiff */


static void
sound_halt_mod(void)
{
	int i;
	for (i=0; i<NUM_VOICES; ++i) {
		if (frotz_audio.streams[i] && frotz_audio.streams[i]->sound_type == MOD) {
			/* fprintf(audio_log, "killing mod stream #%d\n", i); */
			sound_stream_t *s = frotz_audio.streams[i];
			frotz_audio.streams[i] = 0;
			s->cleanup(s);
			free(s);
		}
	}
} /* sound_halt_mod */


static void
sound_halt_ogg(void)
{
	int i;
	for(i=0; i<NUM_VOICES; ++i) {
		if(frotz_audio.streams[i] && frotz_audio.streams[i]->sound_type == OGGV) {
			/* fprintf(audio_log, "killing ogg stream #%d\n", i); */
			sound_stream_t *s = frotz_audio.streams[i];
			frotz_audio.streams[i] = 0;
			s->cleanup(s);
			free(s);
		}
	}
} /* sound_halt_ogg */


static sound_stream_t *load_mod(FILE *fp, long startpos, int id, float volume);
static sound_stream_t *load_aiff(FILE *fp, long startpos, long length, int id, float volume, int repeats);


static void
sound_stop_id(int id)
{
	ux_sem_wait(&frotz_audio.ev_free);
	frotz_audio.event.type   = EVENT_STOP_STREAM;
	frotz_audio.event.data.i = id;
	ux_sem_post(&frotz_audio.ev_pending);
} /* sound_stop_id */


static void
sound_stop_id_real(sound_engine_t *e, int id)
{
	int i;
	for (i=0; i<NUM_VOICES; ++i) {
		sound_stream_t *s = e->streams[i];
		if (s && s->id == id) {
			/*fprintf(audio_log, "killing stream #%d\n", i);*/
			e->streams[i] = 0;
			s->cleanup(s);
			free(s);
		}
	}
} /* sound_stop_id_real */


static void
sound_enqueue(sound_stream_t *s)
{
	ux_sem_wait(&frotz_audio.ev_free);
	frotz_audio.event.type   = EVENT_START_STREAM;
	frotz_audio.event.data.e = s;
	ux_sem_post(&frotz_audio.ev_pending);
} /* sound_enqueue */


static void
sound_enqueue_real(sound_engine_t *e, sound_stream_t *s)
{
	assert(e);
	assert(s);
	int i;

	if (s->sound_type == FORM) {
		sound_halt_aiff();
	} else if (s->sound_type == MOD) {
		sound_halt_mod();
		sound_halt_ogg();
	} else if (s->sound_type == OGGV) {
		sound_halt_mod();
		sound_halt_ogg();
	}

	for (i=0; i<NUM_VOICES; ++i) {
		if (e->streams[i]) /*only use free voices*/
			continue;
		/* fprintf(audio_log, "Enqueue %p to %d\n", s, i);*/
		e->streams[i]       = s;
		e->voices[i].active = true;
		e->voices[i].src    = 0;
		e->voices[i].pos    = 0;
		e->voices[i].repid  = 0;
		break;
	}
} /* sound_enqueue_real */


static float
volume_factor(int vol)
{
	static float lut[8] = {0.0078125f, 0.015625f, 0.03125f, 0.0625f, 0.125f, 0.25f, 0.5f, 1.0f};

	if (vol < 1) vol = 1;
	if (vol > 8) vol = 8;
	return lut[vol-1];
	/* return powf(2, vol - 8); */
} /* volume_factor */


/**********************************************************************
 *                       Public API                                   *
 *                                                                    *
 **********************************************************************/

void
os_init_sound(void)
{
	int i;
	int err;
	static pthread_attr_t attr;

	if (!f_setup.sound) return;

	/* Initialize sound engine */
	/* audio_log = fopen("audio_log.txt", "w"); */
	/* fprintf(audio_log, "os_init_sound...\n"); */
	frotz_audio.buffer_size = 1024;
	frotz_audio.sample_rate = 48000;
	frotz_audio.outl        = (float*)calloc(frotz_audio.buffer_size, sizeof(float));
	frotz_audio.outr        = (float*)calloc(frotz_audio.buffer_size, sizeof(float));

	for(i=0; i<NUM_VOICES; ++i)
		frotz_audio.voices[i].active = 0;

	/* No events registered on startup */
	ux_sem_init(&frotz_audio.ev_free,    0, 1);
	ux_sem_init(&frotz_audio.ev_pending, 0, 0);
	ux_sem_init(&sound_done, 0, 0);
	frotz_audio.event.type = 0;

	/* Start audio thread */
	pthread_attr_init(&attr);
	pthread_attr_setdetachstate(&attr, PTHREAD_CREATE_DETACHED);

	pthread_t unused_id;
	err = pthread_create(&unused_id, &attr, &audio_loop, NULL);
	if (err != 0) {
		fprintf(stderr, "Can't create audio thread :[%s]", strerror(err));
		os_quit(EXIT_FAILURE);
	}
} /* os_init_sound */


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
 * The end_of_sound function is called in os_tick();
 *
 */
void
os_start_sample(int number, int volume, int repeats, zword eos)
{
	(void) eos;
	/* fprintf(audio_log, "os_start_sample(%d,%d,%d,%d)...\n",number,volume,repeats, eos); */
	/* fflush(audio_log); */
	extern bb_map_t     *blorb_map;
	extern FILE         *blorb_fp;

	bb_result_t resource;
	int type;
	const float vol = volume_factor(volume);
	sound_stream_t *s = 0;

	if (!f_setup.sound) return;

	/* Load resource from BLORB data */
	if (blorb_map == NULL) return;

	if (bb_err_None != bb_load_resource(blorb_map, bb_method_FilePos, &resource, bb_ID_Snd, number))
		return;

	type = get_type(blorb_map->chunks[resource.chunknum].type);

	if (type == FORM) {
		s = load_aiff(blorb_fp,
			resource.data.startpos,
			resource.length,
			number,
			vol,
			repeats);
	} else if (type == MOD) {
		s = load_mod(blorb_fp, resource.data.startpos, number, vol);
	} else if (type == OGGV) {
		s = load_aiff(blorb_fp,
			resource.data.startpos,
			resource.length,
			number,
			vol,
			repeats);
		s->sound_type = OGGV;
	}

	if (s)
		sound_enqueue(s);
} /* os_start_sample */


void os_beep(int bv)
{
	(void) bv;
	/* Currently implemented as a simple terminal bell */
	/* To implement generate a high frequency beep for bv=1, */
	/* low frequency for bv=2 */
	/* fprintf(audio_log, "os_beep(%d)...\n", bv); */
	beep();
} /* os_beep */


void os_prepare_sample(int id)
{
	(void) id;
	/* Currently not implemented */
	/* fprintf(audio_log, "os_prepare_sample(%d)...\n", id); */
} /* os_prepare_sample */


void os_stop_sample(int id)
{
	/* fprintf(audio_log, "os_stop_sample(%d)...\n", id); */
	if (!f_setup.sound) return;
	sound_stop_id(id);
} /* os_stop_sample */


void os_finish_with_sample(int id)
{
	/* fprintf(audio_log, "os_finish_with_sample(%d)...\n", id); */
	os_stop_sample(id);
} /* os_finish_with_sample */

#endif /* NO_SOUND */
