/* fastmem.c - Memory related functions (fast version without virtual memory)
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
 * New undo mechanism added by Jim Dunleavy <jim.dunleavy@erha.ie>
 */

#include <stdio.h>
#include <string.h>
#include "frotz.h"

#ifndef MSDOS_16BIT

#include <stdlib.h>

#ifndef SEEK_SET
#define SEEK_SET 0
#define SEEK_CUR 1
#define SEEK_END 2
#endif
#endif /* !MSDOS_16BIT */

#ifdef __WATCOMC__
zbyte huge *zmp = NULL;
zbyte huge *pcp = NULL;
#else
zbyte *zmp = NULL;
zbyte *pcp = NULL;
#endif

extern void seed_random (int);
extern void restart_screen (void);
extern void refresh_text_style (void);
extern void call (zword, int, zword *, int);
extern void split_window (zword);
extern void script_open (void);
extern void script_close (void);


extern zword save_quetzal (FILE *, FILE *);
extern zword restore_quetzal (FILE *, FILE *);

extern void erase_window (zword);

extern void (*op0_opcodes[]) (void);
extern void (*op1_opcodes[]) (void);
extern void (*op2_opcodes[]) (void);
extern void (*var_opcodes[]) (void);

/* char save_name[MAX_FILE_NAME + 1] = DEFAULT_SAVE_NAME; */
char auxiliary_name[MAX_FILE_NAME + 1] = DEFAULT_AUXILIARY_NAME;

static FILE *story_fp = NULL;
#ifdef LATE_ZORK
/* Modified 2026-10-09: private checkpoint access to original story bytes. */
FILE *door_story_file(void) { return story_fp; }
#endif

/*
 * Data for the undo mechanism.
 * This undo mechanism is based on the scheme used in Evin Robertson's
 * Nitfol interpreter.
 * Undo blocks are stored as differences between states.
 */
typedef struct undo_struct undo_t;
struct undo_struct {
	undo_t *next;
	undo_t *prev;
	long pc;
	long diff_size;
	zword frame_count;
	zword stack_size;
	zword frame_offset;
	/* undo diff and stack data follow */
};

static undo_t huge *first_undo = NULL, huge *last_undo = NULL,
	      huge *curr_undo = NULL;
static zbyte huge *prev_zmp, *undo_diff;

static int undo_count = 0;


#ifdef __WATCOMC__
void huge *zrealloc(void huge *p, long size, size_t old_size)
{
	void huge *q = zmalloc(size);
	if (q == NULL)
		return NULL;
	_fmemcpy(q, p, size < old_size ? size : old_size);
	zfree(p);
	return q;
} /* zrealloc */
#endif /* __WATCOMC__ */


/*
 * get_header_extension
 *
 * Read a value from the header extension (former mouse table).
 *
 */
zword get_header_extension(int entry)
{
	zword addr;
	zword val;

	if (z_header.extension_table == 0 || entry > z_header.x_table_size)
		return 0;

	addr = z_header.extension_table + 2 * entry;
	LOW_WORD(addr, val);

	return val;
} /* get_header_extension */


/*
 * set_header_extension
 *
 * Set an entry in the header extension (former mouse table).
 *
 */
void set_header_extension(int entry, zword val)
{
	zword addr;

	if (z_header.extension_table == 0 || entry > z_header.x_table_size)
		return;

	addr = z_header.extension_table + 2 * entry;
	SET_WORD(addr, val);
} /* set_header_extension */


/*
 * restart_header
 *
 * Set all header fields which hold information about the interpreter.
 *
 */
