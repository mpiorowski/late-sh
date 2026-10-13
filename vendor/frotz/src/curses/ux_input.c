/*
 * ux_input.c - Unix interface, input functions
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


#define __UNIX_PORT_FILE

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <limits.h>
#include <libgen.h>
#include <ctype.h>

#include <errno.h>
#include <sys/time.h>
#include <sys/select.h>

#include "ux_defines.h"

#ifdef USE_NCURSES_H
#include <ncurses.h>
#else
#include <curses.h>
#endif

#include "ux_frotz.h"

#ifdef USE_UTF8
#include <wctype.h>
#endif

#ifndef NO_SOUND
#include "ux_sema.h"
extern ux_sem_t sound_done;
#endif

static int start_of_prev_word(int, const zchar*);
static int end_of_next_word(int, const zchar*, int);

static struct timeval global_timeout;

/* Some special characters. */
#define MOD_CTRL 0x40
#define MOD_META 0x80
#define CHR_DEL (MOD_CTRL ^'?')

/* These are useful for circular buffers.
 */
#define RING_DEC( ptr, beg, end) (ptr > (beg) ? --ptr : (ptr = (end)))
#define RING_INC( ptr, beg, end) (ptr < (end) ? ++ptr : (ptr = (beg)))

#define MAX_HISTORY 20
static zchar *history_buffer[MAX_HISTORY];
static zchar **history_next = history_buffer; /* Next available slot. */
static zchar **history_view = history_buffer; /* What the user is looking at. */
#define history_end (history_buffer + MAX_HISTORY - 1)

extern bool is_terminator (zchar);
extern void read_string (int, zchar *);
extern int completion (const zchar *, zchar *);


/*
 * unix_set_global_timeout
 *
 * This sets up a time structure to determine when unix_read_char should
 * return zero (representing input timeout).  When current system time
 * equals global_timeout, boom.
 *
 */
static void unix_set_global_timeout(int timeout)
{
	if (!timeout)
		global_timeout.tv_sec = 0;
	else {
		gettimeofday(&global_timeout, NULL);
		global_timeout.tv_sec += (timeout/10);
		global_timeout.tv_usec += ((timeout%10)*100000);
		if (global_timeout.tv_usec > 999999) {
			global_timeout.tv_sec++;
			global_timeout.tv_usec -= 1000000;
		}
	}
	return;
} /* unix_set_global_timeout */


/*
 * Time left until input timeout.  Return whether an input timeout
 * is in effect and it it is, set diff to the time left until the
 * timeout elapses or zero if it has already elapsed.  If false is returned,
 * diff is not modified, otherwise it is set to a non-negative value.
 */
static bool timeout_left(struct timeval *diff)
{
	struct timeval now;

	if (global_timeout.tv_sec == 0)
		return false;
	gettimeofday( &now, NULL);
	diff->tv_usec = global_timeout.tv_usec - now.tv_usec;
	if (diff->tv_usec < 0) { /* Carry */
		now.tv_sec++;
		diff->tv_usec += 1000000;
	}
	diff->tv_sec = global_timeout.tv_sec - now.tv_sec;
	if (diff->tv_sec < 0)
		diff->tv_sec = diff->tv_usec = 0;
	return true;
} /* timeout_left */


/*
 * os_tick
 *
 * This is something of a catch-all for things that need to be done
 * perodically.  Currently it turn off audio when a sound is finished.
 * It will eventually rewrite the output when the terminal is resized.
 */
void os_tick()
{
#ifdef LATE_ZORK
	/* Modified 2026-10-09: act on shutdown requests in normal execution. */
	door_poll();
#endif
	while (terminal_resized) {
		terminal_resized = 0;
		unix_resize_display();
	}

#ifndef NO_SOUND

	if (f_setup.sound) {
		if (ux_sem_trywait(&sound_done) == 0)
			end_of_sound();
	}
#endif
} /* os_tick */


/*
 * unix_read_char
 *
 * This uses the curses getch() routine to get the next character
 * typed, and returns it.  It returns values which the standard
 * considers to be legal input, and also returns editing and frotz hot
 * keys.  If called with extkeys set it will also return line-editing
 * keys like INSERT etc.
 *
 * If unix_set_global_timeout has been used to set a global timeout
 * this routine may also return ZC_TIME_OUT if input times out.
 */
static int unix_read_char(int extkeys)
{
#ifdef NCURSES_MOUSE_VERSION
	MEVENT mouse_event;
#endif

#ifdef USE_UTF8
	wint_t c;
#else
	int c;
#endif
	int sel, fd = fileno(stdin);
	fd_set rsel;
	struct timeval tval, *t_left, maxwait;


	while(1) {
		/* Wait with select so that we get interrupted on SIGWINCH. */
        	FD_ZERO(&rsel);
		FD_SET(fd, &rsel);
		os_tick();
		refresh();

		/*
		 * If the timeout is 0, we still want to call os_tick
		 * periodically.
		 *
		 * Based on experimentation, 10 msec seems to strike a balance
		 * between cpu usage and not having pauses between sounds
		 */
		maxwait.tv_sec=0;
		maxwait.tv_usec=10000;

		t_left = timeout_left(&tval) ? &tval : NULL;
		/*
		 * if the timeout is zero, we wait forever for input, but if
		 * we are playing a sequence of sounds, we need to periodically
		 * call os_tick().  So if the timeout is zero, wait up to
		 * maxwait for input, but if we get no input continue the
		 * while loop.
		 */
		if (t_left)
			sel = select(fd + 1, &rsel, NULL, NULL, t_left);
		else
			sel = select(fd + 1, &rsel, NULL, NULL, &maxwait);

		if (terminal_resized)
			continue;
		switch (sel) {
		case -1:
			if (errno != EINTR)
				os_fatal(strerror(errno));
			continue;
		case 0:
			if (t_left == NULL)
				/*
				 * The timeout was 0 (wait forever)
				 * but we need to call  os_tick to handle
				 * sound sequences
				 */
				continue;
            		return ZC_TIME_OUT;
        	}

		timeout(0);
#ifdef USE_UTF8
		sel = get_wch(&c);
#else
		c = getch();
#endif

		/* Catch 98% of all input right here... */
		if ((c >= ZC_ASCII_MIN && c <= ZC_ASCII_MAX) || (!u_setup.plain_ascii
			&& c >= ZC_LATIN1_MIN && c <= ZC_LATIN1_MAX))
			return c;

		/* ...and the other 2% makes up 98% of the code. :( */
#ifdef USE_UTF8
		if (sel != KEY_CODE_YES && c >= ZC_LATIN1_MIN) {
			if (c > 0xffff) continue;
			return c;
		}
#endif

		/* On many terminals the backspace key returns DEL. */
#ifdef USE_UTF8
		if (c == (wint_t)erasechar()) return ZC_BACKSPACE;;
#else
		if (c == (int)erasechar()) return ZC_BACKSPACE;;
#endif
		switch(c) {
		/* This should not happen because select said we have input. */
		case ERR:
		/* Ignore NUL (ctrl+space or ctrl+@ on many terminals)
		 * because it would be misinterpreted as timeout
		 * (ZC_TIME_OUT == 0).
		 */
		case 0:
		/* Ncurses appears to produce KEY_RESIZE even if we handle
		 * SIGWINCH ourselves.
		 */
#ifdef KEY_RESIZE
		case KEY_RESIZE:
#endif
			continue;

#ifdef NCURSES_MOUSE_VERSION
		case KEY_MOUSE:
			if (getmouse(&mouse_event) == OK) {
				if (mouse_event.bstate & BUTTON1_RELEASED) continue;
				mouse_x = mouse_event.x + 1;
				mouse_y = mouse_event.y + 1;
				return (mouse_event.bstate & BUTTON1_DOUBLE_CLICKED ? ZC_DOUBLE_CLICK : ZC_SINGLE_CLICK);
			}
			continue;
#endif
		/* Screen decluttering. */
		case MOD_CTRL ^ 'L':
		case MOD_CTRL ^ 'R':
			clearok(curscr, 1);
			refresh();
			clearok(curscr, 0);
			continue;
		/* Lucian P. Smith reports KEY_ENTER on Irix 5.3.  LF has never
		 * been reported, but I'm leaving it in just in case.
		 */
		case '\n':
		case '\r':
		case KEY_ENTER:
		return ZC_RETURN;
		/* I've seen KEY_BACKSPACE returned on some terminals. */
		case KEY_BACKSPACE:
		case '\b':
			return ZC_BACKSPACE;
		/* On terminals with 8-bit character sets or 7-bit connections
		 * "Alt-Foo" may be returned as an escape followed by the ASCII
		 * value of the letter.  We have to decide here whether to
		 * return a single escape or a frotz hot key.
		 */
		case ZC_ESCAPE:
#ifdef USE_UTF8
		nodelay(stdscr, TRUE);
		if (get_wch(&c) == ERR){
			c = ERR;
		}
		nodelay(stdscr, FALSE);
#else
		nodelay(stdscr, TRUE); c = getch(); nodelay(stdscr, FALSE);
#endif
			switch(c) {
			case ERR: return ZC_ESCAPE;
			case 'p': return ZC_HKEY_PLAYBACK;
			case 'r': return ZC_HKEY_RECORD;
			case 's': return ZC_HKEY_SEED;
			case 'u': return ZC_HKEY_UNDO;
			case 'n': return ZC_HKEY_RESTART;
			case 'x': return ZC_HKEY_QUIT;
			case 'd': return ZC_HKEY_DEBUG;
			case 'h': return ZC_HKEY_HELP;
			case 'f': return ZC_WORD_RIGHT;
			case 'b': return ZC_WORD_LEFT;
			default: continue;	/* Ignore unknown combinations. */
			}
		/* The standard function key block. */
		case KEY_UP: return ZC_ARROW_UP;
		case KEY_DOWN: return ZC_ARROW_DOWN;
		case KEY_LEFT: return ZC_ARROW_LEFT;
		case KEY_RIGHT: return ZC_ARROW_RIGHT;
		case KEY_F(1): return ZC_FKEY_MIN;
		case KEY_F(2): return ZC_FKEY_MIN + 1;
		case KEY_F(3): return ZC_FKEY_MIN + 2;
		case KEY_F(4): return ZC_FKEY_MIN + 3;
		case KEY_F(5): return ZC_FKEY_MIN + 4;
		case KEY_F(6): return ZC_FKEY_MIN + 5;
		case KEY_F(7): return ZC_FKEY_MIN + 6;
		case KEY_F(8): return ZC_FKEY_MIN + 7;
		case KEY_F(9): return ZC_FKEY_MIN + 8;
		case KEY_F(10): return ZC_FKEY_MIN + 9;
		case KEY_F(11): return ZC_FKEY_MIN + 10;
		case KEY_F(12): return ZC_FKEY_MIN + 11;
		/* Curses can't differentiate keypad numbers from cursor keys,
		 * which is annoying, as cursor and keypad keys have
		 * nothing to do with each other on, say, a vt200.
		 * So we can only read 1, 3, 5, 7 and 9 from the keypad.  This
		 * would be so silly that we choose not to provide keypad
		 * keys at all.
		 */

		/* Catch the meta key on those plain old ASCII terminals where
		 * it sets the high bit.  This only works in
		 * u_setup.plain_ascii mode: otherwise these character codes
		 * would have been interpreted according to ISO-Latin-1
		 * earlier.
		 */
		case MOD_META | 'p': return ZC_HKEY_PLAYBACK;
		case MOD_META | 'r': return ZC_HKEY_RECORD;
		case MOD_META | 's': return ZC_HKEY_SEED;
		case MOD_META | 'u': return ZC_HKEY_UNDO;
		case MOD_META | 'n': return ZC_HKEY_RESTART;
		case MOD_META | 'x': return ZC_HKEY_QUIT;
		case MOD_META | 'd': return ZC_HKEY_DEBUG;
		case MOD_META | 'h': return ZC_HKEY_HELP;
		case MOD_META | 'f': return ZC_WORD_RIGHT;
		case MOD_META | 'b': return ZC_WORD_LEFT;

		/* these are the UNIX line-editing characters */
		case MOD_CTRL ^ 'B': return ZC_ARROW_LEFT;
		/* use ^C to clear line anywhere it doesn't send SIGINT */
		case MOD_CTRL ^ 'C': return ZC_ESCAPE;
		case MOD_CTRL ^ 'F': return ZC_ARROW_RIGHT;
		case MOD_CTRL ^ 'P': return ZC_ARROW_UP;
		case MOD_CTRL ^ 'N': return ZC_ARROW_DOWN;

		case MOD_CTRL ^ 'A': c = KEY_HOME; break;
		case MOD_CTRL ^ 'E': c = KEY_END; break;
		case MOD_CTRL ^ 'D': c = KEY_DC; break;
		case MOD_CTRL ^ 'K': c = KEY_EOL; break;
		case MOD_CTRL ^ 'U': c = ZC_DEL_TO_BOL; break;
		case MOD_CTRL ^ 'W': c = ZC_DEL_WORD; break;

		/* In raw mode we need to take care of this as well. */
		case MOD_CTRL ^ 'Z':
			unix_suspend_program();
			continue;

		/* use ^Q to immediately exit. */
		case MOD_CTRL ^ 'Q': os_quit(EXIT_SUCCESS);

		default: break; /* Who knows? */
		}

		/* Control-N through Control-U happen to map to the frotz hot
		 * key codes, but not in any mnemonic manner.  It annoys an
		 * emacs user (or this one anyway) when he tries out of habit
		 * to use one of the emacs keys that isn't implemented and he
		 * gets a random hot key function.  It's less jarring to catch
		 * them and do nothing.  [APP]
		 */
		if ((c >= ZC_HKEY_MIN) && (c <= ZC_HKEY_MAX))
			continue;

		/* Finally, if we're in full line mode (os_read_line), we
		 * might return codes which aren't legal Z-machine keys but
		 * are used by the editor.
		 */
		if (extkeys) return c;
	}
} /* unix_read_char */



/*
 * zcharstrlen
 *
 * A version of strlen() for dealing with ZSCII strings
 */
size_t zcharstrlen(zchar *str)
{
	size_t ret = 0;

	while (str[ret] != 0)
		ret++;
	return ret;
} /* zcharstrlen */


/* zcharstrncpy
 *
 * A version of strncpy() for dealing with ZSCII strings
 */
zchar *zcharstrncpy(zchar *dest, zchar *src, size_t n)
{
	size_t i;

	for (i = 0; i < n && src[i] != '\0'; i++)
		dest[i] = src[i];
	for ( ; i < n; i++)
		dest[i] = 0;

	return dest;
} /* zcharstrncpy */


/* zcharstrncmp
 *
 * A version of strncmp() for dealing with ZSCII strings
 */
int zcharstrncmp(zchar *s1, zchar *s2, size_t n)
{
	zchar u1, u2;
	while (n-- > 0) {
		u1 = *s1++;
		u2 = *s2++;
		if (u1 != u2)
			return u1 - u2;
		if (u1 == 0)
			return 0;
	}
	return 0;
} /* zcharstrncmp */


/*
 * unix_add_to_history
 *
 * Add the given string to the next available history buffer slot.
 *
 */
static void unix_add_to_history(zchar *str)
{

	if (*history_next != NULL)
		free( *history_next);
	*history_next = (zchar *)malloc((zcharstrlen(str) + 1) * sizeof(zchar));
	zcharstrncpy( *history_next, str, zcharstrlen(str) + 1);
	RING_INC( history_next, history_buffer, history_end);
	history_view = history_next; /* Reset user frame after each line */

	return;
} /* unix_add_to_history */


