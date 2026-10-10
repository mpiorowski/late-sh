/*
 * file "dossampl.c"
 *
 * DOS front end, sound support
 *
 */

#ifdef __WATCOMC__
#include <malloc.h>
#include <conio.h>
#else
#include <alloc.h>
#endif

#include <dos.h>
#include <stdlib.h>
#include <stdio.h>
#include <string.h>
#include "frotz.h"
#include "dosfrotz.h"

#ifndef NO_SOUND

#ifdef __WATCOMC__
#define SWAP_BYTES(v)	{ (v) = bswap16(v); }
#else
#define SWAP_BYTES(v)	{_AX=v;asm xchg al,ah;v=_AX;}
#endif
#define READ_DSP(v)	{while(!inp(sound_adr+14)&0x80);(v)=inportb(sound_adr+10);}
#define WRITE_DSP(v)	{while(inp(sound_adr+12)&0x80);outportb(sound_adr+12,(v));}

extern void end_of_sound(void);

static struct {
	word prefix;
	byte repeats;
	byte base_note;
	word frequency;
	word unused;
	word length;
} sheader;

static int current_sample = 0;

static void interrupt(*vect) (void) = NULL;

static int play_part = 0;
static int play_count = 0;

static word sound_adr = 0;
static word sound_irq = 0;
static word sound_dma = 0;
static word sound_int = 0;
static word sound_ver = 0;

#ifdef __WATCOMC__
static byte _huge *sample_data = NULL;
#else
static byte far *sample_data = NULL;
#endif

static long sample_adr1 = 0;
static long sample_adr2 = 0;
static word sample_len1 = 0;
static word sample_len2 = 0;


/*
 * start_of_dma
 *
 * Start the DMA transfer to the sound board.
 *
 */
static void start_of_dma(long address, unsigned length)
{
	static unsigned dma_page_port[] = {
		0x87, 0x83, 0x81, 0x82
	};

	length--;

	/* Set up DMA chip */
	outportb(0x0a, 0x04 | sound_dma);
	outportb(0x0c, 0x00);
	outportb(0x0b, 0x48 | sound_dma);
	outportb(2 * sound_dma, byte0(address));
	outportb(2 * sound_dma, byte1(address));
	outportb(dma_page_port[sound_dma], byte2(address));
	outportb(2 * sound_dma + 1, byte0(length));
	outportb(2 * sound_dma + 1, byte1(length));
	outportb(0x0a, sound_dma);

	/* Play 8-bit mono sample */
	WRITE_DSP(0x14)
	    WRITE_DSP(byte0(length))
	    WRITE_DSP(byte1(length))
} /* start_of_dma */


/*
 * end_of_dma
 *
 * This function is called when a hardware interrupt signals the
 * end of the current sound. We may have to play the second half
 * of the sound effect, or we may have to repeat it, or call the
 * end_of_sound function when we are finished.
 *
 */
static void interrupt end_of_dma(void)
{
	/* Play the second half, play another cycle or finish */
	if (play_part == 1 && sample_len2 != 0) {
		play_part = 2;
		start_of_dma(sample_adr2, sample_len2);
	} else if (play_count == 255 || --play_count != 0) {
		play_part = 1;
		start_of_dma(sample_adr1, sample_len1);
	} else {
		play_part = 0;
		end_of_sound();
	}

	/* Tell interrupt controller(s) + sound board we are done */
	outportb(0x20, 0x20);
	if (sound_irq >= 8)
		outportb(0xa0, 0x20);
	inportb(sound_adr + 14);
} /* end_of_dma */


/*
 * dos_init_sound
 *
 * Initialise the sound board and various sound related variables.
 *
 */
