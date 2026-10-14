/*
 * sf_video.c - SDL interface, graphics functions
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

#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include <SDL.h>

#include "generic.h"

static char banner[256];
static int isfullscreen;
static zlong *sbuffer = NULL;
static int sbpitch;		/* in longs */
static int dirty = 0;
static int ewidth, eheight;
static SDL_Window *window = NULL;
static SDL_Renderer *renderer = NULL;
static SDL_Texture *texture = NULL;
int m_timerinterval = 100;

extern bool sdl_active;

static void sf_quitconf(void);

static bool ApplyPalette(sf_picture *);
static zlong screen_palette[16];

extern z_header_t z_header;

/* clipping region */
static int xmin, xmax, ymin, ymax;

void sf_setclip(int x, int y, int w, int h)
{
	if (x < 0) {
		w += x;
		x = 0;
	}
	if (x + w > ewidth)
		w = ewidth - x;
	if (y < 0) {
		h += y;
		y = 0;
	}
	if (y + h > eheight)
		h = eheight - y;
	xmin = x;
	xmax = x + w;
	ymin = y;
	ymax = y + h;
} /* sf_setclip */


void sf_getclip(int *x, int *y, int *w, int *h)
{
	*x = xmin;
	*y = ymin;
	*w = xmax - xmin;
	*h = ymax - ymin;
} /* sf_getclip */


static int mywcslen(zchar * b)
{
	int n = 0;
	while (*b++)
		n++;
	return n;
} /* mywcslen */


static void myGrefresh(void)
{
	SDL_RenderClear(renderer);
	SDL_RenderCopy(renderer, texture, NULL, NULL);
	SDL_RenderPresent(renderer);
} /* myGrefresh */


void sf_wpixel(int x, int y, zlong c)
{
	if (x < xmin || x >= xmax || y < ymin || y >= ymax)
		return;
	sbuffer[x + sbpitch * y] = c;
	dirty = 1;
} /* sf_wpixel */


zlong sf_rpixel(int x, int y)
{
	if (x < 0 || x >= ewidth || y < 0 || y >= eheight)
		return 0;
	return sbuffer[x + sbpitch * y];
} /* sf_rpixel */

#define MAXCUR 64
static zlong savedcur[MAXCUR];

static void drawthecursor(int x, int y, int onoff)
{
	SF_textsetting *ts = sf_curtextsetting();
	int i, h = ts->font->height(ts->font);
	if (h > MAXCUR)
		h = MAXCUR;
	if (onoff) {
		for (i = 0; i < h; i++) {
			savedcur[i] = sf_rpixel(x, y + i);
			sf_wpixel(x, y + i, ts->fore);
		}
	} else {
		for (i = 0; i < h; i++) {
			sf_wpixel(x, y + i, savedcur[i]);
		}
	}
} /* drawthecursor */


bool sf_IsValidChar(zword c)
{
	if (c >= ZC_ASCII_MIN && c <= ZC_ASCII_MAX)
		return true;
	if (c >= ZC_LATIN1_MIN && c <= ZC_LATIN1_MAX)
		return true;
	if (c >= 0x100)
		return true;
	return false;
} /* sf_IsValidChar */


void sf_drawcursor(bool c)
{
	SF_textsetting *ts = sf_curtextsetting();
	drawthecursor(ts->cx, ts->cy, c);
} /* sf_drawcursor */


void sf_chline(int x, int y, zlong c, int n)
{
	zlong *s;
	if (y < ymin || y >= ymax)
		return;
	if (x < xmin) {
		n += x - xmin;
		x = xmin;
	}
	if (x + n > xmax)
		n = xmax - x;
	if (n <= 0)
		return;
	s = sbuffer + x + sbpitch * y;
	while (n--)
		*s++ = c;
	dirty = 1;
} /* sf_chline */


void sf_cvline(int x, int y, zlong c, int n)
{
	zlong *s;
	if (x < xmin || x >= xmax)
		return;
	if (y < xmin) {
		n += y - ymin;
		y = ymin;
	}
	if (y + n > ymax)
		n = ymax - y;
	if (n <= 0)
		return;
	s = sbuffer + x + sbpitch * y;
	while (n--) {
		*s = c;
		s += sbpitch;
	}
	dirty = 1;
} /* sf_cvline */


zlong sf_blendlinear(int a, zlong s, zlong d)
{
	zlong r;
	r = ((s & 0xff) * a + (d & 0xff) * (256 - a)) >> 8;
	s >>= 8;
	d >>= 8;
	r |= (((s & 0xff) * a + (d & 0xff) * (256 - a)) >> 8) << 8;
	s >>= 8;
	d >>= 8;
	r |= (((s & 0xff) * a + (d & 0xff) * (256 - a)) >> 8) << 16;
	return r;
} /* sf_blendlinear */


