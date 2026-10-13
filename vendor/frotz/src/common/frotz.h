/*
 * frotz.h
 *
 * Global declarations and definitions
 *
 */

#ifndef FROTZ_H_
#define FROTZ_H_


/******************************************************************************/
/* MSDOS settings */
/******************************************************************************/
/* For now we assume DOS Frotz is always 16-bit */
#if defined __TURBOC__ || defined __WATCOMC__
  #define MSDOS_16BIT
#endif

#ifdef MSDOS_16BIT

#ifdef USE_UTF8
#error UTF-8 is not supported in DOS Frotz
#endif

#include <malloc.h>

#define zmalloc(size)	halloc((size), 1)
#define zfree(p)	hfree(p)

#ifdef __WATCOMC__
/*
 * Open Watcom C's runtime library does not have a function to reallocate
 * "huge" memory blocks --- unlike Borland Turbo C --- so we need to
 * implement our own function to do that.  (Luckily zrealloc is used only in
 * one place in fastmem.c, which simplifies things somewhat.)
 */
#define huge		_huge
void huge *zrealloc(void huge *p, long size, size_t old_size);
#else
#define zrealloc(p, size, old_size) farrealloc((p), (size))
#endif

#endif /* MSDOS_16BIT */
/******************************************************************************/
/******************************************************************************/

#ifndef __UNIX_PORT_FILE
#include <signal.h>

#if __STDC_VERSION__ >= 199901L
#include <stdbool.h>

#ifndef TRUE
#define TRUE true
#endif
#ifndef FALSE
#define FALSE false
#endif
#else
typedef int bool;
#endif
#endif /* __UNIX_PORT_FILE */


/******************************************************************************/
/* For Amiga and Unix */
/******************************************************************************/
#ifndef MSDOS_16BIT

#define huge
#define zmalloc(size)	malloc(size)
#define zfree(p)	free(p)
#define zrealloc(p, size, old_size) realloc((p), (size))

#endif /* !MSDOS_16BIT */
/******************************************************************************/
/******************************************************************************/

/******************************************************************************/
/* For everything */
/******************************************************************************/
#include <stdio.h>

#include "defs.h"
#include "hash.h"

#ifndef TRUE
#define TRUE 1
#endif

#ifndef FALSE
#define FALSE 0
#endif

#ifndef PATH_MAX
#  ifdef MAXPATHLEN                /* defined in <sys/param.h> some systems */
#    define PATH_MAX      MAXPATHLEN
#  else
#    if FILENAME_MAX > 255         /* used like PATH_MAX on some systems */
#      define PATH_MAX    FILENAME_MAX
#    else
#      define PATH_MAX    (FILNAMSIZ - 1)
#    endif
#  endif /* ?MAXPATHLEN */
#endif /* !PATH_MAX */


/* MSDOS and derived systems use a backslash for path separator */
#if defined(MSDOS_16BIT) || defined(WIN32) || defined(_WIN32) || defined(WIN64) || defined(_WIN64) || defined (__CYGWIN__)
#define PATH_SEPARATOR '\\'
#else
#define PATH_SEPARATOR '/'
#endif

typedef unsigned char zbyte;
typedef unsigned short zword;

#if defined(__TURBOC__) || defined(TOPS20)
typedef unsigned long zlong;
#else
#include <stdint.h>
typedef uint32_t zlong;
#endif

#ifndef USE_UTF8
typedef unsigned char zchar;
#else
typedef unsigned short zchar;
#endif

enum story {
	ZORK1,
	ZORK2,
	ZORK3,
	ZORK1G,
	MINIZORK,
	SAMPLER1,
	SAMPLER2,
	ENCHANTER,
	SORCERER,
	SPELLBREAKER,
	PLANETFALL,
	STATIONFALL,
	BALLYHOO,
	BORDER_ZONE,
	AMFV,
	HHGG,
	LGOP,
	SUSPECT,
	BEYOND_ZORK,
	SHERLOCK,
	ZORK_ZERO,
	SHOGUN,
	ARTHUR,
	JOURNEY,
	LURKING_HORROR,
	BUREAUCRACY,
	TRINITY,
	UNKNOWN
};

/*** screen window ***/
typedef struct {
	zword y_pos;
	zword x_pos;
	zword y_size;
	zword x_size;
	zword y_cursor;
	zword x_cursor;
	zword left;
	zword right;
	zword nl_routine;
	zword nl_countdown;
	zword style;
	zword colour;
	zword font;
	zword font_size;
	zword attribute;
	zword line_count;
	zword true_fore;
	zword true_back;
} Zwindow;

#include "setup.h"
#include "missing.h"
#include "unused.h"

/*** Constants that may be set at compile time ***/
#ifndef DEFAULT_UNDO_SLOTS
#define DEFAULT_UNDO_SLOTS 25
#endif
#ifndef MAX_UNDO_SLOTS
#define MAX_UNDO_SLOTS 500
#endif
#ifndef MAX_FILE_NAME
#define MAX_FILE_NAME 80
#endif
#ifndef TEXT_BUFFER_SIZE
#define TEXT_BUFFER_SIZE 275
#endif
#ifndef INPUT_BUFFER_SIZE
#define INPUT_BUFFER_SIZE 200
#endif
#ifndef STACK_SIZE
#define STACK_SIZE 1024
#endif

extern const char build_timestamp[];

/* Assorted filename extensions */
#define EXT_SAVE	".qzl"
#define EXT_SCRIPT	".scr"
#define EXT_BLORB	".blb"
#define EXT_BLORB2	".zblb"
#define EXT_BLORB3	".blorb"
#define EXT_BLORB4	".zblorb"
#define EXT_COMMAND	".rec"
#define EXT_AUX		".aux"
#define EXT_LENGTH	7

#ifndef DEFAULT_SAVE_NAME
#define DEFAULT_SAVE_NAME "story.qzl"
#endif
#ifndef DEFAULT_SCRIPT_NAME
#define DEFAULT_SCRIPT_NAME "story.scr"
#endif
#ifndef DEFAULT_COMMAND_NAME
#define DEFAULT_COMMAND_NAME "story.rec"
#endif
#ifndef DEFAULT_AUXILIARY_NAME
#define DEFAULT_AUXILIARY_NAME "story.aux"
#endif
#ifndef DEFAULT_SAVE_DIR	/* DG */
#define DEFAULT_SAVE_DIR ".frotz-saves"
#endif

/*** Story file header format ***/
#define H_VERSION 0
#define H_CONFIG 1
#define H_RELEASE 2
#define H_RESIDENT_SIZE 4
#define H_START_PC 6
#define H_DICTIONARY 8
#define H_OBJECTS 10
#define H_GLOBALS 12
#define H_DYNAMIC_SIZE 14
#define H_FLAGS 16
#define H_SERIAL 18
#define H_ABBREVIATIONS 24
#define H_FILE_SIZE 26
#define H_CHECKSUM 28
#define H_INTERPRETER_NUMBER 30
#define H_INTERPRETER_VERSION 31
#define H_SCREEN_ROWS 32
#define H_SCREEN_COLS 33
#define H_SCREEN_WIDTH 34
#define H_SCREEN_HEIGHT 36
#define H_FONT_HEIGHT 38 /* this is the font width in V5 */
#define H_FONT_WIDTH 39 /* this is the font height in V5 */
#define H_FUNCTIONS_OFFSET 40
#define H_STRINGS_OFFSET 42
#define H_DEFAULT_BACKGROUND 44
#define H_DEFAULT_FOREGROUND 45
#define H_TERMINATING_KEYS 46
#define H_LINE_WIDTH 48
#define H_STANDARD_HIGH 50
#define H_STANDARD_LOW 51
#define H_ALPHABET 52
#define H_EXTENSION_TABLE 54
#define H_USER_NAME 56

#define HX_TABLE_SIZE 0
#define HX_MOUSE_X 1
#define HX_MOUSE_Y 2
#define HX_UNICODE_TABLE 3

/*** Various Z-machine constants ***/
#define V1 1
#define V2 2
#define V3 3
#define V4 4
#define V5 5
#define V6 6
#define V7 7
#define V8 8

#define CONFIG_BYTE_SWAPPED 0x01 /* Story file is byte swapped         - V3  */
#define CONFIG_TIME         0x02 /* Status line displays time          - V3  */
#define CONFIG_TWODISKS     0x04 /* Story file occupied two disks      - V3  */
#define CONFIG_TANDY        0x08 /* Tandy licensed game                - V3  */
#define CONFIG_NOSTATUSLINE 0x10 /* Interpr can't support status lines - V3  */
#define CONFIG_SPLITSCREEN  0x20 /* Interpr supports split screen mode - V3  */
#define CONFIG_PROPORTIONAL 0x40 /* Interpr uses proportional font     - V3  */

