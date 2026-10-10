/*
 * sf_ftype.c - SDL interface, FreeType font handling
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

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <math.h>

#include <ft2build.h>
#include FT_FREETYPE_H
#include <freetype/freetype.h>

#include "sf_font3ttf.h"

/***************************************************************/

typedef struct {
	SFONT sfont;
	int ascent, descent, height;
	int minchar, maxchar, totglyphs;
	FT_Face face;
	char *isset;
	SF_glyph *glyphs[0];
} MYFONT;


/* destructor */
static void bdestroy(SFONT * s)
{
	if (s) {
		int i;
		MYFONT *f = (MYFONT *) s;
		for (i = 0; i < f->totglyphs; i++)
			if (f->glyphs[i])
				free(f->glyphs[i]);
		free(f->isset);
		if (f->face)
			FT_Done_Face(f->face);
		free(s);
	}
} /* bdestroy */


static int bheight(SFONT * s)
{
	if (s)
		return ((MYFONT *) s)->height;
	return 0;
} /* bheight */


static int bascent(SFONT * s)
{
	if (s)
		return ((MYFONT *) s)->ascent;
	return 0;
} /* bascent */


static int bdescent(SFONT * s)
{
	if (s)
		return ((MYFONT *) s)->descent;
	return 0;
} /* bdescent */


static int bminchar(SFONT * s)
{
	if (s)
		return ((MYFONT *) s)->minchar;
	return 0;
} /* bminchar */


static int bmaxchar(SFONT * s)
{
	if (s)
		return ((MYFONT *) s)->maxchar;
	return 0;
} /* bmaxchar */


static void setglyph(MYFONT * f, FT_Face face, int ch);

static SF_glyph *getglyph(SFONT * s, zword c, int allowdef)
{
	if (s) {
		MYFONT *f = (MYFONT *) s;
		if (c < f->minchar || c > f->maxchar) {
			if (allowdef)
				c = 0;
			else
				return NULL;
		}
		if (!f->isset[c])
			setglyph(f, f->face, c);
		return f->glyphs[c];
	}
	return NULL;
} /* getglyph */


static int hasglyph(SFONT * fo, zword c, int allowdef)
{
	return (getglyph(fo, c, allowdef) != NULL);
} /* hasglyph */


static int inited = 0, initerr = 0;
static FT_Library library;

static void libfinish(void)
{
	if (!inited)
		return;
	FT_Done_FreeType(library);
	inited = 0;
} /* libfinish */


static void libinit(void)
{
	if (initerr)
		return;
	if (inited)
		return;
	initerr = FT_Init_FreeType(&library);	/* initialize library */
	/* error handling omitted */
	if (initerr)
		printf("FT_Init_FreeType: error %d\n", initerr);
	else {
		inited = 1;
		atexit(libfinish);
	}
} /* libinit */


static MYFONT *makefont(int totglyphs)
{
	MYFONT *res;
	res = calloc(1, sizeof(MYFONT) + totglyphs * sizeof(SF_glyph *));
	if (!res)
		return NULL;
	res->isset = calloc(1, totglyphs);
	if (!res->isset) {
		free(res);
		return NULL;
	}
	res->sfont.destroy = bdestroy;
	res->sfont.height = bheight;
	res->sfont.ascent = bascent;
	res->sfont.descent = bdescent;
	res->sfont.minchar = bminchar;
	res->sfont.maxchar = bmaxchar;
	res->sfont.hasglyph = hasglyph;
	res->sfont.getglyph = getglyph;
	res->totglyphs = totglyphs;
	res->maxchar = totglyphs - 1;
	return res;
} /* makefont */


#define MAXUNI 0xffff