void sf_writeglyph(SF_glyph * g)
{
	SF_textsetting *ts = sf_curtextsetting();

	int i, j, m;

	int w = g->dx;
	int weff = g->xof + g->w;
	zbyte *bmp = (zbyte *) (&(g->bitmap[0]));
	int h = g->h;
	int nby = (g->w + 7) / 8;
	int byw = g->w;

	int x = ts->cx;
	int y = ts->cy;

	int dxpre = g->xof;
	int dypre = ts->font->ascent(ts->font) - h - (int)g->yof;

	int height = ts->font->height(ts->font);
	int width;

	zlong color, bc;

	if ((ts->style & REVERSE_STYLE) != 0) {
		bc = ts->fore;
		color = ts->back;
	} else {
		color = ts->fore;
		bc = ts->back;
	}

	/* compute size and position of background rect */

	if (weff < w)
		weff = w;
	width = weff - ts->oh;
	if ((width > 0) && (ts->backTransparent == 0))
		sf_fillrect(bc, x + ts->oh, y, width, height);

	x += dxpre;
	y += dypre;

	for (i = 0; i < h; i++) {
		int xx = 0;
		if (ts->font->antialiased) {
			for (m = 0; m < byw; m++) {
				int t = *bmp++;
				if (xx < byw) {
					if (t) {
						zlong sval = color;
						if (t < 255)
							sval = sf_blend((int) (t + (t >> 7)), sval, sf_rpixel(x + xx, y));
						sf_wpixel(x + xx, y, sval);
					}
				}
				xx++;
			}
		} else {
			for (m = 0; m < nby; m++) {
				int t = *bmp++;
				for (j = 0; j < 8; j++, t *= 2) {
					if (xx < byw) {
						if (t & 0x80)
							sf_wpixel(x + xx, y, color);
					}
					xx++;
				}
			}
		}
		y++;
	}

	ts->cx += (w);
	ts->oh = (weff > w) ? weff - w : 0;
} /* sf_writeglyph */


void sf_fillrect(zlong color, int x, int y, int w, int h)
{
	zlong *dst;
	int i;
	if (x < xmin) {
		w += x - xmin;
		x = xmin;
	}
	if (x + w > xmax)
		w = xmax - x;
	if (w <= 0)
		return;
	if (y < ymin) {
		h += y - ymin;
		y = ymin;
	}
	if (y + h > ymax)
		h = ymax - y;
	if (h <= 0)
		return;
	dst = sbuffer + x + sbpitch * y;
	while (h--) {
		for (i = 0; i < w; i++)
			dst[i] = color;
		dst += sbpitch;
	}
	dirty = 1;
} /* sf_fillrect */


void sf_rect(zlong color, int x, int y, int w, int h)
{
	sf_chline(x, y, color, w);
	sf_chline(x, y + h - 1, color, w);
	sf_cvline(x, y, color, h);
	sf_cvline(x + w - 1, y, color, h);
} /* sf_rect */


void sf_flushtext(void)
{
	SF_textsetting *ts = sf_curtextsetting();
	ts->cx += ts->oh;
	ts->oh = 0;
} /* sf_flushtext */


/*
 * os_erase_area
 *
 * Fill a rectangular area of the screen with the current background
 * colour. Top left coordinates are (1,1). The cursor does not move.
 *
 * The final argument gives the window being changed, -1 if only a
 * portion of a window is being erased, or -2 if the whole screen is
 * being erased.
 *
 */
void os_erase_area(int top, int left, int bottom, int right, int UNUSED (win))
{
	sf_flushtext();
	sf_fillrect((sf_curtextsetting())->back, left - 1, top - 1,
		    right - left + 1, bottom - top + 1);
} /* os_erase_area */


/*
 * os_peek_colour
 *
 * Return the colour of the screen unit below the cursor. (If the
 * interface uses a text mode, it may return the background colour
 * of the character at the cursor position instead.) This is used
 * when text is printed on top of pictures. Note that this coulor
 * need not be in the standard set of Z-machine colours. To handle
 * this situation, Frotz entends the colour scheme: Colours above
 * 15 (and below 256) may be used by the interface to refer to non
 * standard colours. Of course, os_set_colour must be able to deal
 * with these colours.
 *
 */
int os_peek_colour(void)
{
	SF_textsetting *ts = sf_curtextsetting();
	sf_flushtext();
	return sf_GetColourIndex(sf_rpixel(ts->cx, ts->cy));
} /* os_peek_colour */


static void scroll(int x, int y, int w, int h, int n)
{
	zlong *src, *dst;
	int nmove, step;
	if (n > 0) {
		dst = sbuffer + x + sbpitch * y;
		src = dst + n * sbpitch;
		nmove = h - n;
		step = sbpitch;
	} else if (n < 0) {
		n = -n;
		nmove = h - n;
		step = -sbpitch;
		src = sbuffer + x + sbpitch * (y + nmove - 1);
		dst = src + n * sbpitch;
	} else
		return;
	if (nmove > 0) {
		while (nmove--) {
			memmove(dst, src, w * sizeof(zlong));
			dst += step;
			src += step;
		}
		dirty = 1;
	}
} /* scroll */


