/* late.sh door integration, added 2026-10-09. GPL-2.0-or-later. */
#ifndef FROTZ_DOOR_H
#define FROTZ_DOOR_H
#ifdef LATE_ZORK
#include <stdint.h>
int door_enabled(void);
void door_init(void);
void door_start(void);
void door_input(void);
void door_input_done(void);
int door_manual_save(void);
void door_restore_menu(void);
void door_poll(void);
int door_restore_token(const zchar *);
int door_confirm_menu(void);
uint32_t door_get32(FILE *);
void door_put32(FILE *, uint32_t);
void door_random_write(FILE *);
void door_random_read(FILE *);
int door_windows(FILE *, int);
#endif
#endif