bool dos_init_sound(void)
{
	const char *settings;
	word irc_mask_port;

	/* Read the IRQ, port address, DMA channel and SB version */
	if ((settings = getenv("BLASTER")) == NULL)
		return FALSE;

	sound_irq = dectoi(strchr(settings, 'I') + 1);
	sound_adr = hextoi(strchr(settings, 'A') + 1);
	sound_dma = dectoi(strchr(settings, 'D') + 1);
	sound_ver = dectoi(strchr(settings, 'T') + 1);

	/* Reset mixer chip and DSP */
	outportb(sound_adr + 4, 0);
	outportb(sound_adr + 5, 0);

	outportb(sound_adr + 6, 1);
	inportb(sound_adr + 6);
	inportb(sound_adr + 6);
	inportb(sound_adr + 6);
	outportb(sound_adr + 6, 0);

	/* Turn on speakers */
	WRITE_DSP(0xd1)
	    /* Install the end_of_dma interrupt */
	    if (sound_irq < 8) {
		irc_mask_port = 0x21;
		sound_int = 0x08 + sound_irq;
	} else {
		irc_mask_port = 0xa1;
		sound_int = 0x68 + sound_irq;
	}

	vect = getvect(sound_int);
	setvect(sound_int, end_of_dma);

	/* Allocate 64KB RAM for sample data */
#ifdef __WATCOMC__
	if ((sample_data = (byte _huge *) halloc(0x10000L, 1)) == NULL)
#else
	if ((sample_data = (byte far *) farmalloc(0x10000L)) == NULL)
#endif
		return FALSE;

	word0(sample_adr1) = FP_OFF(sample_data) | (FP_SEG(sample_data) << 4);
	word1(sample_adr1) = FP_SEG(sample_data) >> 12;
	word0(sample_adr2) = 0;
	word1(sample_adr2) = word1(sample_adr1) + 1;

	/* Enable the end_of_dma interrupt */
	outportb(0x20, 0x20);

	if (sound_irq >= 8)
		outportb(0xa0, 0x20);

	outportb(irc_mask_port, inportb(irc_mask_port) & ~(1 << (sound_irq & 7)));

	/* Indicate success */
	return TRUE;
} /* init_sound */


/*
 * dos_reset_sound
 *
 * Free resources allocated for playing samples.
 *
 */
void dos_reset_sound(void)
{
	os_stop_sample(0);

	if (sample_data != NULL) {
#ifdef __WATCOMC__
		hfree(sample_data);
#else
		farfree(sample_data);
#endif
		sample_data = NULL;
	}
	if (sound_adr != 0) {
#ifdef __WATCOMC__
		_dos_setvect(sound_int, vect);
#else
		setvect(sound_int, vect);
#endif
		sound_adr = 0;
	}
} /* dos_reset_sound */


/*
 * os_init_sound
 *
 * Dummy function to satisfy the core code.  DOS Frotz does its sound
 * initialization in dosinit.c in os_init_screen().
 *
 * FIXME: Move the sound initlization from os_init_screen() to here and
 *        somehow work around the ifs.
 *
 */
void os_init_sound(void) { /* do nothing */ }


/*
 * os_prepare_sample
 *
 * Load the sample from the disk.
 *
 */
void os_prepare_sample(int number)
{
	os_stop_sample(0);

	/* Exit if the sound board isn't set up properly */
	if (sample_data == NULL)
		return;
	if (sound_adr == 0)
		return;

	/* Continue only if the desired sample is not already present */
	if (current_sample != number) {

		char sample_name[MAX_FILE_NAME + 1];
		char numstr[2];
		FILE *fp;

		/* Build sample file name */
		strcpy(sample_name, "sound/");

		numstr[0] = '0' + number / 10;
		numstr[1] = '0' + number % 10;

		strncat(sample_name, f_setup.story_name, 6);
		strncat(sample_name, numstr, 2);
		strncat(sample_name, ".snd", 4);

		/* Open sample file */
		if ((fp = fopen(sample_name, "rb")) == NULL)
			return;

		/* Load header and sample data */
		fread(&sheader, sizeof(sheader), 1, fp);

		SWAP_BYTES(sheader.frequency)
		    SWAP_BYTES(sheader.length)
		    fread(sample_data, 1, sheader.length, fp);

		sample_len1 = -word0(sample_adr1);

		if (sample_len1 > sheader.length || sample_len1 == 0)
			sample_len1 = sheader.length;

		sample_len2 = sheader.length - sample_len1;

		WRITE_DSP(0x40)
		    WRITE_DSP(256 - 1000000L / sheader.frequency)
		    current_sample = number;

		/* Close sample file */
		fclose(fp);

	}
} /* os_prepare_sample */


