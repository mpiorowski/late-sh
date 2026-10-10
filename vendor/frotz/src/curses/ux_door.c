/* late.sh Zork door support, added 2026-10-09.
 * Copyright (c) 2026 late.sh contributors.
 * This file is part of Frotz, licensed under GPL-2.0-or-later; see COPYING.
 */
#define _POSIX_C_SOURCE 200809L
#ifndef _XOPEN_SOURCE
#define _XOPEN_SOURCE 700
#endif
#define __UNIX_PORT_FILE
#include "ux_defines.h"
#ifdef USE_NCURSES_H
#include <ncurses.h>
#else
#include <curses.h>
#endif
#include "ux_frotz.h"
#include <ctype.h>
#include <errno.h>
#include <fcntl.h>
#include <signal.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <time.h>
#include <unistd.h>
#include <wchar.h>
#include <wctype.h>

/* The upstream convenience macros omit outer parentheses. */
#undef MAX
#undef MIN
#define MAX(a,b) ((a) > (b) ? (a) : (b))
#define MIN(a,b) ((a) < (b) ? (a) : (b))

extern FILE *door_story_file(void);
extern zword save_quetzal(FILE *, FILE *);
extern zword restore_quetzal(FILE *, FILE *);
extern void restart_header(void);
extern void resize_screen(void);

#define MAX_PAYLOAD (2 * 1024 * 1024)
#define AUTO_FILE "autosave.lz"
#define MANUAL_FILE "manual.lz"
static const char *mode;
static volatile sig_atomic_t stop_requested;
static int describing, reading_story, checkpoint_ok;
static char *last_payload;
static size_t last_size;

uint32_t door_get32(FILE *f)
{
	uint32_t n = 0;
	int i;
	for (i = 0; i < 4; i++) n = (n << 8) | (unsigned char)fgetc(f);
	return n;
}
void door_put32(FILE *f, uint32_t n)
{
	fputc(n >> 24, f); fputc(n >> 16, f); fputc(n >> 8, f); fputc(n, f);
}
static uint32_t digest(const char *p, size_t len)
{
	uint32_t h = 2166136261u;
	while (len--) h = (h ^ (unsigned char)*p++) * 16777619u;
	return h;
}
int door_enabled(void) { return mode != NULL; }
static void request_stop(int sig) { stop_requested = sig; }

/* Fixed path, adjacent exclusive temporary, durable contents, atomic rename. */
static int commit_slot(const char *path, int kind, const char *description,
		       const char *payload, size_t size)
{
	char temp[64];
	FILE *f;
	int fd, ok, i;
	uint64_t now = (uint64_t)time(NULL);
	if (size > MAX_PAYLOAD) return 0;
	snprintf(temp, sizeof(temp), "%s.XXXXXX", path);
	fd = mkstemp(temp);
	if (fd < 0) return 0;
	f = fdopen(fd, "wb");
	if (!f) { close(fd); unlink(temp); return 0; }
	fwrite("LZORK001", 1, 8, f);
	door_put32(f, 1); door_put32(f, kind);
	door_put32(f, z_header.release); door_put32(f, z_header.checksum);
	fwrite(z_header.serial, 1, 6, f); fputc(0, f); fputc(0, f);
	door_put32(f, now >> 32); door_put32(f, now);
	door_put32(f, strlen(description)); door_put32(f, size);
	door_put32(f, digest(payload, size));
	ok = fwrite(payload, 1, size, f) == size && !ferror(f);
	if (fflush(f) || fsync(fd)) ok = 0;
	if (fclose(f)) ok = 0;
	if (ok && rename(temp, path)) ok = 0;
	if (!ok) unlink(temp);
	if (ok && (i = open(".", O_RDONLY)) >= 0) { fsync(i); close(i); }
	return ok;
}