#define CONFIG_COLOUR       0x01 /* Interpr supports colour            - V5+ */
#define CONFIG_PICTURES	    0x02 /* Interpr supports pictures	       - V6  */
#define CONFIG_BOLDFACE     0x04 /* Interpr supports boldface style    - V4+ */
#define CONFIG_EMPHASIS     0x08 /* Interpr supports emphasis style    - V4+ */
#define CONFIG_FIXED        0x10 /* Interpr supports fixed width style - V4+ */
#define CONFIG_SOUND	    0x20 /* Interpr supports sound             - V6  */

#define CONFIG_TIMEDINPUT   0x80 /* Interpr supports timed input       - V4+ */

#define SCRIPTING_FLAG	  0x0001 /* Outputting to transcription file  - V1+ */
#define FIXED_FONT_FLAG   0x0002 /* Use fixed width font               - V3+ */
#define REFRESH_FLAG 	  0x0004 /* Refresh the screen                 - V6  */
#define GRAPHICS_FLAG	  0x0008 /* Game wants to use graphics         - V5+ */
#define OLD_SOUND_FLAG	  0x0010 /* Game wants to use sound effects    - V3  */
#define UNDO_FLAG	  0x0010 /* Game wants to use UNDO feature     - V5+ */
#define MOUSE_FLAG	  0x0020 /* Game wants to use a mouse          - V5+ */
#define COLOUR_FLAG	  0x0040 /* Game wants to use colours          - V5+ */
#define SOUND_FLAG	  0x0080 /* Game wants to use sound effects    - V5+ */
#define MENU_FLAG	  0x0100 /* Game wants to use menus            - V6  */

#define TRANSPARENT_FLAG  0x0001 /* Game wants to use transparency     - V6  */

#define INTERP_DEFAULT 0
#define INTERP_DEC_20 1
#define INTERP_APPLE_IIE 2
#define INTERP_MACINTOSH 3
#define INTERP_AMIGA 4
#define INTERP_ATARI_ST 5
#define INTERP_MSDOS 6
#define INTERP_CBM_128 7
#define INTERP_CBM_64 8
#define INTERP_APPLE_IIC 9
#define INTERP_APPLE_IIGS 10
#define INTERP_TANDY 11

#define DEFAULT_COLOUR 1
#define BLACK_COLOUR 2
#define RED_COLOUR 3
#define GREEN_COLOUR 4
#define YELLOW_COLOUR 5
#define BLUE_COLOUR 6
#define MAGENTA_COLOUR 7
#define CYAN_COLOUR 8
#define WHITE_COLOUR 9
#define GREY_COLOUR 10		/* INTERP_MSDOS only */
#define LIGHTGREY_COLOUR 10 	/* INTERP_AMIGA only */
#define MEDIUMGREY_COLOUR 11 	/* INTERP_AMIGA only */
#define DARKGREY_COLOUR 12 	/* INTERP_AMIGA only */
#define TRANSPARENT_COLOUR 15	/* ZSpec 1.1 */

#define NORMAL_STYLE 0
#define REVERSE_STYLE 1
#define BOLDFACE_STYLE 2
#define EMPHASIS_STYLE 4
#define FIXED_WIDTH_STYLE 8
#define PICTURE_STYLE 16

#define TEXT_FONT 1
#define PICTURE_FONT 2
#define GRAPHICS_FONT 3
#define FIXED_WIDTH_FONT 4

#define BEEP_HIGH	1
#define BEEP_LOW	2

/*** Constants for os_restart_game */
#define RESTART_BEGIN 0
#define RESTART_WPROP_SET 1
#define RESTART_END 2