/**
 * Update the display if contents have changed.
 * Return whether contents had changed, i.e., display was updated.
 */
bool sf_flushdisplay(void)
{
	if (dirty) {
		SDL_UpdateTexture(texture, NULL, sbuffer,
				  sbpitch * sizeof(zlong));
		myGrefresh();
		dirty = 0;
		return true;
	} else
		return false;
} /* sf_flushdisplay */


/*
 * os_scroll_area
 *
 * Scroll a rectangular area of the screen up (units > 0) or down
 * (units < 0) and fill the empty space with the current background
 * colour. Top left coordinates are (1,1). The cursor stays put.
 *
 */
void os_scroll_area(int top, int left, int bottom, int right, int units)
{
	sf_flushtext();

	scroll(left - 1, top - 1, right - left + 1, bottom - top + 1, units);
	if (units > 0)
		sf_fillrect((sf_curtextsetting())->back, left - 1,
			    bottom - units, right - left + 1, units);
	else if (units < 0)
		sf_fillrect((sf_curtextsetting())->back, left - 1, top - 1,
			    right - left + 1, units);
} /* os_scroll_area */


bool os_repaint_window(int UNUSED (win), int UNUSED (ypos_old),
		int UNUSED (ypos_new), int UNUSED (xpos),
		int UNUSED (ysize), int UNUSED (xsize))
{
	/* TODO */
	return FALSE;
} /* os_repaint_window */


int SFdticks = 200;
static SDL_atomic_t SFticked = {0};
static SDL_TimerID timerid = 0;
static Uint32 refreshEventType = 0;

static Uint32 SDLCALL mytimer(Uint32 inter, void * UNUSED (parm))
{
	SDL_AtomicSet(&SFticked, 1);
	SDL_Event event = {0};
	event.type = SDL_USEREVENT;
	event.user.type = refreshEventType;
	SDL_PushEvent(&event);
	return inter;
} /* mytimer */


static void cleanvideo(void)
{
	if (timerid)
		SDL_RemoveTimer(timerid);
	SDL_Quit();
} /* cleanvideo */

#define RM 0x0000ff
#define GM 0x00ff00
#define BM 0xff0000

static void sf_toggle_fullscreen(void)
{
	if (SDL_SetWindowFullscreen
	    (window, isfullscreen ? 0 : SDL_WINDOW_FULLSCREEN_DESKTOP))
		os_warn("Error switching %s fullscreen: %s",
			isfullscreen ? "off" : "to", SDL_GetError());
	else {
		isfullscreen = !isfullscreen;
		if (!isfullscreen)
			SDL_SetWindowSize(window, AcWidth, AcHeight);
		myGrefresh();
	}
} /* sf_toggle_fullscreen */


void sf_initvideo(int W, int H, int full)
{
	Uint32 video_flags = SDL_WINDOW_RESIZABLE, pixfmt;
	Uint32 initflags = SDL_INIT_VIDEO | SDL_INIT_TIMER | SDL_INIT_AUDIO;

	sprintf(banner, "SDL Frotz v%s - %s (z%d)",
		VERSION, f_setup.story_name, z_header.version);

	if (SDL_Init(initflags) < 0) {
		os_fatal("Couldn't initialize SDL: %s", SDL_GetError());
	}

	sdl_active = TRUE;

	/* We don't handle text edit events.  Not that I know why anyone would
	   want to use such an IME with Frotz. */
	SDL_SetHint(SDL_HINT_IME_INTERNAL_EDITING, "1");

	CLEANREG(cleanvideo);

	isfullscreen = full;
	if (full)
		video_flags |= SDL_WINDOW_FULLSCREEN_DESKTOP;
	if ((window =
	     SDL_CreateWindow(banner, SDL_WINDOWPOS_UNDEFINED,
			      SDL_WINDOWPOS_UNDEFINED, W, H, video_flags)))
		renderer = SDL_CreateRenderer(window, -1, 0);
	else
		renderer = NULL;
	if (renderer == NULL) {
		os_fatal("Couldn't create %dx%d window: %s",
			 W, H, SDL_GetError());
	}
	SDL_SetHint(SDL_HINT_RENDER_SCALE_QUALITY, "linear");
	if (SDL_RenderSetLogicalSize(renderer, W, H))
		os_fatal("Failed to set logical rendering size to %dx%d: %s",
			 W, H, SDL_GetError());
	pixfmt = SDL_MasksToPixelFormatEnum(32, RM, GM, BM, 0);
	if (!(texture = SDL_CreateTexture(renderer, pixfmt,
					  SDL_TEXTUREACCESS_STREAMING, W, H)))
		os_fatal("Failed to create texture: %s", SDL_GetError());

	sbuffer = calloc(W * H, sizeof(zlong));
	if (!sbuffer)
		os_fatal("Could not create gc");

	refreshEventType = SDL_RegisterEvents(1);
	timerid = SDL_AddTimer(SFdticks, mytimer, NULL);

	xmin = ymin = 0;
	xmax = ewidth = W;
	ymax = eheight = H;
	sbpitch = W;
	dirty = 1;
} /* sf_initvideo */


/*
 * os_draw_picture
 *
 * Display a picture at the given coordinates.
 *
 */
void os_draw_picture(int picture, int y, int x)
{
	int ew, eh, xx, yy, ix, iy, d;
	int ox, oy, ow, oh;
	Zwindow *winpars;
	sf_picture *pic = sf_getpic(picture);
	zlong *src, *dst, sval, dval, alpha;

	sf_flushtext();

	if (!pic)
		return;
	if (!pic->pixels)
		return;
	src = (zlong *) pic->pixels;

	x--;
	y--;
	ew = ceil(m_gfxScale_w * pic->width);
	eh = ceil(m_gfxScale_h * pic->height);

	/* this takes care of the fact that x, y are really 16 bit values */
	if (x & 0x8000)
		x |= 0xffff0000;
	if (y & 0x8000)
		y |= 0xffff0000;

	/* get current window rect */
	sf_getclip(&ox, &oy, &ow, &oh);
	winpars = curwinrec();
	sf_setclip(winpars->x_pos - 1, winpars->y_pos - 1, winpars->x_size,
		   winpars->y_size);

	/* clip taking into account possible origin
	 * outside the clipping rect */
	if (x < xmin) {
		d = xmin - x;
		ew -= d;
		x = xmin;
		src += d;
	}
	if (x + ew > xmax)
		ew = xmax - x;
	ew /= m_gfxScale_w;

	if (y < ymin) {
		d = ymin - y;
		eh -= d;
		y = ymin;
		src += d * pic->width;
	}
	if (y + eh > ymax)
		eh = ymax - y;
	eh /= m_gfxScale_h;

	sf_setclip(ox, oy, ow, oh);

	if (ew <= 0)
		return;
	if (eh <= 0)
		return;

	/* Use simple scaling without interpolation in palette mode. */
	/* Adapted from start of FrotzGfx::Paint() in Windows Frotz. */
	if (pic->usespalette) {
		if (!pic->adaptive && ApplyPalette(pic))
			sf_flushdisplay();

		for (yy = 0; yy < eh * m_gfxScale_h; yy++) {
			int ys = ceil(yy / m_gfxScale_h);
			if (ys >= pic->height)
			    ys = pic->height - 1;
			for (xx = 0; xx < ew * m_gfxScale_w; xx++) {
				int xs = ceil(xx / m_gfxScale_w);
				if (xs >= pic->width)
				    xs = pic->width - 1;
				int index = pic->pixels[ys * pic->width + xs];
				if (index != pic->transparentcolor)
					sf_wpixel(x + xx, y + yy,
						  screen_palette[index]);
			}
		}
	} else {
		if (pic->adaptive)
			os_fatal("Adaptive images must be paletted");

		for (yy = 0; yy < eh; yy++) {
			for (xx = 0; xx < ew; xx++) {
				dst =
				    sbuffer + x + (zlong)(xx * m_gfxScale_w) +
				    sbpitch * (y + (zlong)(yy * m_gfxScale_h));
				sval = src[xx];
				alpha = (sval >> 24);
				if (alpha == 255)
					dval = sval & 0xffffff;
				else
					dval =
					    sf_blend((int)
						     (alpha + (alpha >> 7)),
						     sval, dst[0]);
				for (iy = 0; iy < m_gfxScale_h; iy++) {
					for (ix = 0; ix < m_gfxScale_w; ix++)
						dst[ix] = dval;
					dst += sbpitch;
				}
			}

			src += pic->width;
		}
	}

	dirty = 1;
} /* os_draw_picture */


static zlong mytimeout;
int mouse_button;
static int numAltQ = 0;

static void set_mouse_xy(int x, int y)
{
	/* This is enough even in fullscreen:
	 * SDL maps mouse events to logical coordinates. */
	mouse_x = x + 1;
	mouse_y = y + 1;
} /* set_mouse_xy */


/**
 * Return the first character in the UTF-8-encoded str.
 * Return 0 on encoding error or if the first character > U+FFFF.
 */
static zword decode_utf8(char *str)
{
	int i, n = 0;
	zword res;
	if (!(*str & 0200))
		return *str;
	if (!(*str & 0100))
		return 0;
	if (!(*str & 040)) {
		n = 2;
		res = *str & 037;
	} else if (!(*str & 020)) {
		n = 3;
		res = *str & 017;
	} else
		return 0;
	for (i = 1; i < n; ++i) {
		if ((str[i] & 0300) != 0200)
			return 0;
		res <<= 6;
		res |= (str[i] & 077);
	}
	return res;
} /* decode_utf8 */


