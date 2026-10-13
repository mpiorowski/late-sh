/*
 * sf_frotz.h
 *
 * Declarations and definitions for the SDL interface
 *
 */

#ifndef _SF_FROTZ_H
#define _SF_FROTZ_H

#include "sf_defines.h"
#include "../common/frotz.h"
#include "../blorb/blorb.h"

#include <stdint.h>

typedef struct {
	bb_result_t bbres;
	zlong type;
	FILE *file;
} myresource;

int sf_getresource(int num, int ispic, int method, myresource * res);
void sf_freeresource(myresource * res);
bool sf_IsAdaptive(int picture);

#ifndef true
#define true 1
#endif
#ifndef false
#define false 0
#endif

#define NON_STD_COLS 238

/* Paths where z-files may be found */
#define PATH1		"ZCODE_PATH"
#define PATH2		"INFOCOM_PATH"

#define CONFIG_FILE	".sfrotzrc"

#define DEFAULT_WIDTH	640
#define DEFAULT_HEIGHT	400

#define MAX(x,y) ((x)>(y)) ? (x) : (y)
#define MIN(x,y) ((x)<(y)) ? (x) : (y

#ifdef WIN32
#define HOMEDIR "USERPROFILE"
#else
#define HOMEDIR "HOME"
#endif

/* this assumes RGBA with lsb = R */
static inline zlong RGB5ToTrue(zword w)
{
	int _r = w & 0x001F;
	int _g = (w & 0x03E0) >> 5;
	int _b = (w & 0x7C00) >> 10;
	_r = (_r << 3) | (_r >> 2);
	_g = (_g << 3) | (_g >> 2);
	_b = (_b << 3) | (_b >> 2);
	return (zlong) (_r | (_g << 8) | (_b << 16));
}

static inline zword TrueToRGB5(zlong u)
{
	return (zword) (((u >> 3) & 0x001f) | ((u >> 6) & 0x03e0) |
		       ((u >> 9) & 0x7c00));
}

void reset_memory(void);
void replay_close(void);
void set_header_extension(int entry, zword val);
int colour_in_use(zword colour);

/*  various data */
extern bool m_tandy;
extern int m_v6scale;
extern double m_gfxScale_w;
extern double m_gfxScale_h;
extern zlong m_defaultFore;
extern zlong m_defaultBack;
extern zlong m_colours[11];
extern zlong m_nonStdColours[NON_STD_COLS];
extern int m_nonStdIndex;
extern bool m_lineInput;
extern bool m_morePrompts;
extern int AcWidth;
extern int AcHeight;
extern int m_random_seed;
extern int m_fullscreen;
extern char *m_fontfiles[9];
extern bool m_localfiles;
extern int m_no_sound;
extern int m_vga_fonts;
extern int SFdticks;
extern char *m_fontdir;
extern bool m_aafonts;
extern char *m_setupfile;
extern int m_frequency;

extern double m_gamma;
extern double m_xscale;
extern double m_yscale;

extern bool sdl_active;

/* sf_resource.c */

/* must be called as soon as possible (i.e. by os_process_arguments()) */
int sf_load_resources(void);

typedef struct {
	int number;	/* 0 means unallocated */
	int width, height;
	zbyte *pixels;
	zlong palette[16];
	int palette_entries;
	int transparentcolor;
	bool adaptive;
	bool usespalette;
} sf_picture;

#define DEFAULT_GAMMA 2.2

void sf_setgamma(double gamma);


/* get pointer from cache */
sf_picture *sf_getpic(int num);

void sf_flushtext(void);

/* glyph */
typedef struct {
	zbyte dx;
	zbyte w, h;
	signed char xof, yof;
	zbyte bitmap[0];
} SF_glyph;

typedef struct sfontstruct SFONT;

extern SFONT *(*ttfontloader)(char *fspec, SFONT * like, int *err);
extern void (*ttfontsdone)();

struct sfontstruct {
	int refcount;
	void (*destroy)(SFONT *);
	int (*height)(SFONT *);
	int (*ascent)(SFONT *);
	int (*descent)(SFONT *);
	int (*minchar)(SFONT *);
	int (*maxchar)(SFONT *);
	int (*hasglyph)(SFONT *, zword, int);
	SF_glyph *(*getglyph)(SFONT *, zword, int);
	int antialiased;
	void *data;
};

typedef struct {
	SFONT *font;
	int proportional;
	int style, zfontnum;
	int cx, cy;		/* cursor position - 0 based */
	int oh;			/* overhang */
	zlong fore, back;
	bool foreDefault, backDefault, backTransparent;
} SF_textsetting;