void restart_header(void)
{
	zword screen_x_size;
	zword screen_y_size;
	zbyte font_x_size;
	zbyte font_y_size;

	int i;

	SET_BYTE(H_CONFIG, z_header.config);
	SET_WORD(H_FLAGS, z_header.flags);

	if (z_header.version == V6 && f_setup.interpreter_number == 0) {
		if (story_id == JOURNEY) {
			switch (z_header.release) {
			case 26: f_setup.interpreter_number = INTERP_MACINTOSH; break;
			case 30: f_setup.interpreter_number = INTERP_AMIGA; break;
			case 77: f_setup.interpreter_number = INTERP_APPLE_IIE; break;
			case 83: f_setup.interpreter_number = INTERP_MSDOS; break;
			default: f_setup.interpreter_number = INTERP_AMIGA; break;
			}
			z_header.interpreter_number = f_setup.interpreter_number;
		} else if (story_id == SHOGUN) {
			switch (z_header.release) {
			case 292: f_setup.interpreter_number = INTERP_MACINTOSH; break;
			case 295: f_setup.interpreter_number = INTERP_AMIGA; break;
			case 311: f_setup.interpreter_number = INTERP_APPLE_IIE; break;
			case 322: f_setup.interpreter_number = INTERP_MSDOS; break;
			default: f_setup.interpreter_number = INTERP_AMIGA; break;
			}
			z_header.interpreter_number = f_setup.interpreter_number;
		} else if (story_id == ZORK_ZERO) {
			switch (z_header.release) {
			case 296: f_setup.interpreter_number = INTERP_MACINTOSH; break;
			case 366: f_setup.interpreter_number = INTERP_AMIGA; break;
			case 383: f_setup.interpreter_number = INTERP_APPLE_IIE; break;
			case 393: f_setup.interpreter_number = INTERP_MSDOS; break;
			default: f_setup.interpreter_number = INTERP_AMIGA; break;
			}
			z_header.interpreter_number = f_setup.interpreter_number;
		} else if (story_id == ARTHUR) {
			switch (z_header.release) {
			case 54: f_setup.interpreter_number = INTERP_AMIGA; break;
			case 64: f_setup.interpreter_number = INTERP_APPLE_IIE; break;
			case 74: f_setup.interpreter_number = INTERP_MSDOS; break;
			default: f_setup.interpreter_number = INTERP_AMIGA; break;
			}
			z_header.interpreter_number = f_setup.interpreter_number;
		}
        }

	if (z_header.version >= V4) {
		SET_BYTE(H_INTERPRETER_NUMBER, z_header.interpreter_number);
		SET_BYTE(H_INTERPRETER_VERSION, z_header.interpreter_version);
		SET_BYTE(H_SCREEN_ROWS, z_header.screen_rows);
		SET_BYTE(H_SCREEN_COLS, z_header.screen_cols);
	}

	/* It's less trouble to use font size 1x1 for V5 games, especially
	   because of a bug in the unreleased German version of "Zork 1" */

	if (z_header.version != V6) {
		screen_x_size = (zword) z_header.screen_cols;
		screen_y_size = (zword) z_header.screen_rows;
		font_x_size = 1;
		font_y_size = 1;
	} else {
		screen_x_size = z_header.screen_width;
		screen_y_size = z_header.screen_height;
		font_x_size = z_header.font_width;
		font_y_size = z_header.font_height;
	}

	if (z_header.version >= V5) {
		SET_WORD(H_SCREEN_WIDTH, screen_x_size);
		SET_WORD(H_SCREEN_HEIGHT, screen_y_size);
		SET_BYTE(H_FONT_HEIGHT, font_y_size);
		SET_BYTE(H_FONT_WIDTH, font_x_size);
		SET_BYTE(H_DEFAULT_BACKGROUND, z_header.default_background);
		SET_BYTE(H_DEFAULT_FOREGROUND, z_header.default_foreground);
	}

	if (z_header.version >= V3 && (z_header.user_name[0] != 0)) {
		for (i = 0; i < 8; i++)
			storeb((zword) (H_USER_NAME + i), z_header.user_name[i]);
	}

	SET_BYTE(H_STANDARD_HIGH, z_header.standard_high);
	SET_BYTE(H_STANDARD_LOW, z_header.standard_low);
} /* restart_header */


void init_header(void)
{
	memset(&z_header, 0, sizeof(z_header));
	/* Claim to support Z-machine Standard 1.1. */
	z_header.standard_high = 1;
	z_header.standard_low = 1;
} /* init_header */


void init_setup(void)
{
	memset(&f_setup, 0, sizeof(f_setup));
	f_setup.undo_slots = DEFAULT_UNDO_SLOTS;
	f_setup.script_cols = 80;
	f_setup.err_report_mode = ERR_DEFAULT_REPORT_MODE;
	f_setup.blorb_file = NULL;
	f_setup.story_file = NULL;
	f_setup.story_name = NULL;
	f_setup.story_base = NULL;
	f_setup.script_name = NULL;
	f_setup.script_name_override = NULL;
	f_setup.command_name = NULL;
	f_setup.save_name = NULL;
	f_setup.tmp_save_name = NULL;
	f_setup.aux_name = NULL;
	f_setup.story_path = NULL;
	f_setup.zcode_path = NULL;
	f_setup.restricted_path = NULL;
} /* init_setup */


/*
 * init_memory
 *
 * Allocate memory and load the story file.
 *
 * Data collected from http://www.russotto.net/zplet/ivl.html
 * and from https://eblong.com/infocom/
 *
 */