static void handle_window_event(SDL_Event * e)
{
	switch (e->window.event) {
	case SDL_WINDOWEVENT_EXPOSED:
		myGrefresh();
	}
} /* handle_window_event */


static zword goodzkey(SDL_Event * e, int allowed)
{
	SDL_Keycode c;
	zword res;

	switch (e->type) {
	case SDL_QUIT:
		sf_quitconf();
		return 0;
	case SDL_MOUSEBUTTONDOWN:
		if (true) {
			mouse_button = e->button.button;
			set_mouse_xy(e->button.x, e->button.y);
			return (e->button.clicks == 1 ? ZC_SINGLE_CLICK : ZC_DOUBLE_CLICK);
		} else
			return 0;
	case SDL_KEYDOWN:
		if ((e->key.keysym.mod & 0xfff) == (KMOD_LALT | KMOD_LCTRL)
		    && e->key.keysym.sym == 'x')
			os_fatal("Emergency exit!\n\n(Control-Alt-X pressed)");
		if (e->key.keysym.mod & (KMOD_LALT | KMOD_RALT)) {
			if (e->key.keysym.sym == 'q') {
				numAltQ++;
				if (numAltQ > 2)
					os_fatal("Emergency exit!\n\n"
						 "(Alt-Q pressed 3 times in succession)");
				return 0;
			} else
				numAltQ = 0;
			if (e->key.keysym.sym == SDLK_RETURN) {
				sf_toggle_fullscreen();
				return 0;
			}
			if (allowed)
				switch (e->key.keysym.sym) {
				case 'x':
					return ZC_HKEY_QUIT;
				case 'p':
					return ZC_HKEY_PLAYBACK;
				case 'r':
					return ZC_HKEY_RECORD;
				case 's':
					return ZC_HKEY_SEED;
				case 'u':
					return ZC_HKEY_UNDO;
				case 'n':
					return ZC_HKEY_RESTART;
				case 'd':
					return ZC_HKEY_DEBUG;
				case 'h':
					return ZC_HKEY_HELP;
				}
			return 0;
		} else
			numAltQ = 0;
		switch (e->key.keysym.sym) {
		case SDLK_INSERT:
			return (allowed ? VK_INS : 0);
		case SDLK_DELETE:
			return (allowed ? VK_DEL : 0);
		case SDLK_BACKSPACE:
			return ZC_BACKSPACE;
		case SDLK_ESCAPE:
			return ZC_ESCAPE;
		case SDLK_RETURN:
			return ZC_RETURN;
		case SDLK_UP:
			return ZC_ARROW_UP;
		case SDLK_DOWN:
			return ZC_ARROW_DOWN;
		case SDLK_LEFT:
			return ZC_ARROW_LEFT;
		case SDLK_RIGHT:
			return ZC_ARROW_RIGHT;
		case SDLK_TAB:
			return (allowed ? VK_TAB : 0);
		case SDLK_PAGEUP:
			return (allowed ? VK_PAGE_UP : 0);
		case SDLK_PAGEDOWN:
			return (allowed ? VK_PAGE_DOWN : 0);
		case SDLK_KP_0:
			return ZC_NUMPAD_0;
		case SDLK_KP_1:
			return ZC_NUMPAD_1;
		case SDLK_KP_2:
			return ZC_NUMPAD_2;
		case SDLK_KP_3:
			return ZC_NUMPAD_3;
		case SDLK_KP_4:
			return ZC_NUMPAD_4;
		case SDLK_KP_5:
			return ZC_NUMPAD_5;
		case SDLK_KP_6:
			return ZC_NUMPAD_6;
		case SDLK_KP_7:
			return ZC_NUMPAD_7;
		case SDLK_KP_8:
			return ZC_NUMPAD_8;
		case SDLK_KP_9:
			return ZC_NUMPAD_9;
		case SDLK_F1:
			return ZC_FKEY_F1;
		case SDLK_F2:
			return ZC_FKEY_F2;
		case SDLK_F3:
			return ZC_FKEY_F3;
		case SDLK_F4:
			return ZC_FKEY_F4;
		case SDLK_F5:
			return ZC_FKEY_F5;
		case SDLK_F6:
			return ZC_FKEY_F6;
		case SDLK_F7:
			return ZC_FKEY_F7;
		case SDLK_F8:
			return ZC_FKEY_F8;
		case SDLK_F9:
			return ZC_FKEY_F9;
		case SDLK_F10:
			return ZC_FKEY_F10;
		case SDLK_F11:
			return ZC_FKEY_F11;
		case SDLK_F12:
			return ZC_FKEY_F12;
		}
		/* XXX Maybe we should just always have text input on. */
		if (!SDL_IsTextInputActive()) {
			c = e->key.keysym.sym;
			if (c >= 32 && c <= 126)
				return c;
		}
		return 0;
	case SDL_TEXTINPUT:
		res = decode_utf8(e->text.text);
		if ((res >= 32 && res <= 126) || res >= 160)
			return res;
		else
			return 0;
	case SDL_USEREVENT:
		if (e->user.type == refreshEventType) {
			myGrefresh();
		}
		return 0;
	case SDL_WINDOWEVENT:
		handle_window_event(e);
	}
	return 0;
} /* goodzkey */