/*
 * unix_history_back
 *
 * Copy last available string to str, if possible.  Return 1 if successful.
 * Only lines of at most maxlen characters will be considered.  In addition
 * the first searchlen characters of the history entry must match those of str.
 */
static int unix_history_back(zchar *str, int searchlen, int maxlen)
{
	zchar **prev = history_view;

	do {
		RING_DEC( history_view, history_buffer, history_end);
		if ((history_view == history_next) || (*history_view == NULL)) {
			os_beep(BEEP_HIGH);
			history_view = prev;
			return 0;
		}
	} while (zcharstrlen( *history_view) > (size_t) maxlen
	    || (searchlen != 0 && zcharstrncmp(str, *history_view, searchlen)));
	zcharstrncpy(str + searchlen, *history_view + searchlen,
		(size_t) maxlen - (zcharstrlen(str) + searchlen));
	return 1;
} /* unix_history_back */


/*
 * unix_history_forward
 *
 * Opposite of unix_history_back, and works in the same way.
 */
static int unix_history_forward(zchar *str, int searchlen, int maxlen)
{
	zchar **prev = history_view;

	do {
		RING_INC( history_view, history_buffer, history_end);
		if ((history_view == history_next) || (*history_view == NULL)) {
			os_beep(BEEP_HIGH);
			history_view = prev;
			return 0;
		}
	} while (zcharstrlen( *history_view) > (size_t) maxlen
	    || (searchlen != 0 && zcharstrncmp(str, *history_view, searchlen)));
	zcharstrncpy(str + searchlen, *history_view + searchlen,
		(size_t) maxlen - (zcharstrlen(str) + searchlen));
	return 1;
} /* unix_history_forward */


/*
 * scrnmove
 *
 * In the row of the cursor, move n characters starting at src to dest.
 *
 */
static void scrnmove(int dest, int src, int n)
{
	int col, x, y;

	getyx(stdscr, y, x);
	if (src > dest) {
		for (col = src; col < src + n; col++) {
#ifdef USE_UTF8
			cchar_t ch;
			mvin_wch(y, col, &ch);
			mvadd_wch(y, col - src + dest, &ch);
#else
			chtype ch = mvinch(y, col);
			mvaddch(y, col - src + dest, ch);
#endif
		}
	} else if (src < dest) {
		for (col = src + n - 1; col >= src; col--) {
#ifdef USE_UTF8
			cchar_t ch;
			mvin_wch(y, col, &ch);
			mvadd_wch(y, col - src + dest, &ch);
#else
			chtype ch = mvinch(y, col);
			mvaddch(y, col - src + dest, ch);
#endif
		}
	}
	move(y, x);
	return;
} /* scrnmove */


/*
 * spaceset
 *
 * In the row of the cursor, set n characters starting at start to space.
 *
 *
 */
static void spaceset(int start, int n)
{
	int y, x;

	getyx(stdscr, y, x);
	while (n--)
		mvaddch(y, start + n, ' ');
	move(y, x);
	return;
} /* spaceset */


#ifdef USE_UTF8
/*
 * utf8_mvaddnstr
 *
 * Like mvaddnstr except modified to deal with UTF-8 and ZSCII
 *
 */
static void utf8_mvaddnstr(int y, int x, zchar * buf, int n)
{
	zchar *bp = buf;

	move(y,x);
	while(*bp && (n > 0)) {
		if(*bp < ZC_LATIN1_MIN) {
			addch(*bp);
		} else {
			if(*bp > 0x7ff) {
				addch(0xe0 | ((*bp >> 12) & 0xf));
				addch(0x80 | ((*bp >> 6) & 0x3f));
				addch(0x80 | (*bp & 0x3f));
			} else {
				addch(0xc0 | ((*bp >> 6) & 0x1f));
				addch(0x80 | (*bp & 0x3f));
			}
		}
		bp++;
		n--;
	}
} /* utf8_mvaddnstr */


/*
 * utf8_mvaddstr
 *
 * Like mvaddstr except modified to deal with UTF-8 and ZSCII
 *
 */
static void utf8_mvaddstr(int y, int x, zchar * buf)
{
	utf8_mvaddnstr(y,x,buf,zcharstrlen(buf));
} /* utf8_mvaddstr */
#endif /* USE_UTF8 */


/*
 * os_read_line
 *
 * Read a line of input from the keyboard into a buffer. The buffer
 * may already be primed with some text. In this case, the "initial"
 * text is already displayed on the screen. After the input action
 * is complete, the function returns with the terminating key value.
 * The length of the input should not exceed "max" characters plus
 * an extra 0 terminator.
 *
 * Terminating keys are the return key (13) and all function keys
 * (see the Specification of the Z-machine) which are accepted by
 * the is_terminator function. Mouse clicks behave like function
 * keys except that the mouse position is stored in global variables
 * "mouse_x" and "mouse_y" (top left coordinates are (1,1)).
 *
 * Furthermore, Frotz introduces some special terminating keys:
 *
 *     ZC_HKEY_KEY_PLAYBACK (Alt-P)
 *     ZC_HKEY_RECORD (Alt-R)
 *     ZC_HKEY_SEED (Alt-S)
 *     ZC_HKEY_UNDO (Alt-U)
 *     ZC_HKEY_RESTART (Alt-N, "new game")
 *     ZC_HKEY_QUIT (Alt-X, "exit game")
 *     ZC_HKEY_DEBUGGING (Alt-D)
 *     ZC_HKEY_HELP (Alt-H)
 *
 * If the timeout argument is not zero, the input gets interrupted
 * after timeout/10 seconds (and the return value is ZC_TIME_OUT).
 *
 * The complete input line including the cursor must fit in "width"
 * screen units.  If the screen width changes during input, width
 * is adjusted by the same amount.  bufmax is not adjusted: buf must
 * contain space for at least bufmax + 1 characters (including final NUL).
 *
 * The function may be called once again to continue after timeouts,
 * misplaced mouse clicks or hot keys. In this case the "continued"
 * flag will be set. This information can be useful if the interface
 * implements input line history.
 *
 * The screen is not scrolled after the return key was pressed. The
 * cursor is at the end of the input line when the function returns.
 *
 * Since Inform 2.2 the helper function "completion" can be called
 * to implement word completion (similar to tcsh under Unix).
 *
 */