/*** Character codes ***/
#define ZC_TIME_OUT 0x00
#define ZC_NEW_STYLE 0x01
#define ZC_NEW_FONT 0x02
#define ZC_BACKSPACE 0x08
#define ZC_INDENT 0x09
#define ZC_GAP 0x0b
#define ZC_RETURN 0x0d
#define ZC_HKEY_MIN 0x0e
#define ZC_HKEY_RECORD 0x0e
#define ZC_HKEY_PLAYBACK 0x0f
#define ZC_HKEY_SEED 0x10
#define ZC_HKEY_UNDO 0x11
#define ZC_HKEY_RESTART 0x12
#define ZC_HKEY_QUIT 0x13
#define ZC_HKEY_DEBUG 0x14
#define ZC_HKEY_HELP 0x15
#define ZC_HKEY_MAX 0x15
#define ZC_ESCAPE 0x1b
#define ZC_DEL_WORD 0x1c
#define ZC_WORD_RIGHT 0x1d
#define ZC_WORD_LEFT 0x1e
#define ZC_DEL_TO_BOL 0x1f
#define ZC_ASCII_MIN 0x20
#define ZC_ASCII_MAX 0x7e
#define ZC_BAD 0x7f
#define ZC_ARROW_MIN 0x81
#define ZC_ARROW_UP 0x81
#define ZC_ARROW_DOWN 0x82
#define ZC_ARROW_LEFT 0x83
#define ZC_ARROW_RIGHT 0x84
#define ZC_ARROW_MAX 0x84
#define ZC_FKEY_MIN 0x85
#define ZC_FKEY_F1 0x85
#define ZC_FKEY_F2 0x86
#define ZC_FKEY_F3 0x87
#define ZC_FKEY_F4 0x88
#define ZC_FKEY_F5 0x89
#define ZC_FKEY_F6 0x8a
#define ZC_FKEY_F7 0x8b
#define ZC_FKEY_F8 0x8c
#define ZC_FKEY_F9 0x8d
#define ZC_FKEY_F10 0x8e
#define ZC_FKEY_F11 0x8f
#define ZC_FKEY_F12 0x90
#define ZC_FKEY_MAX 0x90
#define ZC_NUMPAD_MIN 0x91
#define ZC_NUMPAD_0 0x91
#define ZC_NUMPAD_1 0x92
#define ZC_NUMPAD_2 0x93
#define ZC_NUMPAD_3 0x94
#define ZC_NUMPAD_4 0x95
#define ZC_NUMPAD_5 0x96
#define ZC_NUMPAD_6 0x97
#define ZC_NUMPAD_7 0x98
#define ZC_NUMPAD_8 0x99
#define ZC_NUMPAD_9 0x9a
#define ZC_NUMPAD_MAX 0x9a
#define ZC_SINGLE_CLICK 0x9b
#define ZC_DOUBLE_CLICK 0x9c
#define ZC_MENU_CLICK 0x9d
#define ZC_LATIN1_MIN 0xa0
#define ZC_LATIN1_MAX 0xff

/*** File types ***/
#define FILE_RESTORE 0
#define FILE_SAVE 1
#define FILE_SCRIPT 2
#define FILE_PLAYBACK 3
#define FILE_RECORD 4
#define FILE_LOAD_AUX 5
#define FILE_SAVE_AUX 6
#define FILE_NO_PROMPT 7

/*** Data access macros ***/
#ifdef TOPS20
#define SET_BYTE(addr,v)  { zmp[addr] = v & 0xff; }
#define LOW_BYTE(addr,v)  { v = zmp[addr] & 0xff; }
#else
#define SET_BYTE(addr,v)  { zmp[addr] = v; }
#define LOW_BYTE(addr,v)  { v = zmp[addr]; }
#endif
#define CODE_BYTE(v)	  { v = *pcp++;    }


/******************************************************************************/
/* Amiga macros */
/******************************************************************************/
#if defined (AMIGA)

extern zbyte *pcp;
extern zbyte *zmp;

#define lo(v)	((zbyte *)&v)[1]
#define hi(v)	((zbyte *)&v)[0]

#define SET_WORD(addr,v)  { zmp[addr] = hi(v); zmp[addr+1] = lo(v); }
#define LOW_WORD(addr,v)  { hi(v) = zmp[addr]; lo(v) = zmp[addr+1]; }
#define HIGH_WORD(addr,v) { hi(v) = zmp[addr]; lo(v) = zmp[addr+1]; }
#define CODE_WORD(v)      { hi(v) = *pcp++; lo(v) = *pcp++; }
#define GET_PC(v)         { v = pcp - zmp; }
#define SET_PC(v)         { pcp = zmp + v; }

#endif /* AMIGA */
/******************************************************************************/
/******************************************************************************/