zword sf_read_key(int timeout, bool cursor, bool allowed, bool text)
{
	SDL_Event event;
	zword inch = 0;

	sf_flushtext();
	if (cursor)
		sf_drawcursor(true);
	sf_flushdisplay();

	if (timeout)
		mytimeout = sf_ticks() + m_timerinterval * timeout;
	if (text)
		SDL_StartTextInput();
	while (true) {
		/* Get the next input */
		while (SDL_PollEvent(&event)) {
			if ((inch = goodzkey(&event, allowed)))
				break;
		}
		if (inch)
			break;
		if ((timeout) && (sf_ticks() >= mytimeout)) {
			inch = ZC_TIME_OUT;
			break;
		}
		sf_checksound();
		sf_sleep(10);
	}
	if (text)
		SDL_StopTextInput();

	if (cursor)
		sf_drawcursor(false);

	return inch;
} /* sf_read_key */


/*
 * os_read_key
 *
 * Read a single character from the keyboard (or a mouse click) and
 * return it. Input aborts after timeout/10 seconds.
 *
 */
zchar os_read_key(int timeout, bool cursor)
{
	return sf_read_key(timeout, cursor, false, true);
} /* os_read_key */


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
 *     ZC_HKEY_PLAYBACK (Alt-P)
 *     ZC_HKEY_RECORD (Alt-R)
 *     ZC_HKEY_SEED (Alt-S)
 *     ZC_HKEY_UNDO (Alt-U)
 *     ZC_HKEY_RESTART (Alt-N, "new game")
 *     ZC_HKEY_QUIT (Alt-X, "exit game")
 *     ZC_HKEY_DEBUG (Alt-D)
 *     ZC_HKEY_HELP (Alt-H)
 *
 * If the timeout argument is not zero, the input gets interrupted
 * after timeout/10 seconds (and the return value is 0).
 *
 * The complete input line including the cursor must fit in "width"
 * screen units.
 *
 * The function may be called once again to continue after timeouts,
 * misplaced mouse clicks or hot keys. In this case the "continued"
 * flag will be set. This information can be useful if the interface
 * implements input line history.
 *
 * The screen is not scrolled after the return key was pressed. The
 * cursor is at the end of the input line when the function returns.
 *
 * Since Frotz 2.2 the helper function "completion" can be called
 * to implement word completion (similar to tcsh under Unix).
 *
 */
