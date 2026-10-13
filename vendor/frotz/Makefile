# Makefile for Unix Frotz
# GNU make is required.

# Choose your preferred C compiler
#CC = gcc
#CC = clang

# Enable compiler warnings. This is an absolute minimum.
CFLAGS += -Wall -std=c99 -O3 #-Wextra

# Define your optimization flags.
#
# These are good for regular use.
#CFLAGS += -O2 -fomit-frame-pointer -falign-functions=2 -falign-loops=2 -falign-jumps=2
# These are handy for debugging.
CFLAGS += -g

# Define where you want Frotz installed
PREFIX ?= /usr/local
MANDIR ?= $(MAN_PREFIX)/man
BINDIR ?= $(PREFIX)/bin
#BINDIR ?= $(PREFIX)/games
MAN_PREFIX ?= $(PREFIX)/share
SYSCONFDIR ?= /etc

# Choose your sound support
# OPTIONS: ao, none
SOUND_TYPE ?= ao

# Choose DOS options
#DOS_NO_SOUND ?= yes
DOS_NO_BLORB ?= yes
DOS_NO_GRAPHICS ?= yes
DOS_NO_TRUECOLOUR ?= yes


##########################################################################
# The configuration options below are intended mainly for older flavors
# of Unix.  For Linux, BSD, and Solaris released since 2003, you can
# ignore this section.
##########################################################################

# Default sample rate for sound effects.
# All modern sound interfaces can be expected to support 44100 Hz sample
# rates.  Earlier ones, particularly ones in Sun 4c workstations support
# only up to 8000 Hz.
SAMPLERATE ?= 44100

# Audio buffer size in frames
BUFFSIZE ?= 4096

# Default sample rate converter type
DEFAULT_CONVERTER ?= SRC_SINC_MEDIUM_QUALITY

# Comment this out if you don't want UTF-8 support
USE_UTF8 ?= yes

# Comment this out if your machine's version of curses doesn't support color.
COLOR ?= yes

# Comment this out if your machine's version of curses doesn't support italic.
ITALIC ?= yes

# Select your chosen version of curses.  Unless something old is going
# on, ncursesw should be used because that's how UTF8 is supported.
#CURSES ?= curses
#CURSES ?= ncurses
CURSES ?= ncursesw

# This Makefile uses the pkgconf utility to get information on
# installed libraries.  If your system is missing that utility and
# cannot install it for whatever reason (usually very old systems), you
# will need to uncomment and perhaps modify some of these lines.
# I don't see any way around not having pkgconf for SDL.
#CURSES_LDLAGS += -l$(CURSES) -ltinfo
#CURSES_CFLAGS += -D_DEFAULT_SOURCE -D_XOPEN_SOURCE=600

# Uncomment this to disable Blorb support for dumb and curses interfaces.
# SDL interface always has Blorb support.
# Doing this for the X11 interface will make it use old-style graphics.
#NO_BLORB = yes


################################
# Potentially Missing Functions
#
# These are for enabling local version of certain functions which may be
# missing or behave differently from what's expected in modern system.
# If you're running on a system made in the past 20 years, you should be
# safe leaving these alone.  If not or you're using something modern,
# but very strange intended for very limited machines, you probably know
# what you're doing.  Therefore further commentary on what these
# functions do is probably not necessary.

# For missing memmove()
#NO_MEMMOVE = yes

# For missing strdup()
#NO_STRDUP = yes

# For missing strndup()
#NO_STRNDUP = yes

# For missing strrchr()
#NO_STRRCHR = yes

# For missing basename()
#NO_BASENAME = yes

# Uncomment to disable format codes for dumb interface
#DISABLE_FORMATS = yes

# If your target complains excessively about unused parameters, uncomment this
#SILENCE_UNUSED = yes

# Assorted constants
MAX_UNDO_SLOTS = 500
MAX_FILE_NAME = 80
TEXT_BUFFER_SIZE = 512
INPUT_BUFFER_SIZE = 200
STACK_SIZE = 1024

# If SDL2 is available, you most certainly will/should have pkgconf
# available, but if not, these defaults should work;
SDL_CFLAGS_DEF = -I/usr/include/libpng16 -I/usr/include/SDL2 \
	-D_REENTRANT -I/usr/include/freetype2
SDL_SOUND_CFLAGS_DEF = -I/usr/include/glib-2.0 -I/usr/include/opus \
	-I/usr/include/dbus-1.0 -I/usr/include/libinstpatch-2 -pthread \
	-D_REENTRANT -D_DEFAULT_SOURCE -D_XOPEN_SOURCE=600
SDL_LDFLAGS_DEF = -lm -lpng16 -ljpeg -lSDL2 -lfreetype -lz
SDL_SOUND_LDFLAGS_DEF = -lSDL2_mixer

