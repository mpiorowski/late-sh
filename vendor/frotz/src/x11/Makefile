# For GNU Make.

SOURCES = x_init.c x_input.c x_pic.c x_oldpic.c x_sample.c x_screen.c x_text.c x_blorb.c
OBJECTS = $(SOURCES:.c=.o)
HEADERS = x_frotz.h x_setup.h x_blorb.h

TARGET = frotz_x11.a

ARFLAGS = rc

ifneq ($(NO_PKGCONF), yes)
X11_CFLAGS += $(shell $(PKG_CONFIG) $(X11_PKGS) --cflags)
else
X11_CFLAGS += $(X11_CFLAGS_DEF)
endif

SOUND ?= ao

CFLAGS += -DNO_SOUND
CFLAGS += `$(PKG_CONFIG) $(X11_PKGS) --cflags`

$(TARGET): $(OBJECTS) $(ZFONTSPCF)
	$(AR) $(ARFLAGS) $@ $(OBJECTS)
	$(RANLIB) $@
	@echo "** Done with X11 interface."

clean:
	rm -f $(TARGET) $(OBJECTS) $(ZFONTSPCF)

%.o: %.c
	$(CC) $(CFLAGS) -fPIC -fpic -o $@ -c $<

.PHONY: clean
.SUFFIXES: .c .o
.DELETE_ON_ERROR:

