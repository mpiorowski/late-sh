# Makefile for Unix Frotz
# GNU make is required.

SOURCES = buffer.c err.c fastmem.c files.c getopt.c hotkey.c input.c \
	main.c math.c missing.c object.c process.c quetzal.c random.c \
	redirect.c screen.c sound.c stream.c table.c text.c variable.c

HEADERS = frotz.h setup.h unused.h

OBJECTS = $(SOURCES:.c=.o)

TARGET = frotz_common.a

ARFLAGS = rc

.PHONY: clean
.DELETE_ON_ERROR:

$(TARGET): $(OBJECTS)
	$(AR) $(ARFLAGS) $@ $?
	$(RANLIB) $@
	@echo "** Done with Frotz core."

clean:
	rm -f $(TARGET) $(OBJECTS)
	rm -f *.o defs.h *.O DEFS.H

%.o: %.c
	$(CC) $(CFLAGS) -fPIC -fpic -o $@ -c $<