void init_memory(void)
{
	long size;
	zword addr;
	unsigned n;
	int i, j;
	char errorstring[26]; /* Don't reuse this. */

#ifdef TOPS20
	zword checksum = 0;
	long li;
#endif

	/* INDENT-OFF */
	static struct {
		enum story story_id;
		zword release;
		zbyte serial[6];
	} records[] = {
		{	   ZORK1,   2, "AS000C" },
		{	   ZORK1,   5, ""       },
		{	   ZORK1,  15, "UG3AU5" },
		{	   ZORK1,  20, ""       },
		{	   ZORK1,  23, "820428" },
		{	   ZORK1,  25, "820515" },
		{	   ZORK1,  26, "820803" },
		{	   ZORK1,  28, "821013" },
		{	   ZORK1,  30, "830330" },
		{	   ZORK1,  75, "830929" },
		{	   ZORK1,  76, "840509" },
		{	   ZORK1,  88, "840726" },
		{	   ZORK1,  52, "871125" },
		{	   ZORK1, 119, "880429" },
		{	  ZORK1G,   3, "880113" },
		{	  ZORK1G,  15, "890613" },
		{	   ZORK2,   7, "UG3AU5" },
		{	   ZORK2,  15, "820308" },
		{	   ZORK2,  17, "820427" },
		{	   ZORK2,  18, "820512" },
		{	   ZORK2,  18, "820517" },
		{	   ZORK2,  19, "820721" },
		{	   ZORK2,  22, "830331" },
		{	   ZORK2,  22, "840518" },
		{	   ZORK2,  23, "830411" },
		{	   ZORK2,  48, "840904" },
		{	   ZORK2,  63, "860811" },
		{	   ZORK3,  10, "820818" },
		{	   ZORK3,  12, "821025" },
		{	   ZORK3,  15, "830331" },
		{	   ZORK3,  15, "840518" },
		{	   ZORK3,  16, "830410" },
		{	   ZORK3,  17, "840727" },
		{	   ZORK3,  25, "860811" },
		{	MINIZORK,   2, "840207" },
		{	MINIZORK,  34, "871124" },
		{	SAMPLER1,  26, "840731" },
		{	SAMPLER1,  53, "850407" },
		{	SAMPLER1,  55, "850823" },
		{	SAMPLER2,  97, "870601" },
		{      ENCHANTER,  10, "830810" },
		{      ENCHANTER,  15, "831107" },
		{      ENCHANTER,  16, "831118" },
		{      ENCHANTER,  16, "840518" },
		{      ENCHANTER,  24, "851118" },
		{      ENCHANTER,  29, "860820" },
		{	SORCERER,  67, "831208" },
		{	SORCERER,  67, "0"      },
		{	SORCERER,  85, "840106" },
		{	SORCERER,   4, "840131" },
		{	SORCERER,   6, "840508" },
		{	SORCERER,  13, "851021" },
		{	SORCERER,  15, "851108" },
		{	SORCERER,  18, "860904" },
		{	SORCERER,  63, "850916" },
		{	SORCERER,  87, "860904" },
		{   SPELLBREAKER,  63, "850916" },
		{   SPELLBREAKER,  87, "860904" },
		{     PLANETFALL,   1, "830517" },
		{     PLANETFALL,  20, "830708" },
		{     PLANETFALL,  26, "831014" },
		{     PLANETFALL,  29, "840118" },
		{     PLANETFALL,  37, "851003" },
		{     PLANETFALL,  39, "880501" },
		{     PLANETFALL,  10, "880531" },
		{    STATIONFALL,   1, "861017" },
		{    STATIONFALL,  63, "870218" },
		{    STATIONFALL,  87, "870326" },
		{    STATIONFALL, 107, "870430" },
		{	BALLYHOO,  97, "851218" },
		{	BALLYHOO,  99, "861014" },
		{    BORDER_ZONE,   9, "871008" },
		{	    AMFV,   1, "841226" },
		{	    AMFV,  47, "850313" },
		{	    AMFV,  84, "850516" },
		{	    AMFV, 131, "850628" },
		{	    AMFV,  77, "850814" },
		{	    AMFV,  79, "851122" },
		{	    HHGG, 108, "840809" },
		{	    HHGG, 119, "840822" },
		{	    HHGG,  47, "840914" },
		{	    HHGG,  56, "841221" },
		{	    HHGG,  58, "851002" },
		{	    HHGG,  59, "851108" },
		{	    HHGG,  60, "861002" },
		{	    HHGG,  31, "871119" },
		{	    LGOP,   0, "BLOWN!" },
		{	    LGOP,   1, "851008" },
		{	    LGOP,  57, "860121" },
		{	    LGOP, 118, "860325" },
		{	    LGOP, 160, "860521" },
		{	    LGOP,  50, "860711" },
		{	    LGOP,  59, "860730" },
		{	    LGOP,  59, "861114" },
		{	    LGOP,   4, "880405" },
		{	 SUSPECT,  14, "841005" },
		{	 SUSPECT,  18, "850222" },
		{       SHERLOCK,  97, "871026" },
		{       SHERLOCK,  21, "871214" },
		{       SHERLOCK,  22, "880112" },
		{       SHERLOCK,  26, "880127" },
		{       SHERLOCK,   4, "880324" },
		{    BEYOND_ZORK,   1, "870412" },
		{    BEYOND_ZORK,   1, "870715" },
		{    BEYOND_ZORK,  47, "870915" },
		{    BEYOND_ZORK,  49, "870917" },
		{    BEYOND_ZORK,  51, "870923" },
		{    BEYOND_ZORK,  57, "871221" },
		{    BEYOND_ZORK,  60, "880610" },
		{      ZORK_ZERO,   0, "870831" },
		{      ZORK_ZERO,   1, "871030" },
		{      ZORK_ZERO,  74, "880114" },
		{      ZORK_ZERO,  96, "880224" },
		{      ZORK_ZERO, 153, "880510" },
		{      ZORK_ZERO, 242, "880830" },
		{      ZORK_ZERO, 242, "880901" },
		{      ZORK_ZERO, 296, "881019" },
		{      ZORK_ZERO, 366, "890323" },
		{      ZORK_ZERO, 383, "890602" },
		{      ZORK_ZERO, 392, "890714" },
		{      ZORK_ZERO, 393, "890714" },
		{         SHOGUN, 292, "890314" },
		{         SHOGUN, 295, "890321" },
		{         SHOGUN, 311, "890510" },
		{         SHOGUN, 320, "890627" },
		{         SHOGUN, 321, "890629" },
		{         SHOGUN, 322, "890706" },
		{         ARTHUR,  40, "890502" },
		{         ARTHUR,  41, "890504" },
		{         ARTHUR,  54, "890606" },
		{         ARTHUR,  63, "890622" },
		{         ARTHUR,  74, "890714" },
		{        JOURNEY,  46, "880603" },
		{        JOURNEY,   2, "890303" },
		{        JOURNEY,  26, "890316" },
		{        JOURNEY,  30, "890322" },
		{        JOURNEY,  51, "890522" },
		{        JOURNEY,  54, "890526" },
		{        JOURNEY,  77, "890616" },
		{        JOURNEY,  79, "890627" },
		{        JOURNEY,  83, "890706" },
		{ LURKING_HORROR, 203, "870506" },
		{ LURKING_HORROR, 219, "870912" },
		{ LURKING_HORROR, 221, "870918" },
		{    BUREAUCRACY,  86, "870212" },
		{    BUREAUCRACY, 116, "870602" },
		{    BUREAUCRACY, 160, "880521" },
		{        TRINITY,   1, "851202" },
		{        TRINITY,   1, "860221" },
		{        TRINITY,  14, "860313" },
		{        TRINITY,  11, "860509" },
		{        TRINITY,  12, "860926" },
		{        TRINITY,  15, "870628" },
		{        UNKNOWN,   0, "------" }
	};
	/* INDENT-ON */

	/* Ensure undo slots don't exceed maximum */
	if (f_setup.undo_slots > MAX_UNDO_SLOTS) {
		sprintf(errorstring, "Maximum undo slots is %d", MAX_UNDO_SLOTS);
		os_fatal(errorstring);
	}

	/* Open story file */
	if ((story_fp = os_load_story()) == NULL)
		os_fatal("Cannot open story file");

	/* Allocate memory for story header */
	if ((zmp = (zbyte huge *) zmalloc(64)) == NULL)
		os_fatal("Out of memory");

	/* Load header into memory */
#ifdef TOPS20
	/* One byte at a time for 36-bit sanitization */
	for (i = 0; i < 64 ; i++) {
		if (fread(zmp + i, 1, 1, story_fp) != 1) {
			os_fatal ("Story file read error");
		}
		zmp[i] &= 0xff; /* No nine-bit craziness here! */
	}
#else
	if (fread(zmp, 1, 64, story_fp) != 64)
		os_fatal("Story file read error");
#endif

	/* Copy header fields to global variables */
	LOW_BYTE(H_VERSION, z_header.version);
	if (z_header.version < V1 || z_header.version > V8)
		os_fatal("Unknown Z-code version");
	LOW_BYTE (H_CONFIG, z_header.config);
	if (z_header.version == V3 && (z_header.config & CONFIG_BYTE_SWAPPED))
		os_fatal("Byte swapped story file");

	LOW_WORD(H_RELEASE, z_header.release);
	LOW_WORD(H_RESIDENT_SIZE, z_header.resident_size);
	LOW_WORD(H_START_PC, z_header.start_pc);
	LOW_WORD(H_DICTIONARY, z_header.dictionary);
	LOW_WORD(H_OBJECTS, z_header.objects);
	LOW_WORD(H_GLOBALS, z_header.globals);
	LOW_WORD(H_DYNAMIC_SIZE, z_header.dynamic_size);
	LOW_WORD(H_FLAGS, z_header.flags);

	for (i = 0, addr = H_SERIAL; i < 6; i++, addr++)
		LOW_BYTE(addr, z_header.serial[i]);

	/* Auto-detect buggy story files that need special fixes */
	story_id = UNKNOWN;
	for (i = 0; records[i].story_id != UNKNOWN; i++) {
		if (z_header.release == records[i].release) {
			for (j = 0; j < 6; j++) {
				if (z_header.serial[j] != records[i].serial[j])
					goto no_match;
			}
			story_id = records[i].story_id;
		}
		no_match: ; /* null statement */
	}

	LOW_WORD(H_ABBREVIATIONS, z_header.abbreviations);
	LOW_WORD(H_FILE_SIZE, z_header.file_size);

	/* Calculate story file size in bytes */
	if (z_header.file_size != 0) {
		story_size = (long) 2 * z_header.file_size;

		if (z_header.version >= V4)
			story_size *= 2;
		if (z_header.version >= V6)
			story_size *= 2;
	} else { /* some old games lack the file size entry */
		os_storyfile_seek(story_fp, 0, SEEK_END);
		story_size = os_storyfile_tell(story_fp);
		os_storyfile_seek(story_fp, 64, SEEK_SET);
	}

	LOW_WORD(H_CHECKSUM, z_header.checksum);
	LOW_WORD(H_ALPHABET, z_header.alphabet);
	LOW_WORD(H_FUNCTIONS_OFFSET, z_header.functions_offset);
	LOW_WORD(H_STRINGS_OFFSET, z_header.strings_offset);
	LOW_WORD(H_TERMINATING_KEYS, z_header.terminating_keys);
	LOW_WORD(H_EXTENSION_TABLE, z_header.extension_table);

	/* Zork Zero Macintosh doesn't have the graphics flag set */
	if (story_id == ZORK_ZERO && z_header.release == 296)
		z_header.flags |= GRAPHICS_FLAG;

	/* Adjust opcode tables */
	if (z_header.version <= V4) {
		op0_opcodes[0x09] = z_pop;
		op1_opcodes[0x0f] = z_not;
	} else {
		op0_opcodes[0x09] = z_catch;
		op1_opcodes[0x0f] = z_call_n;
	}

	/* Allocate memory for story data */
	if ((zmp = (zbyte huge *) zrealloc(zmp, story_size, 64)) == NULL)
		os_fatal("Out of memory");

#ifdef TOPS20
	/* Load and sanitize story file one byte at a time. */
	for (size = 64; size < story_size; size++) {
		if (fread(zmp + size, 1, 1, story_fp) != 1) {
			os_fatal("Story file read error");
		}
		zmp[size] &= 0xff; /* No nine-bit craziness here! */
	}
#else
	/* Load story file in chunks of 32KB */
	n = 0x8000;
	for (size = 64; size < story_size; size += n) {
		if (story_size - size < 0x8000)
			n = (unsigned) (story_size - size);
		SET_PC(size);
		if (fread(pcp, 1, n, story_fp) != n)
			os_fatal("Story file read error");
	}
#endif

	/* Read header extension table */
	z_header.x_table_size = get_header_extension(HX_TABLE_SIZE);
	z_header.x_unicode_table = get_header_extension(HX_UNICODE_TABLE);

#ifdef TOPS20
	/* Internal verification; is this where the PDP-10 is blowing up? */
	/* Sum all bytes in story file except header bytes */
	fseek(story_fp, 64, SEEK_SET);
	for (li = 64; li < story_size; li++)
		checksum = (checksum + (fgetc(story_fp) & 0xff)) & 0xffff;
	if (checksum != z_header.checksum)
		os_fatal("Checksum failed!");
#endif
} /* init_memory */


/*
 * init_undo
 *
 * Allocate memory for multiple undo. It is important not to occupy
 * all the memory available, since the IO interface may need memory
 * during the game, e.g. for loading sounds or pictures.
 *
 */
void init_undo(void)
{
	void huge *reserved;

	reserved = NULL;	/* makes compilers shut up */

	if (reserve_mem != 0) {
		if ((reserved = zmalloc(reserve_mem)) == NULL)
			return;
	}

	/* Allocate z_header.dynamic_size bytes for previous dynamic
	 * zmp state + 1.5 z_header.dynamic_size for Quetzal diff + 2.
	 */
	/* FIXME UNDO changed a lot since 2.32. May not be correct. */
#ifdef TOPS20
	prev_zmp = malloc(z_header.dynamic_size & 0xffff);
	undo_diff = malloc(((unsigned long)(z_header.dynamic_size & 0xffff) * 3) / 2 + 2);
#else
	prev_zmp = malloc(z_header.dynamic_size);
	undo_diff = malloc(((unsigned long)z_header.dynamic_size * 3) / 2 + 2);
#endif

	if ((undo_diff != NULL) && (prev_zmp != NULL)) {
		memmove (prev_zmp, zmp, z_header.dynamic_size);
	} else {
		f_setup.undo_slots = 0;
		if (prev_zmp != NULL) zfree(prev_zmp);
		if (undo_diff != NULL) zfree(undo_diff);
	}

	if (reserve_mem != 0)
		zfree(reserved);
} /* init_undo */


/*
 * free_undo
 *
 * Free count undo blocks from the beginning of the undo list.
 *
 */
static void free_undo(int count)
{
	undo_t *p;

	if (count > undo_count)
		count = undo_count;
	while (count--) {
		p = first_undo;
		if (curr_undo == first_undo)
			curr_undo = curr_undo->next;
		first_undo = first_undo->next;
		zfree (p);
		undo_count--;
	}
	if (first_undo)
		first_undo->prev = NULL;
	else
		last_undo = NULL;
} /* free_undo */


/*
 * reset_memory
 *
 * Close the story file and deallocate memory.
 *
 */
void reset_memory(void)
{
	if (story_fp != NULL)
		fclose(story_fp);
	story_fp = NULL;

	if (undo_diff) {
		free_undo(undo_count);
		zfree(undo_diff);
		zfree(prev_zmp);
	}

	undo_diff = NULL;
	undo_count = 0;
	prev_zmp = NULL;

	if (zmp)
		zfree(zmp);
	zmp = NULL;
} /* reset_memory */


/*
 * storeb
 *
 * Write a byte value to the dynamic Z-machine memory.
 *
 */
void storeb(zword addr, zbyte value)
{
#ifdef TOPS20
	addr &= 0xffff;
	value &= 0xff;
#endif
	if (addr >= z_header.dynamic_size)
		runtime_error(ERR_STORE_RANGE);

	if (addr == H_FLAGS + 1) {	/* flags register is modified */
		z_header.flags &= ~(SCRIPTING_FLAG | FIXED_FONT_FLAG);
		z_header.flags |= value & (SCRIPTING_FLAG | FIXED_FONT_FLAG);

		if (value & SCRIPTING_FLAG) {
			if (!ostream_script)
				script_open();
		} else {
			if (ostream_script)
				script_close();
		}
		refresh_text_style();
	}
	SET_BYTE(addr, value);
} /* storeb */