zchar os_read_line(int max, zchar * buf, int timeout, int width, int continued)
{
	static int pos = 0, searchpos = -1;
	int ptx, pty;
	int len = mywcslen(buf);
	SF_textsetting *ts = sf_curtextsetting();
	SDL_Event event;
	sf_flushtext();

	/* Better be careful here or it might segv.  I wonder if we should just
	   ignore 'continued' and check for len > 0 instead?  Might work better
	   with Beyond Zork. */
	if (!continued || pos > len || searchpos > len) {
		pos = len;
		gen_history_reset();	/* Reset user's history view. */
		searchpos = -1;	/* -1 means initialize from len. */
	}
	/* Draw the input line */
	ptx = ts->cx;
	pty = ts->cy;
	ptx -= os_string_width(buf);
	sf_DrawInput(buf, pos, ptx, pty, width, true);

	if (timeout)
		mytimeout = sf_ticks() + m_timerinterval * timeout;
	SDL_StartTextInput();
	while (true) {
		/* Get the next input */
		while (SDL_PollEvent(&event)) {
			zword c;
			if ((c = goodzkey(&event, 1))) {
				switch (c) {
				case ZC_BACKSPACE:
					/* Delete the character to the left of the cursor */
					if (pos > 0) {
						memmove(buf + pos - 1,
							buf + pos,
							sizeof(zchar) *
							(mywcslen(buf) - pos +
							 1));
						pos--;
						sf_DrawInput(buf, pos, ptx, pty,
							     width, true);
					}
					continue;
				case VK_DEL:
					/* Delete the character to the right of the cursor */
					if (pos < mywcslen(buf)) {
						memmove(buf + pos,
							buf + pos + 1,
							sizeof(zchar) *
							(mywcslen(buf) - pos));
						sf_DrawInput(buf, pos, ptx, pty,
							     width, true);
					}
					continue;
				case ZC_ESCAPE:	/* Delete whole line */
					pos = 0;
					buf[0] = '\0';
					searchpos = -1;
					gen_history_reset();
					sf_DrawInput(buf, pos, ptx, pty, width,
						     true);
					continue;
				case VK_TAB:
					if (pos == (int)mywcslen(buf)) {
						zchar extension[10], *s;
						completion(buf, extension);

						/* Add the completion to the input stream */
						for (s = extension; *s != 0;
						     s++)
							if (sf_IsValidChar(*s))
								buf[pos++] =
								    (*s);
						buf[pos] = 0;
						sf_DrawInput(buf, pos, ptx, pty,
							     width, true);
					}
					continue;
				case ZC_ARROW_LEFT:
					/* Move the cursor left */
					if (pos > 0)
						pos--;
					sf_DrawInput(buf, pos, ptx, pty, width,
						     true);
					continue;
				case ZC_ARROW_RIGHT:
					/* Move the cursor right */
					if (pos < (int)mywcslen(buf))
						pos++;
					sf_DrawInput(buf, pos, ptx, pty, width,
						     true);
					continue;
				case ZC_ARROW_UP:
				case ZC_ARROW_DOWN:
					if (searchpos < 0)
						searchpos = mywcslen(buf);
					if ((c == ZC_ARROW_UP
					     ? gen_history_back :
					     gen_history_forward) (buf,
								   searchpos,
								   max)) {
						pos = mywcslen(buf);
						sf_DrawInput(buf, pos, ptx, pty,
							     width, true);
					}
					continue;
					/* Pass through as up/down arrows for Beyond Zork. */
				case VK_PAGE_UP:
					c = ZC_ARROW_UP;
					break;
				case VK_PAGE_DOWN:
					c = ZC_ARROW_DOWN;
					break;
				default:
					if (sf_IsValidChar(c)
					    && mywcslen(buf) < max) {
						/* Add a valid character to the input line
						 * Get the width of the new input line */
						int len = os_string_width(buf);
						len += os_char_width(c);
						len += os_char_width('0');

						/* printf("l%d w%d p%d\n",len,width,pos);
						 * Only allow if the width limit is not exceeded */
						if (len <= width) {
							memmove(buf + pos + 1,
								buf + pos,
								sizeof(zchar) *
								(mywcslen(buf) -
								 pos + 1));
							*(buf + pos) = c;
							pos++;
							sf_DrawInput(buf, pos,
								     ptx, pty,
								     width,
								     true);
						}
						continue;
					}
				}
				if (is_terminator(c)) {
					/* Terminate the current input */
					sf_DrawInput(buf, pos, ptx, pty, width,
						     false);

					if ((c == ZC_SINGLE_CLICK)
					    || (c == ZC_DOUBLE_CLICK)) {
						/*  mouse_x = input.mousex+1;
						   mouse_y = input.mousey+1; */
					} else if (c == ZC_RETURN)
						gen_add_to_history(buf);
					SDL_StopTextInput();
					return c;
				}
			}
		}
		if ((timeout) && (sf_ticks() >= mytimeout)) {
			SDL_StopTextInput();
			return ZC_TIME_OUT;
		}
		sf_checksound();
		sf_sleep(10);
	}
} /* os_read_line */

/* Draw the current input line */
void sf_DrawInput(zchar * buffer, int pos, int ptx, int pty, int width,
		  bool cursor)
{
	int height;
	SF_textsetting *ts = sf_curtextsetting();


	height = ts->font->height(ts->font);

	/* Remove any previous input */
	sf_fillrect(ts->back, ptx, pty, width, height);

	/* Display the input */
	ts->cx = ptx;
	ts->cy = pty;
	os_display_string(buffer);

	if (cursor) {
		int wid = 0, i = 0, oh;
		while (i < pos)
			wid += sf_charwidth(buffer[i++], &oh);
		drawthecursor(ptx + wid, pty, 1);
	}

	/* Update the window */
	sf_flushdisplay();
} /* sf_DrawInput */


/*
 * os_read_mouse
 *
 * Store the mouse position in the global variables "mouse_x" and
 * "mouse_y", the code of the last clicked menu in "menu_selected"
 * and return the mouse buttons currently pressed.
 *
 */
zword os_read_mouse(void)
{
	zbyte c;
	int x, y;
	zword btn = 0;
	/* Get the mouse position */
	SDL_PumpEvents();
	c = SDL_GetMouseState(&x, &y);
	set_mouse_xy(x, y);
	/* Get the last selected menu item */
	/* menu_selected = theWnd->GetMenuClick();*/

	/* Get the mouse buttons */
	if (c & SDL_BUTTON_LMASK)
		btn |= 1;
	if (c & SDL_BUTTON_RMASK)
		btn |= 2;
	if (c & SDL_BUTTON_MMASK)
		btn |= 4;

	return btn;
} /* os_read_mouse */