/******************************************************************************/
/* MSDOS macros */
/******************************************************************************/
#if defined (MSDOS_16BIT)

#define lo(v)   ((zbyte *)&v)[0]
#define hi(v)   ((zbyte *)&v)[1]

#ifdef __WATCOMC__

extern zbyte _huge *pcp;
extern zbyte _huge *zmp;

zword bswap16(zword x);
#pragma aux bswap16 = "xchg ah, al" parm [ax] value [ax];

/*
 * TODO: make these more efficient (and still correct).
 */
#define SET_WORD(addr, v)	{ *(zword _huge *)(zmp+(addr))=bswap16(v); }
#define LOW_WORD(addr, v)	{ (v)=bswap16(*(zword _huge *)(zmp+(addr))); }
#define HIGH_WORD(addr, v)	{ (v)=bswap16(*(zword _huge *)(zmp+(addr))); }
#define CODE_WORD(v)		{ (v)=bswap16(*(zword _huge *)pcp); pcp+=2; }
#define GET_PC(v)		{ (v) = pcp - zmp; }
#define SET_PC(v)		{ pcp = zmp + (v); }

#else /* !__WATCOMC__ */

extern zbyte *pcp;
extern zbyte *zmp;

/*
 * Turbo C has a strange limitation with passing members of structs to
 * assembly code within a macro.  If more than one struct have members
 * of the same name, then Turbo C is unable to tell the difference.  In
 * other words, suppose you have foo.a and bar.a.  Doing
 * "SET_WORD(H_STUFF, foo.a);" will make Turbo C complain about
 * "Ambiguous member name 'a' in function frobnitz".  The solution is to
 * use the "pseudoregister" _AX to pass values in and out a macro.  Right
 * now, just SET_WORD() and LOW_WORD() are being passed troublesome
 * struct members.
 *
 */
#define SET_WORD(addr, v) do {\
	asm les bx,zmp;\
	asm add bx,addr;\
	_AX = (v); \
	asm xchg al,ah;\
	asm mov es:[bx],ax; } while (0);

#define LOW_WORD(addr,v) do {\
	asm les bx,zmp;\
	asm add bx,addr;\
	asm mov ax,es:[bx];\
	asm xchg al,ah;\
	(v) = _AX; } while (0);

#define HIGH_WORD(addr,v) asm {\
	mov bx,word ptr zmp;\
	add bx,word ptr addr;\
	mov al,bh;\
	mov bh,0;\
	mov ah,0;\
	adc ah,byte ptr addr+2;\
	mov cl,4;\
	shl ax,cl;\
	add ax,word ptr zmp+2;\
	mov es,ax;\
	mov ax,es:[bx];\
	xchg al,ah;\
	mov v,ax }

#define CODE_WORD(v) asm {\
	les bx,pcp;\
	mov ax,es:[bx];\
	xchg al,ah;\
	mov v,ax;\
	add word ptr pcp,2 }

#define GET_PC(v) asm {\
	mov bx,word ptr pcp+2;\
	sub bx,word ptr zmp+2;\
	mov ax,bx;\
	mov cl,4;\
	shl bx,cl;\
	mov cl,12;\
	shr ax,cl;\
	add bx,word ptr pcp;\
	adc al,0;\
	sub bx,word ptr zmp;\
	sbb al,0;\
	mov word ptr v,bx;\
	mov word ptr v+2,ax }

#define SET_PC(v) asm {\
	mov bx,word ptr zmp;\
	add bx,word ptr v;\
	mov al,bh;\
	mov bh,0;\
	mov ah,0;\
	adc ah,byte ptr v+2;\
	mov cl,4;\
	shl ax,cl;\
	add ax,word ptr zmp+2;\
	mov word ptr pcp,bx;\
	mov word ptr pcp+2,ax }

#endif /* !__WATCOMC__ */

#endif /* MSDOS_16BIT */
/******************************************************************************/
/******************************************************************************/


/******************************************************************************/
/* Macros for neither Amiga nor MSDOS... that is Unix et al. */
/******************************************************************************/
#if !defined (AMIGA) && !defined (MSDOS_16BIT)

extern zbyte *pcp;
extern zbyte *zmp;

#define lo(v)	(v & 0xff)

#ifdef TOPS20
#define hi(v)  ((v & 0xff00) >> 8)
#define LOW_WORD(addr,v)  { v = ((zword) ( zmp[addr] & 0xff) << 8) | \
	(zmp[addr+1] & 0xff); }