# Here are some reasonable defaults for the the X11 interface.
X11_CFLAGS_DEF = -I/usr/include/libpng16
X11_LDFLAGS_DEF = -lXt -lX11 -lpng16 -ljpeg -lz -lm


#########################################################################
# This section is where Frotz is actually built.
# Under normal circumstances, nothing in this section should be changed.
#########################################################################

ifeq ($(OS),Windows_NT)
WINDOWS_BUILD := 1
CFLAGS += -DNO_STRNDUP
ONLY_TARGET := dumb
else
WINDOWS_BUILD := 0
ONLY_TARGET :=
endif

# Try to use "command -v" to locate executables.  If it's not present,
# try to use "which".
WHICH = "which"
ifeq ($(shell command -v command 2>&1),command)
WHICH = "command -v"
endif

# Determine what system we are on.
RANLIB ?= $(shell "$(WHICH)" ranlib)
AR ?= $(shell "$(WHICH)" ar)
# For now, assume !windows == unix.
OS_TYPE ?= unix
UNAME_S := $(shell uname -s)

ifeq ($(MAKECMDGOALS),tops20)
    EXPORT_TYPE = tops20
endif
ifeq ($(MAKECMDGOALS),dos)
    EXPORT_TYPE = dos
endif
ifeq ($(MAKECMDGOALS),owdos)
    EXPORT_TYPE = dos
endif
ifeq ($(MAKECMDGOALS),dosdefs)
    EXPORT_TYPE = dos
endif
ifeq ($(MAKECMDGOALS),simple)
    EXPORT_TYPE = simple
endif

RANLIB ?= ranlib

# Check to see if pkgconf or pkg-config is available.
# Both are functionally identical with the name pkg-config being deprecated.
PKG_CONFIG ?= pkgconf
PKG_CONFIG_OLD ?= pkg-config

# If we don't have pkgconf, check if pkg-config is available.
ifneq ($(shell "$(WHICH)" $(PKG_CONFIG)),)
PKG_CONFIG = $(PKG_CONFIG_OLD)
else
$(warning *** Could not find $(PKG_CONFIG).  Is $(PKG_CONFIG_OLD) available?)