/*
 * os_more_prompt
 *
 * Display a MORE prompt, wait for a keypress and remove the MORE
 * prompt from the screen.
 *
 */
void os_more_prompt(void)
{
	if (m_morePrompts) {
		SF_textsetting *ts;
		int x, y, h;
		const char *p = sf_msgstring(IDS_MORE);
		sf_flushtext();

		/* Save the current text position */
		sf_pushtextsettings();
		ts = sf_curtextsetting();
		x = ts->cx;
		y = ts->cy;
		h = ts->font->height(ts->font);
		/* Show a [More] prompt */
		while (*p)
			os_display_char((zchar) (*p++));

		/* Wait for a key press */
		sf_read_key(0, true, false, false);
		/* Remove the [More] prompt */
		sf_fillrect(ts->back, x, y, ts->cx - x, h);

		/* Restore the current text position */
		sf_poptextsettings();
	}
} /* os_more_prompt */


zlong *sf_savearea(int x, int y, int w, int h)
{
	zlong *r, *p, *s;
	int i;

	if (x < 0) {
		w += x;
		x = 0;
	}
	if (x + w > ewidth)
		w = ewidth - x;
	if (w <= 0)
		return NULL;

	if (y < 0) {
		h += y;
		y = 0;
	}
	if (y + h > eheight)
		h = eheight - y;
	if (h <= 0)
		return NULL;

	r = p = malloc((w * h + 4) * sizeof(zlong));
	if (!r)
		return NULL;

	*p++ = x;
	*p++ = y;
	*p++ = w;
	*p++ = h;

	s = sbuffer + x + y * sbpitch;
	for (i = 0; i < h; i++) {
		memmove(p, s, w * sizeof(zlong));
		p += w;
		s += sbpitch;
	}
	return r;
} /* sf_savearea */


void sf_restoreareaandfree(zlong * s)
{
	zlong *p, *d;
	int i, x, y, w, h;
	if (!s)
		return;

	p = s;
	x = *p++;
	y = *p++;
	w = *p++;
	h = *p++;

	d = sbuffer + x + y * sbpitch;
	for (i = 0; i < h; i++) {
		memmove(d, p, w * sizeof(zlong));
		p += w;
		d += sbpitch;
	}

	free(s);
	dirty = 1;
	sf_flushdisplay();
} /* sf_restoreareaandfree */


int (*sf_osdialog)(bool ex, const char *def, const char *filt, const char *tit,
		   char **res, zlong * sbuf, int sbp, int ew, int eh,
		   int isfull) = NULL;


int sf_user_fdialog(bool existing, const char *defaultname, const char *filter,
		    const char *title, char **result)
{
	if (sf_osdialog)
		return sf_osdialog(existing, defaultname, filter, title, result,
				   sbuffer, sbpitch, ewidth, eheight,
				   isfullscreen);
	return SF_NOTIMP;
} /* sf_user_fdialog */


void sf_videodata(zlong ** sb, int *sp, int *ew, int *eh)
{
	*sb = sbuffer;
	*sp = sbpitch;
	*ew = ewidth;
	*eh = eheight;
} /* sf_videodata */


extern zword sf_yesnooverlay(int xc, int yc, char *t, int sr);
static void sf_quitconf(void)
{
	if (sf_yesnooverlay(ewidth / 2, eheight / 2, "Quit: are you sure?", 1)
	    == ZC_RETURN) {
		printf
		    ("\n\nQuitting (close button clicked on main window)\n\n");
		SDL_Quit();
		os_quit(EXIT_SUCCESS);
	}
} /* sf_quitconf */


void os_tick(void)
{
	sf_checksound();
	if (SDL_AtomicSet(&SFticked, 0)) {
		if (!sf_flushdisplay()) {
			SDL_Event ev;
			SDL_PumpEvents();
			while (SDL_PeepEvents(&ev, 1, SDL_GETEVENT,
					      SDL_WINDOWEVENT,
					      SDL_WINDOWEVENT) > 0)
				handle_window_event(&ev);
		}
	}
} /* os_tick */


/* Apply the picture's palette to the screen palette. */
/* Adapted from FrotzGfx::ApplyPalette() in Windows Frotz. */
static bool ApplyPalette(sf_picture * graphic)
{
	bool changed = FALSE;
	int i, colors;

	memset(&screen_palette, 0, sizeof(zlong));

	if (graphic->usespalette) {
		colors = graphic->palette_entries;
		if (colors > 16)
			colors = 16;
		for (i = 0; i < colors; i++) {
			if (screen_palette[i] != graphic->palette[i]) {
				changed = TRUE;
				screen_palette[i] = graphic->palette[i];
			}
		}
	}
	return changed;
} /* ApplyPalette */