/* Only text and the V3 styles used by these stories; no curses ABI on disk. */
static int screen_state(FILE *f, int load)
{
	uint32_t rows, cols, cy, cx, y, x;
	uint32_t top;
	int old_y, old_x;
	attr_t old_attr;
	short old_pair;
	getyx(stdscr, old_y, old_x);
	wattr_get(stdscr, &old_attr, &old_pair, NULL);
	rows = load ? door_get32(f) : (uint32_t)LINES;
	cols = load ? door_get32(f) : (uint32_t)COLS;
	cy = load ? door_get32(f) : (uint32_t)old_y;
	cx = load ? door_get32(f) : (uint32_t)old_x;
	if (!rows || !cols || rows > 255 || cols > 255 || cy >= rows || cx >= cols)
		return 0;
	if (!load) {
		door_put32(f, rows); door_put32(f, cols);
		door_put32(f, cy); door_put32(f, cx);
	} else erase();
	/* Keep the pending prompt visible when reconnecting on a shorter screen. */
	top = load && cy >= (uint32_t)LINES ? cy - LINES + 1 : 0;
	for (y = 0; y < rows; y++) for (x = 0; x < cols; x++) {
		uint32_t style;
		int k;
		if (load) {
			wchar_t text[6] = {0};
			for (k = 0; k < 5; k++) text[k] = door_get32(f);
			style = door_get32(f);
			if (style > 7) return 0;
			if (text[0] && y >= top && y - top < (uint32_t)LINES
				&& x + MAX(wcwidth(text[0]), 1) <= (uint32_t)COLS) {
				cchar_t cell;
				attr_t attr = (style & 1 ? A_REVERSE : 0)
					| (style & 2 ? A_BOLD : 0) | (style & 4 ? A_UNDERLINE : 0);
				setcchar(&cell, text, attr, 0, NULL);
				mvadd_wch(y - top, x, &cell);
			}
		} else {
			cchar_t cell;
			wchar_t text[CCHARW_MAX] = {0};
			attr_t attr;
			short pair;
			if (mvin_wch(y, x, &cell) == ERR || getcchar(&cell, text, &attr, &pair, NULL) == ERR)
				return 0;
			style = (attr & A_REVERSE ? 1 : 0) | (attr & A_BOLD ? 2 : 0)
				| (attr & A_UNDERLINE ? 4 : 0);
			for (k = 0; k < 5; k++) door_put32(f, k < CCHARW_MAX ? text[k] : 0);
			door_put32(f, style);
			/* Encode continuation cells explicitly, rather than duplicating a
			 * wide character when the screen is restored. */
			for (k = 1; k < wcwidth(text[0]) && x + 1 < cols; k++) {
				int j;
				x++;
				for (j = 0; j < 6; j++) door_put32(f, 0);
			}
		}
	}
	wattr_set(stdscr, old_attr, old_pair, NULL);
	if (load) {
		if (rows != (uint32_t)LINES || cols != (uint32_t)COLS) resize_screen();
		move(MIN(cy - top, (uint32_t)LINES - 1), MIN(cx, (uint32_t)COLS - 1));
		clearok(stdscr, TRUE); refresh();
	} else move(old_y, old_x);
	return !ferror(f) && !feof(f);
}

static int make_payload(const char *description, char **data, size_t *size)
{
	FILE *f, *q;
	char *vm = NULL;
	size_t vm_size = 0;
	int i, ok;
	/* Quetzal's writer seeks from offset zero; give it its own stream. */
	q = open_memstream(&vm, &vm_size);
	if (!q) return 0;
	ok = save_quetzal(q, door_story_file());
	/* Writer backpatches chunk lengths; publish the complete memory stream. */
	if (fseek(q, 0, SEEK_END)) ok = 0;
	if (fclose(q)) ok = 0;
	if (!ok) { free(vm); return 0; }
	f = open_memstream(data, size);
	if (!f) { free(vm); return 0; }
	fwrite(description, 1, strlen(description), f);
	door_put32(f, zargc);
	for (i = 0; i < 8; i++) door_put32(f, zargs[i]);
	door_random_write(f);
	ok = door_windows(f, 0) && screen_state(f, 0);
	door_put32(f, vm_size);
	if (fwrite(vm, 1, vm_size, f) != vm_size || ferror(f)) ok = 0;
	if (fclose(f)) ok = 0;
	free(vm);
	return ok;
}