/*
 * os_start_sample
 *
 * Play the given sample at the given volume (ranging from 1 to 8 and
 * 255 meaning a default volume). The sound is played once or several
 * times in the background (255 meaning forever). The end_of_sound
 * function is called as soon as the sound finishes.
 *
 */
void os_start_sample(int number, int volume, int repeats, zword eos)
{
	eos = eos;		/* not used in DOS Frotz */

	os_stop_sample(0);

	/* Exit if the sound board isn't set up properly */
	if (sample_data == NULL)
		return;
	if (sound_adr == 0)
		return;

	/* Load new sample */
	os_prepare_sample(number);

	/* Continue only if the sample's in memory now */
	if (current_sample == number) {
		play_count = repeats;

		if (sound_ver < 6) {	/* Set up SB pro mixer chip */
			volume = (volume != 255) ? 7 + volume : 15;
			outportb(sound_adr + 4, 0x04);
			outportb(sound_adr + 5, (volume << 4) | volume);
			outportb(sound_adr + 4, 0x22);
			outportb(sound_adr + 5, 0xff);
		} else {	/* Set up SB16 mixer chip */
			/* Many thanks to Linards Ticmanis for writing this part! */
			volume = (volume != 255) ? 127 + 16 * volume : 255;
			outportb(sound_adr + 4, 0x32);
			outportb(sound_adr + 5, volume);
			outportb(sound_adr + 4, 0x33);
			outportb(sound_adr + 5, volume);
			outportb(sound_adr + 4, 0x30);
			outportb(sound_adr + 5, 0xff);
			outportb(sound_adr + 4, 0x31);
			outportb(sound_adr + 5, 0xff);
		}

		play_part = 1;
		start_of_dma(sample_adr1, sample_len1);

	}
} /* os_start_sample */


/*
 * os_stop_sample
 *
 * Turn off the current sample.
 *
 */
void os_stop_sample(int UNUSED(id))
{
	play_part = 0;

	/* Exit if the sound board isn't set up properly */
	if (sample_data == NULL)
		return;
	if (sound_adr == 0)
		return;

	/* Tell DSP to stop the current sample */
	WRITE_DSP(0xd0)
} /* os_stop_sample */


/*
 * os_finish_with_sample
 *
 * Remove the current sample from memory (if any).
 *
 */
void os_finish_with_sample(int UNUSED(id))
{
	os_stop_sample(0);	/* we keep 64KB allocated all the time */
} /* os_finish_with_sample */


#else /* NO_SOUND */

/* Do-nothing stubs for when sound support is disabled. */

void os_finish_with_sample(int UNUSED(id)) {}
void os_stop_sample(int UNUSED(id)) {}
void os_start_sample(int number, int volume, int repeats, zword eos) {}
void os_prepare_sample(int number) {}
void os_init_sound(void) {}
void dos_reset_sound(void) {}
bool dos_init_sound(void) { return TRUE; }

#endif /* NO_SOUND */


/*
 * os_beep
 *
 * Play a beep sound. Ideally, the sound should be high- (number == 1)
 * or low-pitched (number == 2).
 *
 */
void os_beep(int number)
{
	word T = 888 * number;

	if (!f_setup.sound) return;

	outportb(0x43, 0xb6);
	outportb(0x42, lo(T));
	outportb(0x42, hi(T));
	outportb(0x61, inportb(0x61) | 3);
	delay(75);
	outportb(0x61, inportb(0x61) & ~3);
} /* os_beep */