#define HIGH_WORD(addr,v) { v = ((zword) ( zmp[addr] & 0xff) << 8) | \
	(zmp[addr+1] & 0xff); }
#else
#define hi(v)	(v >> 8)
#define LOW_WORD(addr,v)  { v = ((zword) zmp[addr] << 8) | zmp[addr+1]; }
#define HIGH_WORD(addr,v) { v = ((zword) zmp[addr] << 8) | zmp[addr+1]; }
#endif

#define SET_WORD(addr,v)  { zmp[addr] = hi(v); zmp[addr+1] = lo(v); }
#define CODE_WORD(v)      { v = ((zword) pcp[0] << 8) | pcp[1]; pcp += 2; }
#define GET_PC(v)         { v = pcp - zmp; }
#define SET_PC(v)         { pcp = zmp + v; }

#endif /* !defined (AMIGA) && !defined (MSDOS_16BIT) */

/******************************************************************************/
/******************************************************************************/

#ifdef TOPS20
/*** Convenience zword-to-correctly-signed-sixteen-bit-short ***/
extern short s16(zword);
#endif

/*** Various data ***/

extern enum story story_id;
extern long story_size;

extern zword stack[STACK_SIZE];
extern zword *sp;
extern zword *fp;
extern zword frame_count;

extern zword zargs[8];
extern int zargc;

/* Modified 2026-10-09: optional Zork door hooks, GPL-2.0-or-later. */
#include "door.h"

extern bool ostream_screen;
extern bool ostream_script;
extern bool ostream_memory;
extern bool ostream_record;
extern bool istream_replay;
extern bool message;

extern int cwin;
extern int mwin;

extern int mouse_x;
extern int mouse_y;
extern int menu_selected;
extern int mouse_button;

extern bool enable_wrapping;
extern bool enable_scripting;
extern bool enable_scrolling;
extern bool enable_buffering;

extern bool need_newline_at_exit;

extern char *option_zcode_path;	/* dg */

extern long reserve_mem;

extern int zoptind;
extern int zoptopt;
extern char *zoptarg;

#ifdef TOPS20
/* A weird little TOPS-20 accomodation */
extern bool spurious_getchar;
#endif

/*** Z-machine opcodes ***/
void 	z_add(void);
void 	z_and(void);
void 	z_art_shift(void);
void 	z_buffer_mode(void);
void 	z_call_n(void);
void 	z_call_s(void);
void 	z_catch(void);
void 	z_check_arg_count(void);
void	z_check_unicode(void);
void 	z_clear_attr(void);
void 	z_copy_table(void);
void 	z_dec(void);
void 	z_dec_chk(void);
void 	z_div(void);
void 	z_draw_picture(void);
void 	z_encode_text(void);
void 	z_erase_line(void);
void 	z_erase_picture(void);
void 	z_erase_window(void);
void 	z_get_child(void);
void 	z_get_cursor(void);
void 	z_get_next_prop(void);
void 	z_get_parent(void);
void 	z_get_prop(void);
void 	z_get_prop_addr(void);
void 	z_get_prop_len(void);
void 	z_get_sibling(void);
void 	z_get_wind_prop(void);
void 	z_inc(void);
void 	z_inc_chk(void);
void 	z_input_stream(void);
void 	z_insert_obj(void);
void 	z_je(void);
void 	z_jg(void);
void 	z_jin(void);
void 	z_jl(void);
void 	z_jump(void);
void 	z_jz(void);
void 	z_load(void);
void 	z_loadb(void);
void 	z_loadw(void);
void 	z_log_shift(void);
void 	z_make_menu(void);
void 	z_mod(void);
void 	z_mouse_window(void);
void 	z_move_window(void);
void 	z_mul(void);
void 	z_new_line(void);
void 	z_nop(void);
void 	z_not(void);
void 	z_or(void);
void 	z_output_stream(void);
void 	z_picture_data(void);
void 	z_picture_table(void);
void 	z_piracy(void);
void 	z_pop(void);
void 	z_pop_stack(void);
void 	z_print(void);
void 	z_print_addr(void);
void 	z_print_char(void);
void 	z_print_form(void);
void 	z_print_num(void);
void 	z_print_obj(void);
void 	z_print_paddr(void);
void 	z_print_ret(void);
void 	z_print_table(void);
void	z_print_unicode(void);
void 	z_pull(void);
void 	z_push(void);
void 	z_push_stack(void);
void 	z_put_prop(void);
void 	z_put_wind_prop(void);
void 	z_quit(void);
void 	z_random(void);
void 	z_read(void);
void 	z_read_char(void);
void 	z_read_mouse(void);
void 	z_remove_obj(void);
void 	z_restart(void);
void 	z_restore(void);
void 	z_restore_undo(void);
void 	z_ret(void);
void 	z_ret_popped(void);
void 	z_rfalse(void);
void 	z_rtrue(void);
void 	z_save(void);
void 	z_save_undo(void);
void 	z_scan_table(void);
void 	z_scroll_window(void);
void 	z_set_attr(void);
void 	z_set_font(void);
void 	z_set_colour(void);
void 	z_set_cursor(void);
void 	z_set_margins(void);
void 	z_set_window(void);
void 	z_set_text_style(void);
void	z_set_true_colour(void);
void 	z_show_status(void);
void 	z_sound_effect(void);
void 	z_split_window(void);
void 	z_store(void);
void 	z_storeb(void);
void 	z_storew(void);
void 	z_sub(void);
void 	z_test(void);
void 	z_test_attr(void);
void 	z_throw(void);
void 	z_tokenise(void);
void 	z_verify(void);
void 	z_window_size(void);
void 	z_window_style(void);


