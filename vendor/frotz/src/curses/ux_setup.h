#ifndef CURSES_UX_SETUP_H
#define CURSES_UX_SETUP_H

#define NON_STD_COLS 238

typedef struct unix_setup_struct {
	int disable_color;
	int force_color;
	int foreground_color;
	int background_color;
	int emphasis_mode;
	int screen_width;
	int screen_height;
	int random_seed;
	int current_text_style;		/* also in ux_text.c and ux_screen.c */
	int curses_active;
	int plain_ascii;
	int current_color;		/* ux_text.c ux_screen.c */
	bool color_enabled;		/* ux_init.c ux_pic.c ux_text.c */
	bool mouse_enabled;
	int interpreter;		/* see frotz.h */
	zlong colours[11];
	zlong nonstdcolours[NON_STD_COLS];
	int nonstdindex;
	char *term;			/* Terminal name, aka $TERM. */
} u_setup_t;

extern u_setup_t u_setup;

#endif