/*
 * storew
 *
 * Write a word value to the dynamic Z-machine memory.
 *
 */
void storew(zword addr, zword value)
{
#ifdef TOPS20
	addr &= 0xffff;
	value &= 0xffff;
#endif
	storeb((zword) (addr + 0), hi (value));
	storeb((zword) (addr + 1), lo (value));
} /* storew */


/*
 * z_restart, re-load dynamic area, clear the stack and set the PC.
 *
 * 	no zargs used
 *
 */
void z_restart(void)
{
	static bool first_restart = TRUE;

	flush_buffer();

	os_restart_game(RESTART_BEGIN);

	seed_random(0);

	if (!first_restart) {
		os_storyfile_seek(story_fp, 0, SEEK_SET);
		if (fread(zmp, 1, z_header.dynamic_size, story_fp) != z_header.dynamic_size)
			os_fatal ("Story file read error");
	} else first_restart = FALSE;

	restart_header();
	restart_screen();

	sp = fp = stack + STACK_SIZE;
	frame_count = 0;

	if (z_header.version != V6) {
		long pc = (long) z_header.start_pc;
		SET_PC(pc);
	} else call(z_header.start_pc, 0, NULL, 0);

	os_restart_game (RESTART_END);
} /* z_restart */


/*
 * get_default_name
 *
 * Read a default file name from the memory of the Z-machine and
 * copy it to a string.
 *
 */
static void get_default_name(char *default_name, size_t length, zword addr)
{
	if (addr != 0) {
		zbyte len;
		int i;

		LOW_BYTE(addr, len);
		len = (len >= length) ? (length - 1) : len;

		addr++;

		for (i = 0; i < len; i++) {
			zbyte c;
			LOW_BYTE(addr, c);
			addr++;

			if (c >= 'A' && c <= 'Z')
				c += 'a' - 'A';

			default_name[i] = c;
		}
		default_name[i] = 0;

		if (strchr (default_name, '.') == NULL)
			strncat(default_name, EXT_AUX, length - len - 1);
	} else {
		memset(default_name, 0, length);
		strncpy(default_name, f_setup.aux_name, length - 1);
	}
} /* get_default_name */


/*
 * z_restore, restore [a part of] a Z-machine state from disk
 *
 *	zargs[0] = address of area to restore (optional)
 *	zargs[1] = number of bytes to restore
 *	zargs[2] = address of suggested file name
 *      zargs[3] = 1 if frotz should ask for confirmation of filename
 *		   0 if frotz should silently use provided filename
 *		   default depends on interface
 */