SF_textsetting *sf_curtextsetting(void);

/**
 * Return the advance width of character c.
 * Store glyph width minus advance width to oh ("overhang").
 */
int sf_charwidth(zword c, int *oh);

void sf_writeglyph(SF_glyph * g);

void sf_fillrect(zlong color, int x, int y, int w, int h);

int sf_GetProfileInt(const char *sect, const char *id, int def);
double sf_GetProfileDouble(const char *sect, const char *id, double def);
char *sf_GetProfileString(const char *sect, const char *id, char *def);

void sf_readsettings(void);

zlong sf_GetColour(int colour);
zlong sf_GetDefaultColour(bool fore);
int sf_GetColourIndex(zlong colour);

void sf_initvideo(int w, int h, int full);

int sf_initsound(void);

void sf_initfonts(void);

void sf_setdialog(void);
void sf_initloader(void);

void sf_cleanup_all(void);
void sf_regcleanfunc(void *f, const char *nam);
#define CLEANREG( f) sf_regcleanfunc( (void *)f, #f)

const char *sf_msgstring(int id);

/* consts for msg ids */
enum { IDS_BLORB_GLULX, IDS_BLORB_NOEXEC, IDS_MORE, IDS_HIT_KEY_EXIT, IDS_TITLE,
	IDS_FATAL, IDS_FROTZ, IDS_FAIL_DIRECTSOUND, IDS_FAIL_MODPLUG,
	    IDS_ABOUT_INFO,
	IDS_SAVE_FILTER, IDS_SAVE_TITLE, IDS_RESTORE_TITLE,
	IDS_SCRIPT_FILTER, IDS_SCRIPT_TITLE,
	IDS_RECORD_FILTER, IDS_RECORD_TITLE, IDS_PLAYBACK_TITLE,
	IDS_AUX_FILTER, IDS_SAVE_AUX_TITLE, IDS_LOAD_AUX_TITLE
};

bool sf_IsInfocomV6(void);

zlong sf_blend(int a, zlong s, zlong d);

void sf_sleep(int millisecs);

zlong sf_ticks(void);

void sf_DrawInput(zchar * buffer, int pos, int ptx, int pty, int width,
		  bool cursor);

int sf_aiffwav(FILE * f, int foffs, void **wav, int *size);

int sf_pkread(FILE * f, int foffs, void **out, int *size);

zlong *sf_savearea(int x, int y, int w, int h);
void sf_restoreareaandfree(zlong * s);
#define SF_NOTIMP (-9999)

zword sf_read_key(int timeout, bool cursor, bool allowed, bool text);

int sf_user_fdialog(bool exist, const char *def, const char *filt,
		    const char *title, char **res);
extern int (*sf_osdialog)(bool ex, const char *def, const char *filt,
			  const char *tit, char **res, zlong * sbuf, int sbp,
			  int ew, int eh, int isfull);

void sf_checksound(void);

void sf_installhandlers(void);

void sf_pushtextsettings(void);
void sf_poptextsettings(void);

char *sf_searchfile(char *, int, char *, char *);

void sf_chline(int x, int y, zlong c, int n);
void sf_cvline(int x, int y, zlong c, int n);
bool sf_flushdisplay(void);
void sf_getclip(int *x, int *y, int *w, int *h);
void sf_rect(zlong color, int x, int y, int w, int h);
void sf_setclip(int x, int y, int w, int h);
void sf_wpixel(int x, int y, zlong c);

void sf_InitProfile(const char *fn);
void sf_FinishProfile(void);

#ifdef WIN32
#define OS_PATHSEP ';'
#define OS_DIRSEP '\\'
#else
#define OS_PATHSEP ':'
#define OS_DIRSEP '/'
#endif

#define DEFSIZE 14

/* virtual keys */
#define VK_TAB	0x16
#define VK_INS	0x17
#define VK_PAGE_UP 0x18
#define VK_PAGE_DOWN 0x19
#define VK_DEL 0x100

/* for AIFF resampling */
typedef struct CONVstruct CONV;
struct CONVstruct {
	double ratio;
	// input
	int channels;
	int bytespersam;
	// returns num of output samples
	int (*doCONV)(CONV *, FILE *, void *, int, int);
	void (*finishCONV)(CONV *);
	int maxin, maxout;
	float *inbuf, *outbuf;
	void *aux;
};

#endif