static void setglyph(MYFONT * f, FT_Face face, int ch)
{
	int err, gid = FT_Get_Char_Index(face, ch);
	int mode = FT_RENDER_MODE_MONO;
	SF_glyph *res;
	FT_GlyphSlot slot = face->glyph;
	int i, j, nbypr;
	FT_Bitmap *bitmap;

	f->isset[ch] = 1;
	if (!gid)
		return;

	if (m_aafonts)
		mode = FT_RENDER_MODE_NORMAL;

	err = FT_Load_Glyph(face, gid, 0);
	if (err)
		return;
	if (slot->format != FT_GLYPH_FORMAT_BITMAP) {
		err = FT_Render_Glyph(slot, mode);
		if (err)
			return;
	}
	bitmap = &slot->bitmap;
	nbypr = m_aafonts ? bitmap->width : (bitmap->width + 7) / 8;
	res = calloc(1, sizeof(SF_glyph) + nbypr * bitmap->rows);
	if (!res)
		return;
	for (i = 0; i < bitmap->rows; i++) {
		for (j = 0; j < nbypr; j++) {
			res->bitmap[i * nbypr + j] =
			    bitmap->buffer[i * bitmap->pitch + j];
		}
	}

	res->w = bitmap->width;
	res->h = bitmap->rows;
	res->dx = slot->advance.x / 64;
	res->xof = slot->bitmap_left;
	res->yof = slot->bitmap_top - bitmap->rows + 1;

	f->glyphs[ch] = res;
} /* setglyph */


static SFONT *loadftype(char *fname, int size, SFONT * like, int *err)
{
	MYFONT *res;
	FT_Face face;
	int i;

	*err = 0;
	if (!fname) {
		*err = -8;
		return NULL;
	}
	libinit();
	if (initerr) {
		*err = -99;
		return NULL;
	}

	res = makefont(MAXUNI + 1);
	if (!res) {
		*err = -3;
		return NULL;
	}

	/* create face object */
	if (strcmp(fname, "<builtin_font3>"))
		*err = FT_New_Face(library, fname, 0, &face);
	else
		*err = FT_New_Memory_Face(library, SF_font3_ttf,
			sizeof(SF_font3_ttf), 0, &face);
	if (*err) {
		res->sfont.destroy(&res->sfont);
		return NULL;
	}
	res->face = face;

	if (like) {
		SF_glyph *zero = like->getglyph(like, '0', TRUE);
		*err = FT_Set_Pixel_Sizes(face, 0, like->height(like));

		FT_Matrix tmat;
		tmat.xy = tmat.yx = 0;
		tmat.yy = 1<<16;
		tmat.xx = (zero->dx << 16)/(face->size->metrics.max_advance >>6);
		FT_Set_Transform(face, &tmat, NULL);
	} else
		*err = FT_Set_Pixel_Sizes(face, size, size);
	if (*err) {
		res->sfont.destroy(&res->sfont);
		return NULL;
	}

	res->ascent = face->size->metrics.ascender / 64;
	res->descent = -face->size->metrics.descender / 64;
	res->height = face->size->metrics.height / 64;

	res->sfont.antialiased = m_aafonts;
	res->minchar = 32;
	setglyph(res, face, 0);
	for (i = 32; i < 127; i++)
		setglyph(res, face, i);
	for (i = 0xa0; i < 256; i++)
		setglyph(res, face, i);
	setglyph(res, face, 0x152);
	setglyph(res, face, 0x153);

	return (SFONT *) res;
} /* loadftype */


#ifdef WIN32
#define SYSFONTS "c:/windows/fonts"
#else
#define SYSFONTS "/usr/share/fonts/freetype"
#endif

SFONT *sf_loadftype(char *fspec, SFONT * like, int *err)
{
	char buf[FILENAME_MAX], *fn, *at, *fenv;
	int size = DEFSIZE, fnlen = -1;

	if (!strcmp(fspec, "<builtin_font3>"))
		return loadftype(fspec, size, like, err);

	at = strchr(fspec, '@');
	if (at) {
		fnlen = at - fspec;
		size = atoi(at + 1);
	}

	fn = sf_searchfile(fspec, fnlen, buf, "");
	if (!fn)
		fn = sf_searchfile(fspec, fnlen, buf, "./");
	if (!fn)
		if (m_fontdir)
			fn = sf_searchfile(fspec, fnlen, buf, m_fontdir);
	if (!fn)
		fn = sf_searchfile(fspec, fnlen, buf, SYSFONTS);
	if (!fn) {
		fenv = getenv("FONTS");
		if (fenv)
			fn = sf_searchfile(fspec, fnlen, buf, fenv);
	}

	if (!fn)
		return NULL;

	return loadftype(fn, size, like, err);
} /* sf_loadftype */

/****************************************/

void sf_initloader(void)
{
	ttfontloader = sf_loadftype;
} /* sf_initloader */