zchar os_read_line (int bufmax, zchar *buf, int timeout, int width,
                    int continued)
{
	int ch, y, x;
	int res;
	size_t len = zcharstrlen(buf);
	const int margin = MAX(z_header.screen_width - width, 0);

	/* These are static to allow input continuation to work smoothly. */
	static int scrpos = 0, searchpos = -1, insert_flag = 1;

	/* Set x and y to be at the start of the input area.  */
	getyx(stdscr, y, x);
	x -= len;

	/* Better be careful here or it might segv.  I wonder if we should just
	 * ignore 'continued' and check for len > 0 instead?  Might work better
	 * with Beyond Zork.
	 */
	if (!continued || scrpos > len || searchpos > len) {
		scrpos = len;
		history_view = history_next; /* Reset user's history view. */
		searchpos = -1;		/* -1 means initialize from len. */
		insert_flag = 1;	/* Insert mode is now default. */
	}

	unix_set_global_timeout(timeout);
	for (;;) {
		int x2, max;
		if (scrpos >= width)
			scrpos = width - 1;
		move(y, x + scrpos);
		/* Maybe there's a cleaner way to do this,
		 * but refresh() is  still needed here to
		 * print spaces.  --DG
		 */
		refresh();
		ch = unix_read_char(1);
		getyx(stdscr, y, x2);
		x2++;
		width = z_header.screen_width - margin;
		max = MIN(width, bufmax);
		/* The screen has shrunk and input no longer fits.  Chop. */
		if (len > max) {
			len = max;
			if (scrpos > len)
				scrpos = len;
			if (searchpos > len)
				searchpos = len;
		}
		switch (ch) {
		case ZC_BACKSPACE:	/* Delete preceding character */
			if (scrpos != 0) {
				len--;
				scrpos--;
				searchpos = -1;
				scrnmove(x + scrpos,
					x + scrpos + 1, len - scrpos);
				mvaddch(y, x + len, ' ');
				memmove(buf + scrpos, buf + scrpos + 1,
					(len - scrpos)*sizeof(zchar));
			}
			break;
		case ZC_DEL_WORD:
			if (scrpos != 0) {
				int newoffset =
					start_of_prev_word(scrpos, buf);
				searchpos = -1;
				int delta = scrpos - newoffset;
				int oldlen = len;
				int oldscrpos = scrpos;
				len -= delta;
				scrpos -= delta;
				scrnmove(x + scrpos, x + oldscrpos,
					len - scrpos);
				memmove(buf + scrpos, buf + oldscrpos,
					(len - scrpos)*sizeof(zchar));
				int i = newoffset;
				for (i = len; i <= oldlen ; i++)
					mvaddch(y, x + i, ' ');
			}
			break;
		case ZC_DEL_TO_BOL:
			if (scrpos != 0) {
				searchpos = -1;
				len -= scrpos;
				scrnmove(x, x + scrpos, len);
				memmove(buf, buf + scrpos,
					len*sizeof(zchar));
				int i;
				for (i = len; i <= len + scrpos; i++)
					mvaddch(y, x + i, ' ');
				scrpos = 0;
			}
			break;
		case CHR_DEL:
		case KEY_DC:		/* Delete following character */
			if (scrpos < len) {
				len--; searchpos = -1;
				scrnmove(x + scrpos, x + scrpos + 1,
					len - scrpos);
				mvaddch(y, x + len, ' ');
				memmove(buf + scrpos, buf + scrpos + 1,
				(len - scrpos)*sizeof(zchar));
			}
			continue; /* Don't feed is_terminator bad zchars. */

		case KEY_EOL:	/* Delete from cursor to end of line.  */
			spaceset(x + scrpos, len - scrpos);
			len = scrpos;
			continue;
			case ZC_ESCAPE:		/* Delete whole line */
			spaceset(x, len);
			len = scrpos = 0;
			searchpos = -1;
			history_view = history_next;
			continue;

		/* Cursor motion */
		case ZC_ARROW_LEFT:
			if (scrpos)
				scrpos--;
			continue;
		case ZC_ARROW_RIGHT:
			if (scrpos < len)
				scrpos++;
			continue;
		case KEY_HOME:
			scrpos = 0;
			continue;
		case KEY_END:
			scrpos = len;
			continue;
		case ZC_WORD_RIGHT:
			if (scrpos < len)
				scrpos = end_of_next_word(scrpos, buf, len);
			continue;
		case ZC_WORD_LEFT:
			if (scrpos > 0)
				scrpos = start_of_prev_word(scrpos, buf);
			continue;
		case KEY_IC:	/* Insert Character */
			insert_flag = !insert_flag;
			continue;
		case ZC_ARROW_UP:
		case ZC_ARROW_DOWN:
			if (searchpos < 0)
				searchpos = len;
			if (ch == ZC_ARROW_UP)
				res = unix_history_back(buf, searchpos, max);
			else
				res = unix_history_forward(buf, searchpos, max);

			if (res) {
				spaceset(x, len);
#ifdef USE_UTF8
				utf8_mvaddstr(y, x, buf);
#else
				mvaddstr(y, x, (char *) buf);
#endif
				scrpos = len = zcharstrlen(buf);
			}
			continue;
		/* Passthrough as up/down arrows for Beyond Zork. */
		case KEY_PPAGE: ch = ZC_ARROW_UP; break;
		case KEY_NPAGE: ch = ZC_ARROW_DOWN; break;
		case '\t':
	    /* This really should be fixed to work also in the middle of a
	       sentence. */
			{
			int status;
			zchar extension[10], saved_char;

			saved_char = buf[scrpos];
			buf[scrpos] = '\0';
			status = completion( buf, extension);
			buf[scrpos] = saved_char;

			if (status != 2) {
				int ext_len = zcharstrlen(extension);
				if (ext_len > max - len) {
					ext_len = max - len;
					status = 1;
				}
				memmove(buf + scrpos + ext_len, buf + scrpos,
					(len - scrpos)*sizeof(zchar));
				memmove(buf + scrpos, extension,
					ext_len*sizeof(zchar));
				scrnmove(x + scrpos + ext_len, x + scrpos, len - scrpos);
#ifdef USE_UTF8
				utf8_mvaddnstr(y, x + scrpos, extension,
					ext_len);
#else
				mvaddnstr(y, x + scrpos, (char *)extension,
					ext_len);
#endif
				scrpos += ext_len;
				len += ext_len;
				searchpos = -1;
			}
			if (status) os_beep(BEEP_HIGH);
			}
			continue; /* TAB is invalid as an input character. */
		default:
			/* ASCII or ISO-Latin-1 */
			if ((ch >= ZC_ASCII_MIN && ch <= ZC_ASCII_MAX)
			    || (!u_setup.plain_ascii
			    && ch >= ZC_LATIN1_MIN /*&& ch <= ZC_LATIN1_MAX*/)) {
				searchpos = -1;
				if ((scrpos == max) || (insert_flag && (len == max))) {
					os_beep(BEEP_HIGH);
					continue;
				}
				if (insert_flag && (scrpos < len)) {
					/* move what's there to the right */
					scrnmove(x + scrpos + 1, x + scrpos,
						len - scrpos);
					memmove(buf + scrpos + 1, buf + scrpos,
						(len - scrpos)*sizeof(zchar));
				}
				if (insert_flag || scrpos == len)
					len++;
#ifdef USE_UTF8
				if (ch < ZC_LATIN1_MIN) {
					mvaddch(y, x + scrpos, ch);
				} else if(ch > 0x7ff) {
					mvaddch(y, x + scrpos, 0xe0 |
						((ch >> 12) & 0xf));
					addch(0x80 | ((ch >> 6) & 0x3f));
					addch(0x80 | (ch & 0x3f));
				} else {
					mvaddch(y, x + scrpos, 0xc0 |
						((ch >> 6) & 0x1f));
					addch(0x80 | (ch & 0x3f));
				}
#else
				mvaddch(y, x + scrpos, ch);
#endif
				buf[scrpos++] = ch;
				continue;
			}
		}
		if (is_terminator(ch)) {
			buf[len] = '\0';
#ifdef LATE_ZORK
			/* Modified 2026-10-09: intercept before READ advances the VM. */
			if (ch == ZC_RETURN && door_restore_token(buf)) {
				WINDOW *before = dupwin(stdscr);
				int answer = door_confirm_menu();
				if (before) {
					overwrite(before, stdscr);
					delwin(before);
				}
				spaceset(x, len);
				len = scrpos = 0;
				buf[0] = 0;
				move(y, x);
				refresh();
				if (answer) door_restore_menu();
				continue;
			}
#endif
			if (ch == ZC_RETURN)
				unix_add_to_history(buf);
			/* Games don't know about line editing and might
			 * get confused if the cursor is not at the end
			 * of the input line. */
			move(y, x + len);
			return ch;
		}
	}
} /* os_read_line */