/* Definitions and macros for error handling functions and error codes. */
void	init_err(void);
void	_runtime_error(int, bool);
#define runtime_error_repeat(errnum)    _runtime_error(errnum, TRUE)
#define runtime_error(errnum)           _runtime_error(errnum, FALSE)

/* Error codes */
#define ERR_TEXT_BUF_OVF 1	/* Text buffer overflow */
#define ERR_STORE_RANGE 2	/* Store out of dynamic memory */
#define ERR_DIV_ZERO 3		/* Division by zero */
#define ERR_ILL_OBJ 4		/* Illegal object */
#define ERR_ILL_ATTR 5		/* Illegal attribute */
#define ERR_NO_PROP 6		/* No such property */
#define ERR_STK_OVF 7		/* Stack overflow */
#define ERR_ILL_CALL_ADDR 8	/* Call to illegal address */
#define ERR_CALL_NON_RTN 9	/* Call to non-routine */
#define ERR_STK_UNDF 10		/* Stack underflow */
#define ERR_ILL_OPCODE 11	/* Illegal opcode */
#define ERR_BAD_FRAME 12	/* Bad stack frame */
#define ERR_ILL_JUMP_ADDR 13	/* Jump to illegal address */
#define ERR_SAVE_IN_INTER 14	/* Can't save while in interrupt */
#define ERR_STR3_NESTING 15	/* Nesting stream #3 too deep */
#define ERR_ILL_WIN 16		/* Illegal window */
#define ERR_ILL_WIN_PROP 17	/* Illegal window property */
#define ERR_ILL_PRINT_ADDR 18	/* Print at illegal address */
#define ERR_MAX_FATAL 18

/* Less serious errors */
#define ERR_JIN_0 19		/* @jin called with object 0 */
#define ERR_GET_CHILD_0 20	/* @get_child called with object 0 */
#define ERR_GET_PARENT_0 21	/* @get_parent called with object 0 */
#define ERR_GET_SIBLING_0 22	/* @get_sibling called with object 0 */
#define ERR_GET_PROP_ADDR_0 23	/* @get_prop_addr called with object 0 */
#define ERR_GET_PROP_0 24	/* @get_prop called with object 0 */
#define ERR_PUT_PROP_0 25	/* @put_prop called with object 0 */
#define ERR_CLEAR_ATTR_0 26	/* @clear_attr called with object 0 */
#define ERR_SET_ATTR_0 27	/* @set_attr called with object 0 */
#define ERR_TEST_ATTR_0 28	/* @test_attr called with object 0 */
#define ERR_MOVE_OBJECT_0 29	/* @move_object called moving object 0 */
#define ERR_MOVE_OBJECT_TO_0 30	/* @move_object called moving into object 0 */
#define ERR_REMOVE_OBJECT_0 31	/* @remove_object called with object 0 */
#define ERR_GET_NEXT_PROP_0 32	/* @get_next_prop called with object 0 */
#define ERR_PLAY_SOUND 33	/* @play_sound called without SOUND_FLAG or OLD_SOUND_FLAG set */
#define ERR_SOUND_MISSING 34	/* @play_sound called but picture resources are missing */
#define ERR_PICTURE_MISSING 35	/* @draw_picture called but picture resources are missing */
#define ERR_NUM_ERRORS (35)