typedef struct {
	uint32_t kind, desc_len, size;
	uint64_t saved_at;
	char *data;
} Slot;
static uint32_t at32(const char *p)
{
	const unsigned char *b = (const unsigned char *)p;
	return (uint32_t)b[0] << 24 | (uint32_t)b[1] << 16 | (uint32_t)b[2] << 8 | b[3];
}
static int valid_payload(const Slot *s)
{
	size_t pos = s->desc_len, end, i;
	uint32_t rows, cols, vm_size, chunk, length;
	const char *p = s->data;
	if (pos + 664 > s->size || at32(p + pos) > 4) return 0;
	if (s->kind == 1 && (at32(p + pos) < 2 || at32(p + pos + 4) >= z_header.dynamic_size
		|| at32(p + pos + 8) >= z_header.dynamic_size)) return 0;
	for (i = 1; i <= 8; i++) if (at32(p + pos + i * 4) > 65535) return 0;
	pos += 52; /* operands and RNG */
	if (at32(p + pos) >= 8) return 0;
	for (i = 1; i <= 144; i++) if (at32(p + pos + i * 4) > 65535) return 0;
	pos += 580;
	rows = at32(p + pos); cols = at32(p + pos + 4);
	if (!rows || !cols || rows > 255 || cols > 255
		|| at32(p + pos + 8) >= rows || at32(p + pos + 12) >= cols) return 0;
	pos += 16;
	end = pos + (size_t)rows * cols * 24;
	if (end + 16 > s->size) return 0;
	for (; pos < end; pos += 24) {
		for (i = 0; i < 5; i++) {
			uint32_t ch = at32(p + pos + i * 4);
			if (ch > 0x10ffff || (ch >= 0xd800 && ch <= 0xdfff)) return 0;
		}
		if (at32(p + pos + 20) > 7) return 0;
	}
	vm_size = at32(p + pos); pos += 4;
	if (vm_size < 12 || pos + vm_size != s->size || memcmp(p + pos, "FORM", 4)
		|| at32(p + pos + 4) != vm_size - 8 || memcmp(p + pos + 8, "IFZS", 4)) return 0;
	pos += 12;
	/* Bound all chunks and PCs before the upstream Quetzal reader touches VM memory. */
	while (pos < s->size) {
		if (pos + 8 > s->size) return 0;
		chunk = at32(p + pos); length = at32(p + pos + 4); pos += 8;
		if (length > s->size - pos) return 0;
		if (chunk == 0x49466864) { /* IFhd */
			uint32_t pc;
			if (length < 13) return 0;
			pc = (unsigned char)p[pos + 10] << 16 | (unsigned char)p[pos + 11] << 8
				| (unsigned char)p[pos + 12];
			if (pc >= (uint32_t)story_size) return 0;
		}
		if (chunk == 0x53746b73) { /* Stks: saved function return PCs */
			size_t frame = pos;
			if (length < 8) return 0;
			while (frame < pos + length) {
				uint32_t info, used, pc;
				if (frame + 8 > pos + length) return 0;
				info = at32(p + frame); pc = info >> 8;
				if (pc >= (uint32_t)story_size) return 0;
				used = ((unsigned char)p[frame + 6] << 8 | (unsigned char)p[frame + 7]) + (info & 15);
				frame += 8 + (size_t)used * 2;
				if (frame > pos + length) return 0;
			}
		}
		pos += length + (length & 1);
	}
	return pos == s->size;
}
static int read_slot(const char *path, int expected, Slot *s)
{
	FILE *f = fopen(path, "rb");
	char magic[8], serial[6];
	uint32_t format, release, checksum, hash;
	int ok;
	memset(s, 0, sizeof(*s));
	if (!f) return 0;
	ok = fread(magic, 1, 8, f) == 8 && !memcmp(magic, "LZORK001", 8);
	format = door_get32(f); s->kind = door_get32(f);
	release = door_get32(f); checksum = door_get32(f);
	ok = fread(serial, 1, 6, f) == 6 && ok;
	if (fgetc(f) != 0 || fgetc(f) != 0) ok = 0;
	s->saved_at = (uint64_t)door_get32(f) << 32;
	s->saved_at |= door_get32(f);
	s->desc_len = door_get32(f); s->size = door_get32(f); hash = door_get32(f);
	if (!ok || format != 1 || s->kind != (uint32_t)expected
	    || release != z_header.release || checksum != z_header.checksum
	    || memcmp(serial, z_header.serial, 6) || s->desc_len > 512
	    || s->size < s->desc_len + 652 || s->size > MAX_PAYLOAD) {
		fclose(f); return 0;
	}
	s->data = malloc(s->size);
	ok = s->data && fread(s->data, 1, s->size, f) == s->size
		&& fgetc(f) == EOF && !ferror(f) && digest(s->data, s->size) == hash;
	fclose(f);
	if (ok) ok = valid_payload(s);
	if (!ok) { free(s->data); s->data = NULL; }
	return ok;
}
static void json_description(const Slot *s)
{
	uint32_t i;
	putchar('"');
	for (i = 0; i < s->desc_len; i++) {
		unsigned char c = s->data[i];
		if (c == '"' || c == '\\') putchar('\\');
		if (c >= 32 && c != 127) putchar(c);
	}
	putchar('"');
}