/*
 * os_read_key
 *
 * Read a single character from the keyboard (or a mouse click) and
 * return it. Input aborts after timeout/10 seconds.
 *
 */
zchar os_read_key (int timeout, bool cursor)
{
	zchar c;

	refresh();
	if (!cursor)
		curs_set(0);

	unix_set_global_timeout(timeout);
	c = (zchar) unix_read_char(0);

	if (!cursor)
		curs_set(1);
	return c;
} /* os_read_key */


/*
 * os_read_file_name
 *
 * Return the name of a file.  Flag can be one of:
 *
 *    FILE_SAVE      - Save game file
 *    FILE_RESTORE   - Restore game file
 *    FILE_SCRIPT    - Transcript file
 *    FILE_RECORD    - Command file for recording
 *    FILE_PLAYBACK  - Command file for playback
 *    FILE_SAVE_AUX  - Save auxiliary ("preferred settings") file
 *    FILE_LOAD_AUX  - Load auxiliary ("preferred settings") file
 *    FILE_NO_PROMPT - Return file without prompting the user
 *
 * The length of the file name is limited by MAX_FILE_NAME. Ideally
 * an interpreter should open a file requester to ask for the file
 * name. If it is unable to do that then this function should call
 * print_string and read_string to ask for a file name.
 *
 * Return value is NULL if there was a problem.
 */