/*
 * There are four error reporting modes: never report errors;
 * report only the first time a given error type occurs; report
 * every time an error occurs; or treat all errors as fatal
 * errors, killing the interpreter. I strongly recommend
 * "report once" as the default. But you can compile in a
 * different default by changing the definition of
 * ERR_DEFAULT_REPORT_MODE. In any case, the player can
 * specify a report mode on the command line by typing "-Z 0"
 * through "-Z 3".
 *
 */
#define ERR_REPORT_NEVER (0)
#define ERR_REPORT_ONCE (1)
#define ERR_REPORT_ALWAYS (2)
#define ERR_REPORT_FATAL (3)
#define ERR_DEFAULT_REPORT_MODE ERR_REPORT_ONCE

/*** Assorted initialization functions ***/
void   init_header(void);
void   init_setup(void);
void   init_buffer(void);
void   init_process(void);
void   init_sound(void);

/*** Various global functions ***/
zchar	translate_from_zscii(zbyte);
zbyte	translate_to_zscii(zchar);

void 	flush_buffer(void);
void	new_line(void);
void	print_char(zchar);
void	print_num(zword);
void	print_object(zword);
void 	print_string(const char *);
void	reset_window(void);

void 	stream_mssg_on(void);
void 	stream_mssg_off(void);

void	ret(zword);
void 	store(zword);
void 	branch(bool);

void	storeb(zword, zbyte);
void	storew(zword, zword);

void	end_of_sound(void);

int	completion(const zchar *buffer, zchar *result);

bool is_terminator(zchar);
void read_string(int max, zchar *buffer);
bool read_yes_or_no(const char *);

int colour_in_use(zword);

void screen_new_line(void);

int zgetopt(int, char **, const char *);

/*** returns the current window ***/
Zwindow * curwinrec(void);


/*** Interface functions ***/
void 	os_beep(int);
int  	os_char_width(zchar);
int  	os_check_unicode(int, zchar);
void 	os_display_char(zchar);
void 	os_display_string(const zchar *);
void 	os_draw_picture(int, int, int);
void 	os_erase_area(int, int, int, int, int);
void 	os_finish_with_sample(int);
int  	os_font_data(int, int *, int *);
void 	os_init_screen(void);
void	os_init_sound(void);
FILE	*os_load_story(void);
void 	os_more_prompt(void);
int  	os_peek_colour(void);
bool  	os_picture_data(int, int *, int *);
void 	os_prepare_sample(int);
void 	os_process_arguments(int, char *[]);
int	os_random_seed(void);
char  	*os_read_file_name(const char *, int);
zchar	os_read_key(int, bool);
zchar	os_read_line(int, zchar *, int, int, int);
void 	os_reset_screen(void);
void 	os_restart_game(int);
void 	os_scroll_area(int, int, int, int, int);
void 	os_set_colour(int, int);
int	os_from_true_colour(zword);
void 	os_set_cursor(int, int);
void 	os_set_font(int);
void 	os_set_text_style(int);
int	os_get_text_style(void);
void 	os_start_sample(int, int, int, zword);
void 	os_stop_sample(int);
int	os_storyfile_seek(FILE *, long, int);
int	os_storyfile_tell(FILE *);
int  	os_string_width(const zchar *);
zword	os_to_true_colour (int);
void	os_init_setup(void);
void	os_quit(int);

/* Don't use string expansions from the core!  The dumb interface
 * doesn't support it for historical reasons.  Suuport in the interfaces is
 * optional.
 */
void 	os_fatal(const char *, ...);
void 	os_warn(const char *, ...);

/**
 * Called regularly by the interpreter, at least every few instructions
 * (only when interpreting: e.g., not when waiting for input).
 */
void    os_tick(void);

/* Front ends call this if the terminal size changes. */
void    resize_screen(void);

/* This is callable only from resize_screen. */
bool    os_repaint_window(int win, int ypos_old, int ypos_new, int xpos,
				int ysize, int xsize);

#endif