void door_init(void)
{
	struct sigaction sa;
	Slot s;
	mode = getenv("LATE_FROTZ_DOOR");
	if (!mode || !*mode) { mode = NULL; return; }
	if (z_header.version != V3) { fprintf(stderr, "Door mode requires a V3 story\n"); exit(21); }
	if (!strcmp(mode, "inspect-auto") || !strcmp(mode, "inspect-manual")) {
		int manual = !strcmp(mode, "inspect-manual");
		const char *path = manual ? MANUAL_FILE : AUTO_FILE;
		struct stat st;
		if (stat(path, &st)) {
			puts(errno == ENOENT ? "{\"status\":\"missing\"}" : "{\"status\":\"unavailable\"}");
		} else if (access(path, R_OK)) {
			puts("{\"status\":\"unavailable\"}");
		} else if (read_slot(path, manual ? 2 : 1, &s)) {
			printf("{\"status\":\"ready\",\"saved_at\":%llu,\"description\":", (unsigned long long)s.saved_at);
			json_description(&s); puts("}"); free(s.data);
		} else puts("{\"status\":\"invalid\"}");
		exit(0);
	}
	if (strcmp(mode, "new") && strcmp(mode, "auto") && strcmp(mode, "manual")) {
		fprintf(stderr, "Invalid LATE_FROTZ_DOOR mode\n"); exit(21);
	}
	memset(&sa, 0, sizeof(sa)); sa.sa_handler = request_stop;
	sigemptyset(&sa.sa_mask);
	sigaction(SIGHUP, &sa, NULL); sigaction(SIGTERM, &sa, NULL);
	sigaction(SIGUSR1, &sa, NULL);
}

void door_start(void)
{
	Slot s;
	FILE *f, *q;
	uint32_t vm_size;
	long offset;
	int i, ok;
	if (!mode || !strcmp(mode, "new")) return;
	if (!read_slot(!strcmp(mode, "auto") ? AUTO_FILE : MANUAL_FILE,
			!strcmp(mode, "auto") ? 1 : 2, &s)) {
		fprintf(stderr, "Selected Zork save is invalid or unavailable\n"); os_quit(21);
	}
	f = fmemopen(s.data, s.size, "rb");
	if (!f) os_quit(21);
	fseek(f, s.desc_len, SEEK_SET);
	zargc = door_get32(f);
	for (i = 0; i < 8; i++) zargs[i] = door_get32(f);
	door_random_read(f);
	ok = zargc >= 0 && zargc <= 8 && door_windows(f, 1) && screen_state(f, 1);
	vm_size = door_get32(f); offset = ftell(f);
	if (!ok || offset < 0 || vm_size < 12 || (size_t)offset + vm_size != s.size) {
		fprintf(stderr, "Invalid save continuation: ok=%d offset=%ld vm=%u size=%u\n", ok, offset, vm_size, s.size);
		os_quit(21);
	}
	q = fmemopen(s.data + offset, vm_size, "rb");
	ok = q ? restore_quetzal(q, door_story_file()) : 0;
	if (ok != 2) {
		fprintf(stderr, "Invalid save VM image (%d)\n", ok); os_quit(21);
	}
	fclose(q); fclose(f);
	restart_header();
	free(s.data);
	if (s.kind == 1) z_read();
	else branch(2);
}