void z_restore(void)
{
#ifdef LATE_ZORK
	/* Modified 2026-10-09: story RESTORE uses the door menu policy. */
	if (door_enabled()) {
		door_restore_menu();
		return;
	}
#endif
	char *new_name;
	char default_name[MAX_FILE_NAME + 1];
	FILE *gfp = NULL;

	zword success = 0;

	if (zargc != 0) {
		/* Get the file name */
		get_default_name(default_name, MAX_FILE_NAME + 1, (zargc >= 3) ? zargs[2] : 0);

		/* Do we have the fourth paramenter?
		 * If the fourth paramenter is 0, skip to the else.
		 * Otherwise, any value is equivalent to 1, do the first.
		 * If no fourth parameter, give a prompt.
		 */
		if ((zargc >= 4) ? zargs[3] : 1) {
			new_name = os_read_file_name(default_name, FILE_LOAD_AUX);
			if (new_name == NULL)
				goto finished;
			free(f_setup.aux_name);
			f_setup.aux_name = strdup(default_name);
		} else {
			new_name = os_read_file_name (default_name, FILE_NO_PROMPT);
			if (new_name == NULL)
				goto finished;
		}

		/* Open auxiliary file */
		if ((gfp = fopen (new_name, "rb")) == NULL)
			goto finished;

		/* Load auxiliary file */
		success = fread (zmp + zargs[0], 1, zargs[1], gfp);

		/* Close auxiliary file */
		fclose (gfp);
	} else {
	/*
	 * long pc;
	 * zword release;
	 * zword addr;
	 * int i;
	 */
		/* Get the file name */
		new_name = os_read_file_name(f_setup.save_name, FILE_RESTORE);
		if (new_name == NULL)
			goto finished;
		zfree(f_setup.save_name);
		f_setup.save_name = strdup(new_name);

		/* Open game file */
		if ((gfp = fopen(new_name, "rb")) == NULL)
			goto finished;
		success = restore_quetzal(gfp, story_fp);
		if ((short) success >= 0) {
			/* Close game file */
			fclose (gfp);
			if ((short) success > 0) {
				zbyte old_screen_rows;
				zbyte old_screen_cols;

				/* In V3, reset the upper window. */
				if (z_header.version == V3)
					split_window(0);

				LOW_BYTE (H_SCREEN_ROWS, old_screen_rows);
				LOW_BYTE (H_SCREEN_COLS, old_screen_cols);

				/* Reload cached header fields. */
				restart_header ();

				/*
				 * Since QUETZAL files may be saved on
				 * many different machines, the screen sizes
				 * may vary a lot. Erasing the status window
				 * seems to cover up most of the
				 * resulting badness.
				 */
				if (z_header.version > V3 && z_header.version != V6
				    && (z_header.screen_rows != old_screen_rows
				    || z_header.screen_cols != old_screen_cols))
					erase_window (1);
			}
		} else
			os_fatal ("Error reading save file");
	}

finished:
	if (gfp == NULL && f_setup.restore_mode)
		os_fatal ("Error reading save file");

	if (z_header.version <= V3)
		branch(success);
	else
		store(success);
} /* z_restore */


/*
 * mem_diff
 *
 * Set diff to a Quetzal-like difference between a and b,
 * copying a to b as we go.  It is assumed that diff points to a
 * buffer which is large enough to hold the diff.
 * mem_size is the number of bytes to compare.
 * Returns the number of bytes copied to diff.
 *
 */
static long mem_diff(zbyte *a, zbyte *b, zword mem_size, zbyte *diff)
{
	unsigned size = mem_size;
	zbyte *p = diff;
	unsigned j;
	zbyte c;

	for (;;) {
		for (j = 0; size > 0 && (c = *a++ ^ *b++) == 0; j++)
			size--;
		if (size == 0) break;
		size--;
		if (j > 0x8000) {
			*p++ = 0;
			*p++ = 0xff;
			*p++ = 0xff;
			j -= 0x8000;
		}
		if (j > 0) {
			*p++ = 0;
			j--;
			if (j <= 0x7f) {
				*p++ = j;
			} else {
				*p++ = (j & 0x7f) | 0x80;
				*p++ = (j & 0x7f80) >> 7;
			}
		}
		*p++ = c;
		*(b - 1) ^= c;
	}
	return p - diff;
} /* mem_diff */


/*
 * mem_undiff
 *
 * Applies a quetzal-like diff to dest
 *
 */
static void mem_undiff(zbyte *diff, long diff_length, zbyte *dest)
{
	zbyte c;

	while (diff_length) {
		c = *diff++;
		diff_length--;
		if (c == 0) {
			unsigned runlen;

			if (!diff_length)
				return;  /* Incomplete run */
			runlen = *diff++;
			diff_length--;
			if (runlen & 0x80) {
				if (!diff_length)
					return; /* Incomplete extended run */
				c = *diff++;
				diff_length--;
				runlen = (runlen & 0x7f) | (((unsigned) c) << 7);
			}
			dest += runlen + 1;
		} else
			*dest++ ^= c;
 	}
} /* mem_undiff */


/*
 * restore_undo
 *
 * This function does the dirty work for z_restore_undo.
 *
 */
int restore_undo(void)
{
	long pc;

	/* undo feature unavailable */
	if (f_setup.undo_slots == 0)
		return -1;

	/* no saved game state */
	if (curr_undo == NULL)
		return 0;

	pc = curr_undo->pc;

	/* undo possible */
	memmove(zmp, prev_zmp, z_header.dynamic_size);
	SET_PC(pc);
	curr_undo->pc = pc;
	sp = stack + STACK_SIZE - curr_undo->stack_size;
	fp = stack + curr_undo->frame_offset;
	frame_count = curr_undo->frame_count;
	mem_undiff((zbyte *) (curr_undo + 1), curr_undo->diff_size, prev_zmp);
	memmove (sp, (zbyte *)(curr_undo + 1) + curr_undo->diff_size,
		curr_undo->stack_size * sizeof (*sp));

	curr_undo = curr_undo->prev;
	restart_header();
	return 2;
} /* restore_undo */


/*
 * z_restore_undo, restore a Z-machine state from memory.
 *
 *	no zargs used
 *
 */
void z_restore_undo(void)
{
	store ((zword) restore_undo ());

} /* z_restore_undo */


