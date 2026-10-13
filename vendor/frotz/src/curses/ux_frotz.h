/*
 * ux_frotz.h
 *
 * Unix interface, declarations, definitions, and defaults
 *
 */

#ifndef UX_FROTZ_H
#define UX_FROTZ_H

#include <signal.h>

#include "../common/frotz.h"
#include "../blorb/blorb.h"
#include "../blorb/blorblow.h"
#include "ux_setup.h"

#define MASTER_CONFIG		"frotz.conf"
#define USER_CONFIG		".frotzrc"
#define ASCII_DEF		1
#define ATTRIB_ASSIG_DEF	0
#define ATTRIB_TEST_DEF		0
#define COLOR_DEF		1
#define ERROR_HALT_DEF		0
#define EXPAND_DEF		0
#define PIRACY_DEF		0
#define TANDY_DEF		0
#define OBJ_MOVE_DEF		0
#define OBJ_LOC_DEF		0
#define BACKGROUND_DEF		BLACK_COLOUR
#define FOREGROUND_DEF		WHITE_COLOUR
#define HEIGHT_DEF		-1	/* let curses figure it out */
#define CONTEXTLINES_DEF	0
#define WIDTH_DEF		80
#define TWIDTH_DEF		80
#define SEED_DEF		-1
#define SLOTS_DEF		MAX_UNDO_SLOTS
#define LMARGIN_DEF		0
#define RMARGIN_DEF		0
#define ERR_REPORT_DEF		ERR_REPORT_ONCE
#define	QUETZAL_DEF		1
#define SAVEDIR_DEF		"if-saves"
#define ZCODEPATH_DEF		"/usr/games/zcode:/usr/local/games/zcode"


#define LINELEN		256	/* for getconfig()	*/
#define COMMENT		'#'	/* for config files	*/
#define PATHSEP		':'	/* for pathopen()	*/
#define DIRSEP		'/'	/* for pathopen()	*/

#define EDITMODE_EMACS	0
#define EDITMODE_VI	1

#define PIC_NUMBER	0
#define PIC_WIDTH	2
#define PIC_HEIGHT	4
#define PIC_FLAGS	6
#define PIC_DATA	8
#define PIC_COLOUR	11

enum emph_mode {
	EMPHASIS_ITALIC,
	EMPHASIS_UNDERLINE,
	EMPHASIS_NONE
};


#ifndef EMPHASIS_DEFAULT
#ifdef ITALIC_SUPPORT
#define EMPHASIS_DEFAULT EMPHASIS_ITALIC
#else
#define EMPHASIS_DEFAULT EMPHASIS_UNDERLINE
#endif
#endif

/* Paths where z-files may be found */
#define	PATH1		"ZCODE_PATH"
#define PATH2		"INFOCOM_PATH"

#define MAX(x,y) ((x)>(y)) ? (x) : (y)
#define MIN(x,y) ((x)<(y)) ? (x) : (y)

/* Some regular curses (not ncurses) libraries don't do this correctly. */
#ifndef getmaxyx
#define getmaxyx(w, y, x)	(y) = getmaxy(w), (x) = getmaxx(w)
#endif

extern bool color_enabled;		/* ux_text */

extern char stripped_story_name[FILENAME_MAX+1];
extern char semi_stripped_story_name[FILENAME_MAX+1];
extern char *progname;
extern char *gamepath;	/* use to find sound files */

extern f_setup_t f_setup;
extern u_setup_t u_setup;

extern volatile sig_atomic_t terminal_resized;

/*** Functions specific to the Unix port of Frotz ***/
bool unix_init_pictures(void);		/* ux_pic.c */
/* void unix_init_scrollback(void); */	/* ux_screen.c */
/* void unix_save_screen(int); */		/* ux_screen.c */
/* void unix_do_scrollback(void); */		/* ux_screen.c */
void unix_resize_display(void);		/* ux_screen.c */
void unix_suspend_program(void);        /* ux_screen.c */
void unix_get_terminal_size(void);      /* ux_init.c */

FILE *os_path_open(const char *, const char *);

#endif /* UX_FROTZ_H */