char *os_read_file_name (const char *default_name, int flag)
{
#ifdef LATE_ZORK
	/* Modified 2026-10-09: door saves bypass filenames; other files disabled. */
	if (door_enabled()) return NULL;
#endif
	FILE *fp;
	int saved_replay = istream_replay;
	int saved_record = ostream_record;
 	int i;
	char *tempname;
	zchar answer[4];
	char path_separator[2];
	char file_name[FILENAME_MAX + 1];
	char *ext;

	path_separator[0] = PATH_SEPARATOR;
	path_separator[1] = 0;

	/* Turn off playback and recording temporarily */
	istream_replay = 0;
	ostream_record = 0;

	/* If we're restoring a game before the interpreter starts,
	 * our filename is already provided.  Just go ahead silently.
	 */
	if (f_setup.restore_mode || flag == FILE_NO_PROMPT) {
		file_name[0]=0;
	} else {
		print_string ("Enter a file name.\nDefault is \"");

		/* After successfully reading or writing a file, the default
		 * name gets saved and used again the next time something is
		 * to be read or written.  In restricted mode, we don't want
		 * to show any path prepended to the actual file name.  Here,
		 * we strip out that path and display only the filename.
		 */
		if (f_setup.restricted_path != NULL) {
			tempname = basename((char *)default_name);
			print_string(tempname);
		} else
#ifdef USE_UTF8
		{
			zchar z;
			i = 0;
			while (default_name[i]) {
				if((default_name[i] & 0x80) == 0) {
					print_char(default_name[i++]);
				} else if((default_name[i] & 0xe0) == 0xc0 ) {
					z = default_name[i++] & 0x1f;
					z = (z << 6) | (default_name[i++] & 0x3f);
					print_char(z);
				} else if((default_name[i] & 0xf0) == 0xe0 ) {
					z = default_name[i++] & 0xf;
					z = (z << 6) | (default_name[i++] & 0x3f);
					z = (z << 6) | (default_name[i++] & 0x3f);
					print_char(z);
				} else {
					i+=4;
					print_char('?');
				}
			}
		}
#else
		print_string (default_name);
#endif
		print_string ("\": ");
#ifdef USE_UTF8
		{
			zchar z_name[FILENAME_MAX + 1];
			zchar *zp;
			read_string (FILENAME_MAX - EXT_LENGTH, z_name);
			i = 0;
			zp = z_name;
			while (*zp) {
				if(*zp <= 0x7f) {
					if (i > FILENAME_MAX)
						break;
					file_name[i++] = *zp;
				} else if(*zp > 0x7ff) {
					if (i > FILENAME_MAX - 2)
						break;
					file_name[i++] = 0xe0 | ((*zp >> 12) & 0xf);
					file_name[i++] = 0x80 | ((*zp >> 6) & 0x3f);
					file_name[i++] = 0x80 | (*zp & 0x3f);
				} else {
					if (i > FILENAME_MAX - 1)
						break;
					file_name[i++] = 0xc0 | ((*zp >> 6) & 0x1f);
					file_name[i++] = 0x80 | (*zp & 0x3f);
				}
				zp++;
			}
			file_name[i] = 0;
		}
#else
		read_string (FILENAME_MAX - EXT_LENGTH, (zchar *)file_name);
#endif
	}

	/* Return failure if path provided when in restricted mode.
	 * I think this is better than accepting a path/filename
	 * and stripping off the path.
	 */
	if (f_setup.restricted_path) {
		tempname = dirname(file_name);
		if (strlen(tempname) > 1)
			return NULL;
	}

	/* Use the default name if nothing was typed */
	if (file_name[0] == 0) {
		/* If FILE_NO_PROMPT, restrict to currect directory. */
		/* If FILE_NO_PROMPT and using restricted path, then */
		/*   nothing more needs to be done to restrict the   */
		/*   file access there. */
		if (flag == FILE_NO_PROMPT && f_setup.restricted_path == NULL) {
			tempname = basename((char *)default_name);
			strncpy(file_name, tempname, FILENAME_MAX);
		} else
			strncpy (file_name, default_name, FILENAME_MAX);
	}

	/* If we're restricted to one directory, strip any leading path left
	 * over from a previous call to os_read_file_name(), then prepend
	 * the prescribed path to the filename.  Hostile leading paths won't
	 * get this far.  Instead we return failure a few lines above here if
	 * someone tries it.
	 */
	if (f_setup.restricted_path != NULL) {
		for (i = strlen(file_name); i > 0; i--) {
			if (file_name[i] == PATH_SEPARATOR) {
				i++;
				break;
			}
		}
		tempname = strdup(file_name + i);
		strncpy(file_name, f_setup.restricted_path, FILENAME_MAX);

		/* Make sure the final character is the path separator. */
		if (file_name[strlen(file_name)-1] != PATH_SEPARATOR) {
			strncat(file_name, path_separator, FILENAME_MAX - strlen(file_name) - 2);
		}
		strncat(file_name, tempname, strlen(file_name) - strlen(tempname) - 1);
	}

	ext = strrchr(file_name, '.');
	if (flag == FILE_NO_PROMPT) {
		if (strncmp(ext, EXT_AUX, 4)) {
			os_warn("Blocked unprompted access of %s. Should only be %s files.", file_name, EXT_AUX);
			return NULL;
		}
	}

	/* Add appropriate filename extensions if not already there. */
	if (flag == FILE_SAVE || flag == FILE_RESTORE) {
		if (ext == NULL || strncmp(ext, EXT_SAVE, EXT_LENGTH) != 0) {
			strncat(file_name, EXT_SAVE, EXT_LENGTH);
		}
	} else if (flag == FILE_SCRIPT) {
		if (ext == NULL || strncmp(ext, EXT_SCRIPT, EXT_LENGTH) != 0) {
			strncat(file_name, EXT_SCRIPT, EXT_LENGTH);
		}
	} else if (flag == FILE_SAVE_AUX || flag == FILE_LOAD_AUX) {
		 if (ext == NULL || strncmp(ext, EXT_AUX, EXT_LENGTH) != 0) {
			strncat(file_name, EXT_AUX, EXT_LENGTH);
		}
	} else if (flag == FILE_RECORD || flag == FILE_PLAYBACK) {
		if (ext == NULL || strncmp(ext, EXT_COMMAND, EXT_LENGTH) != 0) {
			strncat(file_name, EXT_COMMAND, EXT_LENGTH);
		}
	}

	/* Warn if overwriting a file. */
	if ((flag == FILE_SAVE || flag == FILE_SAVE_AUX ||
	    flag == FILE_RECORD || flag == FILE_SCRIPT)
	    && ((fp = fopen(file_name, "rb")) != NULL)) {
		fclose (fp);
		print_string("Overwrite existing file? ");
		read_string(4, answer);
		if (tolower(answer[0] != 'y'))
			return NULL;
	}

	/* Restore state of playback and recording */
	istream_replay = saved_replay;
	ostream_record = saved_record;

	return strdup(file_name);
} /* os_read_file_name */


/*
 * os_read_mouse
 *
 * Store the mouse position in the global variables "mouse_x" and
 * "mouse_y" and return the mouse buttons currently pressed.
 *
 */
zword os_read_mouse (void)
{
	/* INCOMPLETE */
	return 0;
} /* os_read_mouse */


/*
 * Search for start of preceding word
 * param currpos marker position
 * param buf input buffer
 * returns new position
 */
static int start_of_prev_word(int currpos, const zchar* buf) {
	int i, j;
	for (i = currpos - 1; i > 0 && buf[i] == ' '; i--) {}
	j = i;
	for (; i > 0 && buf[i] != ' '; i--) {}
	if (i < j && i != 0)
		i += 1;
	return i;
} /* start_of_prev_word */


/*
 * Search for end of next word
 * param currpos marker position
 * param buf input buffer
 * param len length of buf
 * returns new position
 */
static int end_of_next_word(int currpos, const zchar* buf, int len) {
	int i;
	for (i = currpos; i < len && buf[i] == ' '; i++) {}
	for (; i < len && buf[i] != ' '; i++) {}
	return i;
} /* end_of_next_word */