# Check if pkg-config is available.
ifneq ($(shell "$(WHICH)" $(PKG_CONFIG_OLD)),)
$(warning *** Found $(PKG_CONFIG_OLD)!  Now proceeding normally.)
PKG_CONFIG = $(PKG_CONFIG_OLD)
else
# Neither pkgconf nor pkg-config found.
$(warning *** Neither $(PKG_CONFIG) nor $(PKG_CONFIG_OLD) are found!)
$(warning *** Trying default configuration settings.  If these don't work,)
$(warning *** check the configuration section of the main Makefile.)
NO_PKGCONF = yes
endif
endif


#######################
# Curses configuration.
ifeq ($(NO_PKGCONF), yes)
CURSES_LDFLAGS += -l$(CURSES)
CURSES_CFLAGS += -D_DEFAULT_SOURCE
else
# if pkgconf has $(CURSES)
ifeq ($(shell $(PKG_CONFIG) --exists $(CURSES) && echo 0),0)
# use pkgconf $(CURSES) info
PKGCONF_CURSES = yes
CURSES_LDFLAGS += $(shell $(PKG_CONFIG) $(CURSES) --libs)
CURSES_CFLAGS += $(shell $(PKG_CONFIG) $(CURSES) --cflags)
else # Otherwise, try something obvious like before.
CURSES_LDFLAGS += -l$(CURSES)
CURSES_CFLAGS += -D_DEFAULT_SOURCE
endif
endif


#############################
# OS-specific configurations.
# NetBSD
ifeq ($(UNAME_S),NetBSD)
NETBSD = yes
CFLAGS += -D_NETBSD_SOURCE -I/usr/pkg/include
LDFLAGS += -Wl,-R/usr/pkg/lib -L/usr/pkg/lib
SDL_LDFLAGS += -lexecinfo
ifeq ($(CURSES), ncursesw)
CURSES_CFLAGS += -I/usr/pkg/include/ncursesw
else
CURSES_CFLAGS += -I/usr/pkg/include/ncurses
endif
endif

# FreeBSD
ifeq ($(UNAME_S),FreeBSD)
FREEBSD = yes
CFLAGS += -I/usr/local/include -D__BSD_VISIBLE=1
LDFLAGS += -L/usr/local/lib
SDL_LDFLAGS += -lexecinfo
endif

# OpenBSD
ifeq ($(UNAME_S),OpenBSD)
OPENBSD = yes
NO_EXECINFO_H = yes
NO_UCONTEXT_H = yes
NO_IMMINTRIN_H = yes
CFLAGS += -I/usr/local/include
LDFLAGS += -L/usr/local/lib
SDL_CFLAGS += -DSDL_DISABLE_IMMINTRIN_H
SDL_LDFLAGS += -lexecinfo
endif

# Linux
ifeq ($(UNAME_S),Linux)
LINUX = yes
CFLAGS += -D_POSIX_C_SOURCE=200809L
NPROCS = $(shell grep -c ^processor /proc/cpuinfo)
endif

# macOS
ifeq ($(UNAME_S),Darwin)
MACOS = yes
# On macOS, we'll try to use Homebrew's ncurses
HOMEBREW_PREFIX ?= $(shell brew --prefix)
LDFLAGS += -L$(HOMEBREW_PREFIX)/lib

CURSES_MACOS ?= ncurses
CURSES = $(CURSES_MACOS)

CURSES_CONFIG ?= $(shell stat -f'%N' $(HOMEBREW_PREFIX)/opt/ncurses/bin/ncurses6-config)
# Reset CURSES_LDFLAGS, CURSES_CFLAGS.
CURSES_LDFLAGS = $(shell $(CURSES_CONFIG) --libs)
CURSES_CFLAGS = $(shell $(CURSES_CONFIG) --cflags)
ifeq ($(CURSES_CONFIG),)
$(error no ncurses6-config found. Install Homebrew and brew install ncurses, or set CURSES_CONFIG yourself)
endif
SDL_CFLAGS += -D_XOPEN_SOURCE
endif


# Make sure the right curses include file is included.
ifeq ($(CURSES), curses)
CURSES_DEFINE = USE_CURSES_H
else ifneq ($(findstring ncurses,$(CURSES)),)
CURSES_CFLAGS += -D_XOPEN_SOURCE_EXTENDED
CURSES_DEFINE = USE_NCURSES_H
endif

NAME = frotz
VERSION = 2.56pre
#RELEASE_NOTES = "Official release."
RELEASE_NOTES = "Development release."

# Export variables to sub-Makefiles.
export VERSION
export RELEASE_NOTES
export CC
# Modified 2026-10-09: optional late.sh Zork door support (GPL-2.0-or-later).
ifeq ($(ZORK_DOOR),1)
CFLAGS += -DLATE_ZORK
endif
export CFLAGS
export CURSES_CFLAGS
export NPROCS
export MAKEFLAGS
export AR
export RANLIB
export SYSCONFDIR
export COLOR
export ITALIC
export SOUND_TYPE
export NO_SOUND
export DESTDIR
export PREFIX
export PKG_CONFIG
export NO_PKGCONF

export CURSES_DEFINE
export SAMPLERATE
export BUFFSIZE
export DEFAULT_CONVERTER
export USE_UTF8

export SDL_PKGS
export SDL_SOUND_PKGS
export SDL_CFLAGS
export SDL_LDFLAGS
export SDL_SOUND_CFLAGS
export SDL_SOUND_LDFLAGS
export SDL_CFLAGS_DEF
export SDL_SOUND_CFLAGS_DEF
export SDL_LDFLAGS_DEF
export SDL_SOUND_LDFLAGS_DEF

export X11_PKGS
export X11_CFLAGS
export X11_CFLAGS_DEF


# If we're working from git, we have access to proper variables.
# If not, make it clear that we're working from a release.
#
GIT_DIR ?= .git
ifneq ($(and $(wildcard $(GIT_DIR)),$(shell "$(WHICH)" git)),)
GIT_HASH = $(shell git rev-parse HEAD)
GIT_HASH_SHORT = $(shell git rev-parse --short=8 HEAD)
GIT_DATE = $(shell git show -s --format=%ci)
else
GIT_HASH = 042d7bcadc1e2a8090cf737c841d8b7fc14b6eff
GIT_HASH_SHORT = 042d7bc
GIT_DATE = 2026-09-21 03:20:35 +0000
endif
export CFLAGS


# Source locations
#
SRCDIR = src
COMMON_DIR = $(SRCDIR)/common
COMMON_LIB = $(COMMON_DIR)/frotz_common.a
COMMON_DEFINES = $(COMMON_DIR)/defs.h
HASH = $(COMMON_DIR)/hash.h

MISC_DIR = $(SRCDIR)/misc
FONTS_DIR = fonts

BLORB_DIR = $(SRCDIR)/blorb
BLORB_LIB = $(BLORB_DIR)/blorblib.a

CURSES_DIR = $(SRCDIR)/curses
CURSES_LIB = $(CURSES_DIR)/frotz_curses.a

DUMB_DIR = $(SRCDIR)/dumb
DUMB_LIB = $(DUMB_DIR)/frotz_dumb.a

DOS_DIR = $(SRCDIR)/dos

X11_DIR = $(SRCDIR)/x11
X11_LIB = $(X11_DIR)/frotz_x11.a
X11_PKGS = x11 xt libpng libjpeg zlib

SDL_DIR = $(SRCDIR)/sdl
SDL_LIB = $(SDL_DIR)/frotz_sdl.a
SDL_PKGS = libpng libjpeg sdl2 freetype2 zlib
SDL_SOUND_PKGS = SDL2_mixer
SDL_LDFLAGS += -lm

SUBDIRS = $(COMMON_DIR) $(CURSES_DIR) $(X11_DIR) $(SDL_DIR) $(DUMB_DIR) $(BLORB_DIR) $(DOS_DIR) $(FONTS_DIR)
SUB_CLEAN = $(SUBDIRS:%=%-clean)

FROTZ_BIN = frotz$(EXTENSION)
DFROTZ_BIN = dfrotz$(EXTENSION)
XFROTZ_BIN = xfrotz$(EXTENSION)
SFROTZ_BIN = sfrotz$(EXTENSION)
DOS_BIN = frotz.exe

FROTZ_LIBS  = $(COMMON_LIB) $(CURSES_LIB) $(BLORB_LIB) $(COMMON_LIB)
DFROTZ_LIBS = $(COMMON_LIB) $(DUMB_LIB) $(BLORB_LIB) $(COMMON_LIB)
XFROTZ_LIBS = $(COMMON_LIB) $(X11_LIB) $(BLORB_LIB) $(COMMON_LIB)
SFROTZ_LIBS = $(COMMON_LIB) $(SDL_LIB) $(BLORB_LIB) $(COMMON_LIB)

# Tools
SNAVIG = $(MISC_DIR)/snavig.pl
SNAVIG_DIR = snavig

ifdef NO_BLORB
SOUND_TYPE = none
CURSES_SOUND = disabled
BLORB_SUPPORT = disabled
FROTZ_LIBS  = $(COMMON_LIB) $(CURSES_LIB) $(COMMON_LIB)
DFROTZ_LIBS = $(COMMON_LIB) $(DUMB_LIB) $(COMMON_LIB)
else
BLORB_SUPPORT = enabled
endif

ifeq ($(SOUND_TYPE), ao)
CURSES_SOUND_LDFLAGS += -lao -lpthread -lm \
			-lsndfile -lvorbisfile -lmodplug -lsamplerate
CURSES_SOUND = enabled
else
CURSES_SOUND = disabled
endif

# Check to see if the -Orecurse option is available.  It's nice for
# watching the output of a parallel build, but is otherwise not
# necessary.
ifneq ($(filter output-sync,$(value .FEATURES)),)
MAKEFLAGS += -Orecurse
endif

# Just the version number without the dot
DOSVER = $(shell echo $(VERSION) | sed s/\\.//g)

# Build recipes
#
curses: $(FROTZ_BIN)
ncurses: $(FROTZ_BIN)
$(FROTZ_BIN): $(FROTZ_LIBS)
	$(CC) $+ -o $@ $(LDFLAGS) $(CURSES_LDFLAGS) $(CURSES_SOUND_LDFLAGS)
	@echo "** Done building Frotz with curses interface"
	@echo "** Audio support $(CURSES_SOUND) (type $(SOUND_TYPE))"
	@echo "** Blorb support $(BLORB_SUPPORT)"

nosound: nosound_helper $(FROTZ_BIN) | nosound_helper
nosound_helper:
	$(eval SOUND_TYPE= none)
	$(eval CURSES_SOUND_LDFLAGS= )
	$(eval CURSES_SOUND= disabled)

dumb: $(DFROTZ_BIN)
$(DFROTZ_BIN): $(DFROTZ_LIBS)
	$(CC) $+ -o $@ $(LDFLAGS)
	@echo "** Done building Frotz with dumb interface."
	@echo "** Blorb support $(BLORB_SUPPORT)"

x11: $(XFROTZ_BIN)
$(XFROTZ_BIN): $(XFROTZ_LIBS)
ifneq ($(NO_PKGCONF), yes)
	$(eval X11_LDFLAGS += $(shell $(PKG_CONFIG) $(X11_PKGS) --libs))
else
	$(eval X11_LDFLAGS += $(X11_LDFLAGS_DEF))
endif
	$(CC) $+ -o $@ $(LDFLAGS) $(X11_LDFLAGS) -lm
	@echo "** Done building Frotz with X11 interface."

sdl: $(SFROTZ_BIN)
$(SFROTZ_BIN): $(SFROTZ_LIBS)
ifneq ($(NO_PKGCONF), yes)
	$(eval SDL_LDFLAGS += $(shell $(PKG_CONFIG) $(SDL_PKGS) --libs))
	$(eval SDL_SOUND_LDFLAGS = $(shell $(PKG_CONFIG) $(SDL_SOUND_PKGS) --libs))
else
	$(eval SDL_LDFLAGS += $(SDL_LDFLAGS_DEF))
	$(eval SDL_SOUND_LDFLAGS += $(SDL_SOUND_LDFLAGS_DEF))
endif
	$(CC) $+ -o $@ $(LDFLAGS) $(SDL_LDFLAGS) $(SDL_SOUND_LDFLAGS)
	@echo "** Done building Frotz with SDL interface."

sdl_nosound: sdl_nosound_helper $(SFROTZ_BIN) | sdl_nosound_helper
sdl_nosound_helper:
	$(eval SOUND_TYPE= none)
	$(eval SDL_SOUND_LDFLAGS= )
	$(eval SDL_SOUND_CFLAGS= )
	$(eval SDL_SOUND_PKGS= )


owdos: $(DOS_BIN)
$(DOS_BIN): $(COMMON_DEFINES) $(HASH)
ifneq ($(shell "$(WHICH)" wmake),)
	wmake -f Makefile.ow
else
	$(error wmake command not found.  Cannot make the DOS version)
endif

all: $(FROTZ_BIN) $(DFROTZ_BIN) $(SFROTZ_BIN) $(XFROTZ_BIN)

# Snavig-processed targets
#
snavig:
	@echo "Snavig: Change an object's shape..."
	@echo "  Snavig processes source code for compilation on very old"
	@echo "  systems which may have trouble with how the Frotz source"
	@echo "  is normally presented."
	@echo "Possible snavig-processed targets:"
	@echo "    dos    (done)"
	@echo "    tops20 (done)"
	@echo "    simple (done)"
	@echo "    its    (not even started)"
	@echo "    tops10 (not even started)"
	@echo "    tenex  (not even started)"
	@echo "    waits  (not even started)"
	@echo "That's all for now."

dos: $(COMMON_DEFINES) $(HASH)
	@rm -rf $(SNAVIG_DIR)
	@mkdir $(SNAVIG_DIR)
	@echo "** Copying support text files to $(SNAVIG_DIR)"
	@cp Makefile.tc Makefile.ow $(SNAVIG_DIR)
	@cp owbuild.bat tcbuild.bat $(SNAVIG_DIR)
	@cp INSTALL_DOS $(SNAVIG_DIR)/INSTALL.txt
	@cp doc/frotz.txt $(SNAVIG_DIR)
	@sed -i "/^OW_DOS_DIR.*/d" $(SNAVIG_DIR)/Makefile.ow
	@sed -i "/^CORE_DIR.*/d" $(SNAVIG_DIR)/Makefile.ow
	@sed -i "/^BLORB_DIR.*/d" $(SNAVIG_DIR)/Makefile.ow
	@echo "** Invoking snavig"
	@$(SNAVIG) -t dos $(COMMON_DIR) $(BLORB_DIR) $(DOS_DIR) $(SNAVIG_DIR)
	@echo "** Adding frotz.prj for building with Turbo C IDE"
	@cp frotz.prj $(SNAVIG_DIR)
	@echo "$(SNAVIG_DIR)/ now contains Frotz source code for 16-bit DOS."
	@echo "Supported compilers are:"
	@echo "  Borland Turbo C 3.00 and Borland MAKE 3.6"
	@echo "  Open Watcom C 2.0 and Open Watcom MAKE (wmake)"

tops20: $(COMMON_DEFINES) $(HASH)
	@rm -rf $(SNAVIG_DIR)
	@mkdir $(SNAVIG_DIR)
	@echo "** Invoking snavig"
	@cp Makefile.kcc $(SNAVIG_DIR)/Makefile
	@$(SNAVIG) -t tops20 $(COMMON_DIR) $(DUMB_DIR) $(SNAVIG_DIR)
	@echo "$(SNAVIG_DIR)/ now contains Frotz source code for $(EXPORT_TYPE)."
	@echo "Supported compilers are:"
	@echo "  KCC-6.620(c2l3)"

simple:	$(COMMON_DEFINES) $(HASH)
	@rm -rf $(SNAVIG_DIR)
	@mkdir $(SNAVIG_DIR)
	@echo "** Invoking snavig"
	@$(SNAVIG) -t simple $(COMMON_DIR) $(DUMB_DIR) $(SNAVIG_DIR)
	@echo "$(SNAVIG_DIR)/ now contains Frotz source code for $(EXPORT_TYPE)."
	@echo "Compile with \"cc -o dfrotz *.c\""


common_lib:	$(COMMON_LIB)
curses_lib:	$(CURSES_LIB)
x11_lib:	$(X11_LIB)
sdl_lib:	$(SDL_LIB)
dumb_lib:	$(DUMB_LIB)
blorb_lib:	$(BLORB_LIB)
dos_lib:	$(DOS_LIB)

common-lib:	$(COMMON_LIB)
curses-lib:	$(CURSES_LIB)
x11-lib:	$(X11_LIB)
sdl-lib:	$(SDL_LIB)
dumb-lib:	$(DUMB_LIB)
blorb-lib:	$(BLORB_LIB)
dos-lib:	$(DOS_LIB)

$(COMMON_LIB): $(COMMON_DEFINES) $(HASH)
	$(MAKE) -C $(COMMON_DIR)

$(CURSES_LIB): $(COMMON_DEFINES) $(HASH)
	$(MAKE) -C $(CURSES_DIR)

$(X11_LIB): $(COMMON_DEFINES) $(HASH)
	$(MAKE) -C $(X11_DIR)

$(SDL_LIB): $(COMMON_DEFINES) $(HASH) $(SDL_DIR)
	$(MAKE) -C $(SDL_DIR)

$(DUMB_LIB): $(COMMON_DEFINES) $(HASH)
	$(MAKE) -C $(DUMB_DIR)

$(BLORB_LIB): $(COMMON_DEFINES) $(HASH)
	$(MAKE) -C $(BLORB_DIR)

$(SUB_CLEAN):
	-$(MAKE) -C $(@:%-clean=%) clean


# Compile-time generated defines and strings
# Stuff common to all interfaces are generated here to allow for parallel
# compilation to work without having to wait for the common module to finish.
dosdefs: hash defs
defs: common_defines
common-defines: common_defines
common_defines: $(COMMON_DEFINES)
$(COMMON_DEFINES):
ifeq ($(wildcard $(COMMON_DEFINES)),)
	@echo "** Generating $@"
	@echo "#ifndef COMMON_DEFINES_H" > $@
	@echo "#define COMMON_DEFINES_H" >> $@
	@echo "#define VERSION \"$(VERSION)\"" >> $@
	@echo "#define RELEASE_NOTES \"$(RELEASE_NOTES)\"" >> $@

ifeq ($(EXPORT_TYPE), dos)
	@echo "#define MSDOS_16BIT" >> $@

	@echo "/* Uncomment this to disable Blorb support. */" >> $@
ifeq ($(DOS_NO_BLORB), yes)
	@echo "#define NO_BLORB" >> $@
else
	@echo "/* #define  NO_BLORB */" >>  $@
endif

	@echo "/* Uncomment this to disable sound support. */" >> $@
ifeq ($(DOS_NO_SOUND), yes)
	@echo "#define NO_SOUND" >> $@
else
	@echo "/* #define NO_SOUND */" >> $@
endif

	@echo "/* Uncomment this to disable graphics support. */" >> $@
ifeq ($(DOS_NO_GRAPHICS), yes)
	@echo "#define NO_GRAPHICS" >> $@
else
	@echo "/* #define NO_GRAPHICS */" >> $@
endif

	@echo "/* Uncomment this to disable true-colour support. */" >> $@
ifeq ($(DOS_NO_TRUECOLOUR), yes)
	@echo "#define NO_TRUECOLOUR" >> $@
else
	@echo "/* #define NO_TRUECOLOUR */" >> $@
endif


else
ifeq ($(EXPORT_TYPE), tops20)
	@echo "#ifndef TOPS20" >> $@
	@echo "#define TOPS20" >> $@
	@echo "#endif" >> $@
	@echo "#define NO_STRDUP" >> $@
	@echo "#define NO_STRNDUP" >> $@
	@echo "#define NO_BASENAME" >> $@
	@echo "#define MAXPATHLEN 39" >> $@
	@echo "#define NO_BLORB" >> $@
else

ifdef MACOS
	@echo "#define MACOS" >> $@
endif

ifeq ($(OS_TYPE), unix)
	@echo "#define UNIX" >> $@
endif
endif
endif

	@echo "#define MAX_UNDO_SLOTS $(MAX_UNDO_SLOTS)" >> $@
	@echo "#define MAX_FILE_NAME $(MAX_FILE_NAME)" >> $@
	@echo "#define TEXT_BUFFER_SIZE $(TEXT_BUFFER_SIZE)" >> $@
	@echo "#define INPUT_BUFFER_SIZE $(INPUT_BUFFER_SIZE)" >> $@
	@echo "#define STACK_SIZE $(STACK_SIZE)" >> $@
ifdef NO_BLORB
	@echo "#define NO_BLORB" >> $@
endif
ifdef NO_BASENAME
	@echo "#define NO_BASENAME" >> $@
endif
ifdef NO_STRRCHR
	@echo "#define NO_STRRCHR" >> $@
endif
ifdef NO_MEMMOVE
	@echo "#define NO_MEMMOVE" >> $@
endif
ifdef NO_STRDUP
	@echo "#define NO_STRDUP" >> $@
endif
ifdef NO_STRNDUP
	@echo "#define NO_STRNDUP" >> $@
endif
ifdef NO_UCONTEXT_H
	@echo "#define NO_UCONTEXT_H" >> $@
endif
ifdef NO_EXECINFO_H
	@echo "#define NO_EXECINFO_H" >> $@
endif

# When exporting source code rather than compiling it here, the target machine
# typically can't do UTF8.
ifeq ($(EXPORT_TYPE),)
	$(if $(findstring yes,$(USE_UTF8)), @echo "#define USE_UTF8" >> $@)
endif

ifdef FREEBSD
	@echo "#define __BSD_VISIBLE 1" >> $@
endif
ifdef DISABLE_FORMATS
	@echo "#define DISABLE_FORMATS" >> $@
endif
	@echo "#endif /* COMMON_DEFINES_H */" >> $@
endif


hash: $(HASH)
$(HASH):
ifeq ($(wildcard $(HASH)),)
	@echo "** Generating $@"
	@echo "#ifndef VERSION" > $@
	@echo "#define VERSION \"$(VERSION)\"" >> $@
	@echo "#endif" >> $@
	@echo "#ifndef RELEASE_NOTES" >> $@
	@echo "#define RELEASE_NOTES \"$(RELEASE_NOTES)\"" >> $@
	@echo "#endif" >> $@
	@echo "#define GIT_HASH \"$(GIT_HASH)\"" >> $@
	@echo "#define GIT_HASH_SHORT \"$(GIT_HASH_SHORT)\"" >> $@
	@echo "#define GIT_DATE \"$(GIT_DATE)\"" >> $@
endif


# Administrative stuff
#
install: install_frotz
install-frotz: install_frotz
install_frotz: $(FROTZ_BIN)
	mkdir -p $(DESTDIR)$(BINDIR) && test -w $(DESTDIR)$(BINDIR)
	install -c -m 755 $(FROTZ_BIN) $(DESTDIR)$(BINDIR)
	mkdir -p $(DESTDIR)$(MANDIR)/man6 && test -w $(DESTDIR)$(MANDIR)/man6
	install -m 644 doc/frotz.6 $(DESTDIR)$(MANDIR)/man6/

uninstall: uninstall_frotz
uninstall-frotz: uninstall-frotz
uninstall_frotz:
	rm -f $(DESTDIR)$(BINDIR)/frotz
	rm -f $(DESTDIR)$(MANDIR)/man6/frotz.6

install_dumb: install_dfrotz
install-dumb: install_dfrotz
install-dfrotz: install_dfrotz
install_dfrotz: $(DFROTZ_BIN)
	mkdir -p $(DESTDIR)$(BINDIR) && test -w $(DESTDIR)$(BINDIR)
	install -c -m 755 $(DFROTZ_BIN) $(DESTDIR)$(BINDIR)
	mkdir -p $(DESTDIR)$(MANDIR)/man6 && test -w $(DESTDIR)$(MANDIR)/man6
	install -m 644 doc/dfrotz.6 $(DESTDIR)$(MANDIR)/man6/

uninstall_dumb: uninstall_dfrotz
uninstall-dumb: uninstall_dfrotz
uninstall-dfrotz: uninstall_dfrotz
uninstall_dfrotz:
	rm -f $(DESTDIR)$(BINDIR)/dfrotz
	rm -f $(DESTDIR)$(MANDIR)/man6/dfrotz.6

fonts_pcf:
	$(MAKE) -C $(FONTS_DIR) pcf

install_x11: install_xfrotz
install-x11: install_xfrotz
install-xfrotz: install_xfrotz
install_xfrotz: $(XFROTZ_BIN)
	mkdir -p "$(DESTDIR)$(PREFIX)/bin" && test -w $(DESTDIR)$(BINDIR)
	mkdir -p "$(DESTDIR)$(MANDIR)/man6" && test -w $(DESTDIR)$(MANDIR)/man6
	install "$(XFROTZ_BIN)" "$(DESTDIR)$(PREFIX)/bin/"
	install -m 644 doc/xfrotz.6 "$(DESTDIR)$(MANDIR)/man6/"
	$(MAKE) -C $(FONTS_DIR) install_pcf

uninstall_x11: uninstall_xfrotz
uninstall-x11: uninstall_xfrotz
uninstall-xfrotz: uninstall_xfrotz
uninstall_xfrotz:
	rm -f "$(DESTDIR)$(PREFIX)/bin/xfrotz"
	rm -f "$(DESTDIR)$(MANDIR)/man6/xfrotz.6"
	$(MAKE) -C $(FONTS_DIR) uninstall_pcf

install-sdl: install_sfrotz
install_sdl: install_sfrotz
install-sfrotz: install_sfrotz
install_sfrotz: $(SFROTZ_BIN)
	mkdir -p $(DESTDIR)$(BINDIR) && test -w $(DESTDIR)$(BINDIR)
	install -c -m 755 $(SFROTZ_BIN) $(DESTDIR)$(BINDIR)
	mkdir -p $(DESTDIR)$(MANDIR)/man6 && test -w $(DESTDIR)$(MANDIR)/man6
	install -m 644 doc/sfrotz.6 $(DESTDIR)$(MANDIR)/man6/

uninstall-sdl: uninstall_sfrotz
uninstall_sdl: uninstall_sfrotz
uninstall-sfrotz: uninstall_sfrotz
uninstall_sfrotz:
	rm -f $(DESTDIR)$(BINDIR)/sfrotz
	rm -f $(DESTDIR)$(MANDIR)/man6/sfrotz.6

install-all:	install_all
install_all:	install_frotz install_dfrotz install_sfrotz install_xfrotz

uninstall-all:	uninstall_all
uninstall_all:	uninstall_frotz uninstall_dfrotz uninstall_sfrotz uninstall_xfrotz


dist: $(NAME)-$(VERSION).tar.gz
frotz-$(VERSION).tar.gz:
ifneq ($(and $(wildcard $(GIT_DIR)),$(shell "$(WHICH)" git)),)
	git archive --format=tgz --prefix $(NAME)-$(VERSION)/ HEAD -o $(NAME)-$(VERSION).tar.gz
	@echo $(NAME)-$(VERSION).tar.gz created.
else
	@echo "Not in a git repository or git command not found.  Cannot make an archive file."
endif

dosdist:
	@echo
	@echo "  ** Populating $(NAME)$(DOSVER) with things for the DOS Frotz precompiled zipfile."
	@echo "  ** Just add frotz.exe compiled wherever."
	@echo "  ** Then do \"zip -r $(NAME)$(DOSVER).zip $(NAME)$(DOSVER)\""
	@echo
	@mkdir -p $(NAME)$(DOSVER)
	@cp ChangeLog $(NAME)$(DOSVER)/changes.txt
	@cp frotz.lsm $(NAME)$(DOSVER)
	@cp doc/frotz.txt $(NAME)$(DOSVER)
	@cp doc/file_id.diz $(NAME)$(DOSVER)
	@cp HOW_TO_PLAY $(NAME)$(DOSVER)/howto.txt
	@cp README $(NAME)$(DOSVER)/README.txt
	@cp AUTHORS $(NAME)$(DOSVER)/AUTHORS.txt
	@cp COPYING $(NAME)$(DOSVER)/COPYING.txt
	@unix2dos -q $(NAME)$(DOSVER)/*
	@touch $(NAME)$(DOSVER)/*

owdosdist: $(NAME)$(DOSVER).zip
$(NAME)$(DOSVER).zip: dosdist $(DOS_BIN)
	@cp $(DOS_BIN) $(NAME)$(DOSVER)/$(DOS_BIN)
	@zip -r $(NAME)$(DOSVER).zip $(NAME)$(DOSVER)

clean: $(SUB_CLEAN)
	rm -rf $(NAME)-$(VERSION)
	rm -rf $(COMMON_DEFINES) $(HASH)
	rm -f FROTZ.BAK FROTZ.EXE FROTZ.LIB FROTZ.DSK *.OBJ
	rm -f FROTZ.MAP *.ERR
	rm -f frotz.map *.err

distclean: clean
	rm -f $(FROTZ_BIN) $(DFROTZ_BIN) $(SFROTZ_BIN) $(XFROTZ_BIN) $(DOS_BIN)
	rm -f a.out
	rm -rf $(NAME)src $(NAME)$(DOSVER) $(SNAVIG_DIR)
	rm -f $(NAME)*.tar.gz $(NAME)src.zip $(NAME)$(DOSVER).zip snavig.zip

help:
	@echo "Targets:"
	@echo "    frotz: (default target) the standard curses version"
	@echo "    install/uninstall: install or uninstall the curses version"
	@echo "    nosound: build the curses edition without sound support"
	@echo "    dumb:  for dumb terminals and wrapper scripts"
	@echo "    sdl:   for SDL graphics and sound"
	@echo "    x11:   for X11 graphics"
	@echo "    all:   build curses, dumb, SDL, and x11 versions"
	@echo "    owdos: Cross-compile to DOS with Open Watcom C for Unix"
	@echo "There are also install_X and uninstall_X where X is one of"
	@echo "    curses, dumb, sdl, or x11 for installing and deinstalling"
	@echo "    Frotz built with just that interface."
	@echo "Use Snavig to rework the source code for compilation on very old systems:"
	@echo "    dos:       For compilation on DOS with TurboC or OpenWatcom C"
	@echo "    tops20:    For compilation on TOPS20 with KCC"
	@echo "    simple:    For a very simplified dumb-frotz"
	@echo "Housekeeping targets:"
	@echo "    clean:     clean up files created by compilation"
	@echo "    distclean: like clean, but also delete executables"
	@echo "    dist:      create a source tarball"
	@echo ""

.SUFFIXES:
.SUFFIXES: .c .o .h

.PHONY: all clean dist dosdist curses ncurses dumb sdl hash help snavig \
	common_defines dosdefs curses_defines nosound nosound_helper \
	$(COMMON_DEFINES) $(HASH) \
	blorb_lib common_lib curses_lib dumb_lib \
	install install_dfrotz install_sfrotz install_xfrotz $(SUB_CLEAN)
	uninstall uninstall_dfrotz uninstall_sfrotz uninstall_xfrotz