void door_input(void)
{
	char *data = NULL;
	size_t size = 0;
	int ok;
	if (!mode) return;
	flush_buffer();
	ok = make_payload("", &data, &size);
	if (ok && checkpoint_ok && size == last_size && !memcmp(data, last_payload, size)) {
		free(data); reading_story = 1; refresh(); return;
	}
	checkpoint_ok = ok && commit_slot(AUTO_FILE, 1, "", data, size);
	if (ok) { free(last_payload); last_payload = data; last_size = size; }
	else free(data);
	/* Publish the pending prompt after its checkpoint has committed. */
	refresh();
	if (!checkpoint_ok) {
		print_string("\nWarning: automatic save failed; the previous checkpoint is retained.\n");
		flush_buffer(); refresh();
	}
	reading_story = 1;
	door_poll();
}

void door_input_done(void) { reading_story = 0; }

static int read_description(char *description)
{
	wchar_t text[129] = {0};
	wint_t key;
	int len = 0, y, x, start, width, i, result = 0;
	mbstate_t encoding;
	describing = 1;
	print_string("One manual slot. Saving replaces it. Description:\n"); flush_buffer();
	getyx(stdscr, y, x);
	wtimeout(stdscr, 100);
	for (;;) {
		os_tick();
		getmaxyx(stdscr, y, width);
		y--;
		start = len; width = 0;
		while (start > 0 && width + MAX(wcwidth(text[start - 1]), 0) < COLS - 2) {
			start--;
			width += MAX(wcwidth(text[start]), 0);
		}
		move(y, 0); clrtoeol();
		if (len > start) addnwstr(text + start, len - start);
		refresh();
		i = get_wch(&key);
		if (i == ERR) continue;
		if (key == 27) break;
		if (key == '\n' || key == '\r' || (i == KEY_CODE_YES && key == KEY_ENTER)) {
			result = 1; break;
		}
		if (key == 127 || key == 8 || (i == KEY_CODE_YES && key == KEY_BACKSPACE)) {
			if (len) text[--len] = 0;
		} else if (key == 21) { len = 0; text[0] = 0; }
		else if (i != KEY_CODE_YES && key <= 0x10ffff && !(key >= 0xd800 && key <= 0xdfff)
			 && !iswcntrl(key) && len < 128) {
			text[len++] = key; text[len] = 0;
		}
	}
	wtimeout(stdscr, -1);
	memset(&encoding, 0, sizeof(encoding));
	x = 0;
	for (i = 0; i < len; i++) {
		char bytes[MB_LEN_MAX];
		size_t count = wcrtomb(bytes, text[i], &encoding);
		if (count == (size_t)-1 || x + count > 512) { result = 0; break; }
		memcpy(description + x, bytes, count); x += count;
	}
	description[x] = 0;
	describing = 0;
	print_string("\n"); flush_buffer();
	return result;
}
int door_manual_save(void)
{
	char description[513], *data = NULL;
	size_t size = 0;
	int ok;
	reading_story = 0;
	if (!read_description(description)) return 0;
	ok = make_payload(description, &data, &size)
		&& commit_slot(MANUAL_FILE, 2, description, data, size);
	free(data);
	if (!ok) print_string("Manual save failed; the previous manual slot is retained.\n");
	return ok;
}
int door_restore_token(const zchar *text)
{
	const char *word = "restore";
	int i = 0;
	if (!mode || !reading_story || describing) return 0;
	while (*text && *text <= 32) text++;
	while (word[i] && text[i] < 128 && tolower(text[i]) == word[i]) i++;
	return !word[i] && (!text[i] || text[i] <= 32);
}
int door_confirm_menu(void)
{
	zchar answer;
	/* Overlay without changing the VM window cursor or scrolling story text. */
	move(LINES - 1, 0); clrtoeol();
	addnstr("Return to this Zork's menu? [y/N] ", COLS - 1); refresh();
	answer = os_read_key(0, TRUE);
	return answer == 'y' || answer == 'Y';
}
void door_restore_menu(void)
{
	if (!checkpoint_ok && last_payload)
		checkpoint_ok = commit_slot(AUTO_FILE, 1, "", last_payload, last_size);
	if (checkpoint_ok) os_quit(20);
	print_string("\nCannot return: automatic save failed. Try again after storage is available.\n");
	flush_buffer(); refresh();
}
void door_poll(void)
{
	int sig = stop_requested;
	if (!mode || !sig) return;
	if (sig == SIGUSR1 && !reading_story) return;
	stop_requested = 0;
	if (sig == SIGUSR1) door_restore_menu();
	else os_quit(0);
}