/*
 * z_save, save [a part of] the Z-machine state to disk.
 *
 *	zargs[0] = address of memory area to save (optional)
 *	zargs[1] = number of bytes to save
 *	zargs[2] = address of suggested file name
 *      zargs[3] = 1 if frotz should ask for confirmation of filename
 *		   0 if frotz should silently use provided filename
 *		   default depends on interface
 */
void z_save(void)
{
#ifdef LATE_ZORK
	/* Modified 2026-10-09: one atomic manual slot, cosmetic description. */
	if (door_enabled()) {
		branch(door_manual_save());
		return;
	}
#endif
	char *new_name;
	char default_name[MAX_FILE_NAME + 1];
	FILE *gfp;

	zword success = 0;

	if (zargc != 0) {
		/* Get the file name */
		get_default_name(default_name, MAX_FILE_NAME + 1, (zargc >= 3) ? zargs[2] : 0);

		/* Do we have the fourth paramenter?
		 * If the fourth paramenter is 0, skip to the else.
		 * Otherwise, any value is equivalent to 1, do the first.
		 * If no fourth parameter, give a prompt.
		 */
		if ((zargc >= 4) ? zargs[3] : 1) {
			new_name = os_read_file_name(default_name, FILE_SAVE_AUX);
			if (new_name == NULL)
				goto finished;
			free(f_setup.aux_name);
			f_setup.aux_name = strdup(default_name);
		} else {
			new_name = os_read_file_name (default_name, FILE_NO_PROMPT);
			if (new_name == NULL)
				goto finished;
		}

		/* Open auxiliary file */
		if ((gfp = fopen(new_name, "wb")) == NULL)
			goto finished;

		/* Write auxiliary file */
		success = fwrite(zmp + zargs[0], zargs[1], 1, gfp);

		/* Close auxiliary file */
		fclose(gfp);

	} else {
	/*
	 * long pc;
	 * zword addr;
	 * zword nsp, nfp;
	 * int skip;
	 * int i;
	 */
		/* Get the file name */
		new_name = os_read_file_name(f_setup.save_name, FILE_SAVE);
		if (new_name == NULL)
			goto finished;

		free(f_setup.save_name);
		f_setup.save_name = strdup(new_name);

		/* Open game file */
		if ((gfp = fopen(new_name, "wb")) == NULL)
			goto finished;

		success = save_quetzal(gfp, story_fp);

		/* Close game file and check for errors */
		if (fclose(gfp) == EOF || ferror(story_fp)) {
			print_string("Error writing save file\n");
			goto finished;
		}
		/* Success */
		success = 1;
	}

finished:

	if (z_header.version <= V3)
		branch(success);
	else
		store(success);

} /* z_save */


/*
 * save_undo
 *
 * This function does the dirty work for z_save_undo.
 *
 */
int save_undo(void)
{
	long diff_size;
	zword stack_size;
	undo_t huge *p;
	long pc;

	/* undo feature unavailable */
	if (f_setup.undo_slots == 0)
		return -1;

	/* save undo possible */
	while (last_undo != curr_undo) {
		p = last_undo;
		last_undo = last_undo->prev;
		free(p);
		undo_count--;
	}
	if (last_undo)
		last_undo->next = NULL;
	else
		first_undo = NULL;

	if (undo_count == f_setup.undo_slots)
		free_undo(1);

	diff_size = mem_diff(zmp, prev_zmp, z_header.dynamic_size, undo_diff);
	stack_size = stack + STACK_SIZE - sp;
	do {
		p = zmalloc(sizeof (undo_t) + diff_size + stack_size * sizeof (*sp));
		if (p == NULL)
			free_undo(1);
	} while (!p && undo_count);
	if (p == NULL)
		return -1;
	pc = p->pc;
	GET_PC(pc);	/* Turbo C doesn't like seeing p->pc here */
	p->pc = pc;
	p->frame_count = frame_count;
	p->diff_size = diff_size;
	p->stack_size = stack_size;
	p->frame_offset = fp - stack;
	memmove(p + 1, undo_diff, diff_size);
	memmove((zbyte *)(p + 1) + diff_size, sp, stack_size * sizeof (*sp));

	if (!first_undo) {
		p->prev = NULL;
		first_undo = p;
	} else {
		last_undo->next = p;
		p->prev = last_undo;
	}
	p->next = NULL;
	curr_undo = last_undo = p;
	undo_count++;
	return 1;
} /* save_undo */


/*
 * z_save_undo, save the current Z-machine state for a future undo.
 *
 *	no zargs used
 *
 */
void z_save_undo(void)
{
	store((zword) save_undo());
} /* z_save_undo */


/*
 * z_verify, check the story file integrity.
 *
 *	no zargs used
 *
 */
void z_verify (void)
{
	zword checksum = 0;
	long i;

	/* Sum all bytes in story file except header bytes */
	os_storyfile_seek(story_fp, 64, SEEK_SET);
	for (i = 64; i < story_size; i++)
		checksum += fgetc(story_fp);

	/* Branch if the checksums are equal */
#ifdef TOPS20
	checksum &= 0xffff;
#endif
	branch(checksum == z_header.checksum);
} /* z_verify */
